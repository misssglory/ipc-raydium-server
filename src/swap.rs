use anyhow::{Context, Result, anyhow};
use raydium_amm_swap::{
  consts::SOL_MINT,
  interface::{AmmPool, PoolKeys, PoolType},
};
// use solana_client_helpers::ClientResult;
use solana_client::{
  // ClientResult,
  client_error::{ClientError, ClientErrorKind},
  rpc_config::{CommitmentConfig, RpcTransactionConfig, UiTransactionEncoding},
  rpc_request::Address,
  rpc_response::{
    OptionSerializer, UiTokenAmount, UiTransactionStatusMeta,
    UiTransactionTokenBalance,
  },
};
use solana_sdk::{
  message::Message, pubkey::Pubkey, signature::Signer, transaction::Transaction,
};
use solana_signature::Signature;
use std::{
  str::FromStr,
  sync::{
    Arc,
    //  RwLock
  },
  time::{Duration, Instant},
};
use tokio::{sync::RwLock, task, time::sleep};
use tracing::{debug, error, info, warn};

use crate::{client::SwapClient, config::SwapConfig, types::SwapResult};
use spl_associated_token_account::{
  get_associated_token_address, instruction::create_associated_token_account,
};
use spl_token::{ID as TOKEN_PROGRAM_ID, instruction::initialize_account};
/// Swap executor that can be reused with different mints
#[derive(Clone)]
pub struct SwapExecutor {
  client: Arc<SwapClient>,
  config: Arc<RwLock<SwapConfig>>,
}

impl SwapExecutor {
  pub fn new(client: SwapClient, config: SwapConfig) -> Self {
    SwapExecutor {
      client: Arc::new(client),
      config: Arc::new(RwLock::new(config)),
    }
  }

  pub fn from_arc(
    client: Arc<SwapClient>,
    config: Arc<RwLock<SwapConfig>>,
  ) -> Self {
    SwapExecutor { client, config }
  }

  pub fn get_post_token_amount_u64(
    meta: Arc<UiTransactionStatusMeta>,
    owner_address: String,
    mint_address: String,
  ) -> Option<u64> {
    if let OptionSerializer::Some(post_balances) = &meta.post_token_balances {
      return SwapExecutor::get_token_amount_u64(
        post_balances,
        owner_address,
        mint_address,
      );
    }
    None
  }

  pub fn get_token_amount_u64(
    balances: &Vec<UiTransactionTokenBalance>,
    owner_address: String,
    mint_address: String,
  ) -> Option<u64> {
    let balance_entry = balances.iter().find(|b| {
      if let OptionSerializer::Some(o) = &b.owner {
        return *o == owner_address && b.mint == mint_address;
      }
      false
    })?;
    balance_entry.ui_token_amount.amount.parse::<u64>().ok()
  }

  pub async fn execute_swap(
    &self,
    input_mint: Option<&Pubkey>,
    output_mint: Option<&Pubkey>,
    amount_in: Option<u64>,
    pool_id: Option<Pubkey>,
    create_destination_ata: bool,
  ) -> Result<SwapResult> {
    info!("🚀 Starting swap execution");

    let config = self.config.read().await;
    // let config = config.clone();

    let mut input_mint = input_mint.unwrap_or(&config.input_mint);
    let output_mint = output_mint.unwrap_or(&config.output_mint);
    let amount_in = amount_in.unwrap_or(config.amount_in);
    info!("Input: {} -> Output: {}", input_mint, output_mint);
    info!("Amount in: {}, Slippage: {}%", amount_in, config.slippage * 100.0);

    // Check if both mints are WSOL
    // let sol_mint = Pubkey::from_str(SOL_MINT)?;
    // let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")?;
    // if input_mint == &sol_mint && output_mint == &sol_mint {
    //     warn!("Both input and output mints are WSOL. Changing input mint to USDC.");
    //     input_mint = &usdc_mint;
    // }

    // let is_input_token = false;
    let client = self.client.clone();
    // let output_mint = config.output_mint.clone();
    // let ata_task = task::spawn(
    //   SwapExecutor::ensure_ata_exists(
    //   client.clone(),
    //   output_mint.clone(),
    //   is_input_token,
    // )
    // );

    let destination_ata =
      get_associated_token_address(&client.keypair().pubkey(), &output_mint);
    debug!("Find ATA: {}", destination_ata.to_string());

    let pool_id = pool_id
      .unwrap_or(self.find_raydium_pool(&input_mint, &output_mint).await?);

    debug!("Find pool");

    let pool_keys: PoolKeys<AmmPool> = self
      .client
      .amm_client()
      .fetch_pools_keys_by_id(&pool_id)
      .await
      .context("Failed to fetch pool keys")?;
    debug!("Pool keys");

    let mut amount_out = 0;
    if config.slippage < 1.0 {
      let pool_info = self
        .client
        .amm_client()
        .fetch_pool_by_id(&pool_id)
        .await
        .context("Failed to fetch pool by ID")?;
      debug!("Pool info");
      let pool =
        pool_info.data.first().ok_or_else(|| anyhow!("No pool data found"))?;

      let rpc_data = self
        .client
        .amm_client()
        .get_rpc_pool_info(&pool_id)
        .await
        .context("Failed to get RPC pool info")?;
      debug!("RPC data");

      let compute_result = self
        .client
        .amm_client()
        .compute_amount_out(&rpc_data, pool, amount_in, config.slippage)
        .context("Failed to compute amount out")?;
      amount_out = compute_result.min_amount_out;
    }

    info!("Swap parameters:");
    info!("  Input amount: {}", amount_in);
    info!("  Minimum output: {}", amount_out);

    let key =
      pool_keys.data.first().ok_or_else(|| anyhow!("No pool key found"))?;

    info!("Sending swap transaction...");

    // return Err(anyhow!("test"));
    let source_ata =
      get_associated_token_address(&client.keypair().pubkey(), &input_mint);
    debug!("Source ATA: {}", source_ata.to_string());
    // return Err(anyhow!("test"));

    return Ok(SwapResult::new(
      Signature::new_unique(),
      *input_mint,
      *output_mint,
      pool_id,
      amount_in,
      0,
    ));

    match self
      .client
      .amm_client()
      .swap_amm(
        key,
        &input_mint,
        &output_mint,
        amount_in,
        amount_out,
        source_ata,
        destination_ata,
        create_destination_ata,
      )
      .await
    {
      Ok(signature) => {
        info!("Transaction sent: {}", signature);

        //TODO: Confirm ATA
        // ata_task.await.expect("ATA error");
        let client = client.clone();
        let txr = client
          .rpc_client()
          .get_transaction_with_config(
            &signature,
            RpcTransactionConfig {
              encoding: Some(UiTransactionEncoding::Json),
              commitment: Some(CommitmentConfig::confirmed()),
              ..Default::default()
            },
          )
          // .get_transaction(
          //     &signature,
          //     UiTransactionEncoding::Json
          // )
          .await;

        match txr {
          Ok(tx) => {
            if let Some(meta) = tx.transaction.meta {
              info!("Transaction meta:");
              info!("Pre token balances: {:?}", meta.pre_token_balances);
              info!("Post token balances: {:?}", meta.post_token_balances);
              info!("Logs: {:?}", meta.log_messages);
              info!("Instructions: {:?}", meta.inner_instructions);
              //TODO: Calcualte delta amount
              if let Some(balance) = SwapExecutor::get_post_token_amount_u64(
                Arc::new(meta),
                client.keypair().pubkey().to_string(),
                output_mint.to_string(),
              ) {
                amount_out = balance;
              }
              info!("Amount out for round trip: {}", amount_out);
            }
          }
          Err(err) => {
            error!("Tx error: {}", err);
          }
        }

        let result = SwapResult::new(
          signature,
          input_mint.clone(),
          output_mint.clone(),
          pool_id,
          amount_in,
          amount_out,
        );

        Ok(result)
      }
      Err(err) => {
        error!("Swap error: {}", err);
        Err(err)
      }
    }
  }

  pub async fn notify_swap(
    client: Arc<SwapClient>,
    result: Arc<SwapResult>,
  ) -> Result<()> {
    // self.wait_for_confirmation(&result.signature).await?;

    if let Some(notifier) = client.notifier() {
      let message = result.format_for_telegram()?;
      notifier.send_message(&message).await?
      // .map_err(|e| warn!("Failed to send Telegram notification: {}", e))
      // .ok();
    }

    Ok(())
  }

  /// Ensure ATA exists or create it with retry logic

  async fn ensure_ata_exists(
    // &self,
    // config: SwapConfig,
    client: Arc<SwapClient>,
    // owner: &Keypair,
    mint: Pubkey,
    is_input_token: bool,
  ) -> Result<
    Pubkey,
    // ClientError
  > {
    let start = Instant::now();

    // let config = config.read().await;
    let pk = client.keypair().pubkey();
    let ata = get_associated_token_address(&pk, &mint);
    info!("Checking ATA: {}", ata);

    // Check if ATA exists
    match client.rpc_client().get_account(&ata).await {
      Ok(_) => {
        let elapsed = start.elapsed();
        info!("ATA already exists, checked in {} ms", elapsed.as_millis());
        return Ok(ata);
      }
      Err(_) => {
        info!("Creating new ATA...");
      }
    }

    // Create ATA instruction
    let create_ata_ix =
      create_associated_token_account(&pk, &pk, &mint, &TOKEN_PROGRAM_ID);

    let mut instructions = vec![create_ata_ix];

    // If this is SOL (wrapped SOL input), we need to initialize it
    if is_input_token && mint.to_string() == SOL_MINT {
      let initialize_ix =
        initialize_account(&TOKEN_PROGRAM_ID, &ata, &mint, &pk)?;
      instructions.push(initialize_ix);
    }

    // Add priority fee for faster inclusion
    // let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(25_000);
    // instructions.insert(0, priority_fee_ix);

    // Execute with retry logic
    let mut retries = 3;

    while retries > 0 {
      let blockhash = client.rpc_client().get_latest_blockhash().await;
      // .context("Failed to get blockhash")?;

      let message = Message::new(&instructions, Some(&pk));
      let mut tx = Transaction::new_unsigned(message);

      if let Ok(bh) = blockhash {
        tx.try_sign(&[client.keypair()], bh)?;
      } else {
        // ClientError::new_with_request(ClientErrorKind::SigningError(()), request)
        return Err(anyhow!("Create ATA sign error"));
      }

      match client.rpc_client().send_and_confirm_transaction(&tx).await {
        Ok(sig) => {
          let elapsed = start.elapsed();
          info!(
            "ATA created successfully in {} ms, signature: {}",
            elapsed.as_millis(),
            sig
          );
          return Ok(ata);
        }
        Err(e) => {
          retries -= 1;
          warn!("ATA creation failed, retries left: {}, error: {}", retries, e);
          if retries > 0 {
            sleep(Duration::from_millis(100)).await;
          }
          // else {
          // e
          // ClientError(e)
          // }
        }
      }
    }

    Err(anyhow!("Failed to create ATA after retries"))
    // ClientError::new_with_request(kind, request)
  }

  pub async fn check_balance(&self, mint: &Address) -> Result<u64> {
    let balance_start = Instant::now();

    // let is_input_token = false;
    //TODO: Check ATA without creation
    // let owner_ata_input = self.ensure_ata_exists(mint, is_input_token).await;
    // let config = self.config.read().unwrap();
    let ata =
      get_associated_token_address(&self.client.keypair().pubkey(), mint);
    let account_info = self.client.rpc_client().get_account(&ata).await;
    // let token_account: Account = spl_token::state::Account::unpack_from_slice(&account_info.data)?;

    if let Ok(acc) = account_info {
      info!(
        "Balance check: {}. In {} ms",
        acc.lamports,
        balance_start.elapsed().as_millis(),
      );
      return Ok(acc.lamports);
    }
    // if let Ok(owner_ata_input) = owner_ata_input {
    //     let balance_response = self
    //         .client
    //         .rpc_client()
    //         .get_token_account_balance(&owner_ata_input)
    //         .await;
    //     // balance_response
    //     if let Ok(balance) = balance_response {
    //         info!(
    //             "Check balance executed in {} ms",
    //             balance_start.elapsed().as_millis()
    //         );
    //         return Ok(balance);
    //     }
    //     // debug!("Balance: {}", balance_response);
    //     // if let Ok(balance) = balance_response {
    //     //     info!("Input token balance: {}", balance.ui_amount_string);
    //     //     if balance.amount.parse::<u64>().unwrap_or(0) < self.config.amount_in {
    //     //         return Err(anyhow!("Insufficient balance for swap"));
    //     //     }
    //     // }
    // } else {
    // return Err(anyhow!("Check balance error"));
    // Err(ClientError(()))
    // ClientError()
    // }
    Err(anyhow!("Check balance owner ATA error"))
  }

  pub async fn execute_round_trip_with_notification(
    &self,
    input_mint: Option<&Pubkey>,
    output_mint: Option<&Pubkey>,
    sleep_millis: u64,
  ) -> Result<()> {
    // let config = self.config.read().await;
    // let input_mint = input_mint.unwrap_or(&config.input_mint);
    // let output_mint = output_mint.unwrap_or(&config.output_mint);
    let buy_result =
      self.execute_swap(input_mint, output_mint, None, None, true).await;

    if let Ok(buy) = buy_result {
      let client = self.client.clone();
      let buy = Arc::new(buy);
      // let notify_buy_task =
      //   task::spawn(SwapExecutor::notify_swap(client, buy.clone()));
      let notify_buy_result =
        SwapExecutor::notify_swap(client, buy.clone()).await?;

      if sleep_millis > 0 {
        let sleep_after_buy = std::time::Duration::from_millis(sleep_millis); // Average block time
        std::thread::sleep(sleep_after_buy);
      }

      // let config = self.config.read().await;

      // let balance = 1;
      // while balance > 0 { }

      //   let balance_result = self.check_balance(&config.output_mint).await;
      // if let Ok(balance) = balance_result {
      let balance = buy.amount_out;

      // std::mem::swap(&mut self.config.input_mint, &mut self.config.output_mint);
      // {
      //     let mut config = self.config.write().await;
      //     config.change_direction(balance);
      // }
      let buy = buy.clone();
      let sell_result = self
        .execute_swap(
          Some(&buy.output_mint),
          Some(&buy.input_mint),
          Some(balance),
          Some(buy.pool_id),
          false,
        )
        .await;
      // if let Ok(sell) = sell_result {
      // }

      // notify_buy_task.await;
      match sell_result {
        Ok(sell) => {
          let client = self.client.clone();
          SwapExecutor::notify_swap(client, Arc::new(sell)).await;
          return Ok(());
        }
        Err(err) => {
          return Err(anyhow!("Round trip sell error: {}", err));
        }
      }
      // return Err(anyhow!("Round trip sell error"));
    }
    return Err(anyhow!("Round trip balance error"));
    Err(anyhow!("Round trip buy error"))
  }

  async fn find_raydium_pool(
    &self,
    input_mint: &Pubkey,
    output_mint: &Pubkey,
  ) -> Result<Pubkey> {
    let start = Instant::now();

    info!("Searching for pool with mints: {} -> {}", input_mint, output_mint);

    let all_mint_pools = self
      .client
      .amm_client()
      .fetch_pool_info(
        &input_mint.to_string(),
        &output_mint.to_string(),
        &PoolType::Standard,
        None,
        None,
        None,
        None,
      )
      .await
      .context("Failed to fetch pool info")?;

    let elapsed = start.elapsed();
    info!("Pool search completed in {} ms", elapsed.as_millis());

    if all_mint_pools.is_empty() {
      return Err(anyhow!("No Raydium pool found for token pair"));
    }

    let first_pool = all_mint_pools.first().unwrap();
    let pool_id =
      Pubkey::from_str(&first_pool.id).context("Failed to parse pool ID")?;

    info!("Found pool: {}", pool_id);
    Ok(pool_id)
  }

  async fn wait_for_confirmation(&self, signature: &Signature) -> Result<()> {
    let timeout = Duration::from_secs(30);
    let start = Instant::now();

    info!("Waiting for transaction confirmation...");

    while start.elapsed() < timeout {
      match self.client.rpc_client().get_signature_status(signature).await {
        Ok(Some(_)) => {
          info!("Transaction confirmed in {} ms", start.elapsed().as_millis());
          return Ok(());
        }
        _ => sleep(Duration::from_millis(500)).await,
      }
    }

    warn!("Transaction not confirmed within timeout");
    Ok(())
  }

  /// Get quote for a swap (without executing)
  pub async fn get_quote(
    &self,
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_in: u64,
  ) -> Result<u64> {
    info!("Getting quote for swap");

    let pool_id = self.find_raydium_pool(&input_mint, &output_mint).await?;

    let pool_info = self
      .client
      .amm_client()
      .fetch_pool_by_id(&pool_id)
      .await
      .context("Failed to fetch pool by ID")?;

    let rpc_data = self
      .client
      .amm_client()
      .get_rpc_pool_info(&pool_id)
      .await
      .context("Failed to get RPC pool info")?;

    let pool =
      pool_info.data.first().ok_or_else(|| anyhow!("No pool data found"))?;

    let compute_result = self
      .client
      .amm_client()
      .compute_amount_out(
        &rpc_data,
        pool,
        amount_in,
        self.config.read().await.slippage,
      )
      .context("Failed to compute amount out")?;

    Ok(compute_result.min_amount_out)
  }
}
