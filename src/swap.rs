use anyhow::{Context, Result, anyhow};
use raydium_amm_swap::{
  consts::SOL_MINT,
  interface::{AmmPool, ClmmPool, ClmmSinglePoolInfo, PoolKeys, PoolType},
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
  cmp::Ordering,
  collections::HashMap,
  str::FromStr,
  sync::Arc,
  time::{Duration, Instant},
};
use tokio::{sync::RwLock, task, time::sleep};
use tracing::{debug, error, info, warn};

use crate::{
  amm::client::RpcPoolInfo, client::SwapClient, config::SwapConfig,
  types::SwapResult,
};
use spl_associated_token_account::{
  get_associated_token_address, instruction::create_associated_token_account,
};
use spl_token::{ID as TOKEN_PROGRAM_ID, instruction::initialize_account};

use compare::{Compare, natural};

use std::cmp::Ordering::{Equal, Greater, Less};

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

  pub async fn config_mut(
    &self,
  ) -> tokio::sync::RwLockWriteGuard<SwapConfig> {
    self.config.write().await
  }

  pub fn get_post_token_amount_u64(
    meta: Arc<UiTransactionStatusMeta>,
    owner_address: String,
    mint_address: String,
  ) -> Result<u64> {
    if let OptionSerializer::Some(post_balances) = &meta.post_token_balances {
      return SwapExecutor::get_token_amount_u64(
        post_balances,
        owner_address,
        mint_address,
      );
    }
    Err(anyhow!("Post balance error"))
  }

  pub fn get_pre_token_amount_u64(
    meta: Arc<UiTransactionStatusMeta>,
    owner_address: String,
    mint_address: String,
  ) -> Result<u64> {
    if let OptionSerializer::Some(post_balances) = &meta.pre_token_balances {
      return SwapExecutor::get_token_amount_u64(
        post_balances,
        owner_address,
        mint_address,
      );
    }
    Err(anyhow!("Pre balance error"))
  }

  pub fn get_token_amount_u64(
    balances: &Vec<UiTransactionTokenBalance>,
    owner_address: String,
    mint_address: String,
  ) -> Result<u64> {
    let balance_entry = balances.iter().find(|b| {
      if let OptionSerializer::Some(o) = &b.owner {
        return *o == owner_address && b.mint == mint_address;
      }
      false
    });
    match balance_entry {
      Some(balance) => Ok(balance.ui_token_amount.amount.parse::<u64>()?),
      None => Err(anyhow!("Balance not found")),
    }
  }

  // Helper function to convert a balance vec to HashMap for easier lookup
  fn balances_to_hashmap(
    balances: &Vec<UiTransactionTokenBalance>,
    owner: &String,
  ) -> HashMap<String, u64> {
    let mut map = HashMap::new();

    for balance in balances {
      if let OptionSerializer::Some(balance_owner) = &balance.owner {
        if balance_owner == owner {
          if let Ok(amount) = balance.ui_token_amount.amount.parse::<u64>() {
            map.insert(balance.mint.clone(), amount);
          }
        }
      }
    }
    map
  }

  pub fn get_token_balance_deltas(
    meta: Arc<UiTransactionStatusMeta>,
    owner: &String,
  ) -> HashMap<String, i64> {
    let mut deltas = HashMap::new();

    // Get pre balances
    let pre_balances = match &meta.pre_token_balances {
      OptionSerializer::Some(balances) => {
        SwapExecutor::balances_to_hashmap(balances, owner)
      }
      OptionSerializer::None => HashMap::new(),
      OptionSerializer::Skip => HashMap::new(),
    };

    // Get post balances
    let post_balances = match &meta.post_token_balances {
      OptionSerializer::Some(balances) => {
        SwapExecutor::balances_to_hashmap(balances, owner)
      }
      OptionSerializer::None => HashMap::new(),
      OptionSerializer::Skip => HashMap::new(),
    };

    // Collect all unique mints from both pre and post balances
    let all_mints: std::collections::HashSet<_> =
      pre_balances.keys().chain(post_balances.keys()).collect();

    // Calculate deltas for each mint
    for mint in all_mints {
      let pre_amount = pre_balances.get(mint).copied().unwrap_or(0);
      let post_amount = post_balances.get(mint).copied().unwrap_or(0);

      // Calculate delta (post - pre) as i64 to handle negative values
      let delta = post_amount as i64 - pre_amount as i64;

      // Only include non-zero deltas if you want (optional)
      if delta != 0 {
        deltas.insert(mint.clone(), delta);
      }
    }

    deltas
  }

  pub async fn execute_swap(
    &self,
    input_mint: Option<&Pubkey>,
    output_mint: Option<&Pubkey>,
    amount_in: Option<u64>,
    pool_id: Option<Pubkey>,
    amount_out: u64,
    create_destination_ata: bool,
  ) -> Result<SwapResult> {
    info!("🚀 Starting swap execution");

    let config = self.config.read().await;

    // let mut check_ata_task: Result<(Pubkey, bool)> = Err(anyhow!("Create ATA task"));
    let mut check_ata_task: Option<task::JoinHandle<Result<(Pubkey, bool)>>> =
      None;
    let output_mint = output_mint.unwrap_or(&config.output_mint);
    if create_destination_ata {
      check_ata_task = Some(task::spawn(SwapExecutor::ensure_ata_exists(
        self.client.clone(),
        output_mint.clone(),
        true,
      )));
    }
    let input_mint = input_mint.unwrap_or(&config.input_mint);
    let amount_in = amount_in.unwrap_or(config.amount_in);
    info!("Input: {} -> Output: {}", input_mint, output_mint);
    info!("Amount in: {}, Slippage: {}%", amount_in, config.slippage * 100.0);

    let client = self.client.clone();

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

    let create_destination_ata = match check_ata_task {
      None => false,
      Some(ata_task) => ata_task.await??.1,
    };
    // let mut amount_out = 0;
    // if config.slippage < 1.0 {
    //   let pool_info = self
    //     .client
    //     .amm_client()
    //     .fetch_pool_by_id(&pool_id)
    //     .await
    //     .context("Failed to fetch pool by ID")?;
    //   debug!("Pool info");
    //   let pool =
    //     pool_info.data.first().ok_or_else(|| anyhow!("No pool data found"))?;

    //   let rpc_data = self
    //     .client
    //     .amm_client()
    //     .get_rpc_pool_info(&pool_id)
    //     .await
    //     .context("Failed to get RPC pool info")?;
    //   debug!("RPC data");

    //   let compute_result = self
    //     .client
    //     .amm_client()
    //     .compute_amount_out(&rpc_data, pool, amount_in, config.slippage)
    //     .context("Failed to compute amount out")?;
    //   amount_out = compute_result.min_amount_out;
    // }

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

    // return Ok(SwapResult::new(
    //   Signature::new_unique(),
    //   input_mint.clone(),
    //   output_mint.clone(),
    //   pool_id,
    //   amount_in,
    //   0,
    // ));

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

        let mut amount_out = amount_out;
        let mut amount_in = amount_in;

        match txr {
          Ok(tx) => {
            if let Some(meta) = tx.transaction.meta {
              debug!("Transaction meta:");
              debug!("Pre token balances: {:?}", meta.pre_token_balances);
              debug!("Post token balances: {:?}", meta.post_token_balances);
              debug!("Logs: {:?}", meta.log_messages);
              debug!("Instructions: {:?}", meta.inner_instructions);
              let meta_ref = Arc::new(meta);
              let owner = client.keypair().pubkey().to_string();
              // let pre_balance = SwapExecutor::get_pre_token_amount_u64(
              //   meta_ref.clone(),
              //   owner.clone(),
              //   output_mint_str.clone(),
              // )?;
              // let post_balance = SwapExecutor::get_post_token_amount_u64(
              //   meta_ref.clone(),
              //   owner,
              //   output_mint_str,
              // )?;
              // amount_out = post_balance - pre_balance;
              let balance_deltas = SwapExecutor::get_token_balance_deltas(
                meta_ref.clone(),
                &owner,
              );
              amount_out = std::cmp::max(
                *balance_deltas.get(&output_mint.to_string()).unwrap_or(&0),
                0,
              ) as u64;
              amount_in = std::cmp::max(
                -*balance_deltas.get(&input_mint.to_string()).unwrap_or(&0),
                0,
              ) as u64;
              info!(
                "Amount out for round trip: {}. Amount in: {}",
                amount_out, amount_in
              );
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
    if let Some(notifier) = client.notifier() {
      debug!("Before format");
      let message = result.format_for_telegram()?;
      notifier.send_message(&message).await?
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
    (Pubkey, bool),
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
        return Ok((ata, false));
      }
      Err(_) => {
        info!("Creating new ATA...");
        return Ok((ata, true));
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
          return Ok((ata, false));
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
    let buy =
      self.execute_swap(input_mint, output_mint, None, None, 0, true).await?;

    let buy = Arc::new(buy);
    if let Err(err) =
      SwapExecutor::notify_swap(self.client.clone(), buy.clone()).await
    {
      error!("Notify swap error: {}", err);
    }

    let config = self.config.read().await;

    std::thread::sleep(Duration::from_millis(sleep_millis));

    let target_quote =
      (buy.amount_out as f64 / config.min_profit_percent) as u64;
    //TODO: Nofity result slippage
    let quote = self
      .quote_loop(
        Some(buy.pool_id),
        None,
        None,
        Some(buy.amount_in),
        None,
        Some(target_quote),
        1500,
        Less,
      )
      .await;

    let buy = buy.clone();
    let sell_result = self
      .execute_swap(
        Some(&buy.output_mint),
        Some(&buy.input_mint),
        Some(buy.amount_out),
        Some(buy.pool_id),
        // buy.amount_out,
        0,
        false,
      )
      .await?;

    // let client = client.clone();
    let sell = Arc::new(sell_result);
    if let Err(err) =
      SwapExecutor::notify_swap(self.client.clone(), sell.clone())
        .await
    {
      error!("Notify error: {}", err);
    }

    let target_quote = (buy.amount_out as f64) as u64;
    //TODO: Nofity result slippage
    let quote = self
      .quote_loop(
        Some(buy.pool_id),
        None,
        None,
        Some(sell.amount_in),
        None,
        Some(target_quote),
        1500,
        Greater,
      )
      .await;

    Ok(())
    // match sell_result {
    //   Ok(sell) => {
    //     let client = self.client.clone();
    //     SwapExecutor::notify_swap(client, Arc::new(sell)).await?;
    //     return Ok(());
    //   }
    //   Err(err) => {
    //     return Err(anyhow!("Round trip sell error: {}", err));
    //   }
    // }
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

  pub async fn quote_loop(
    &self,
    pool_id: Option<Pubkey>,
    input_mint: Option<&Pubkey>,
    output_mint: Option<&Pubkey>,
    amount_in: Option<u64>,
    timelimit_millis: Option<u64>,
    target_quote: Option<u64>,
    poll_period_millis: u64,
    cmp_order: Ordering,
  ) -> Result<u64> {
    let mut quote = 0;
    let config = self.config.read().await;
    let amount_in = amount_in.unwrap_or(config.amount_in);
    let timelimit_millis =
      timelimit_millis.unwrap_or(config.timelimit_seconds * 1000) as u128;

    let pool_id = match pool_id {
      Some(p_id) => p_id,
      None => {
        let input_mint = input_mint.unwrap_or(&config.input_mint);
        let output_mint = output_mint.unwrap_or(&config.output_mint);
        let pid = self.find_raydium_pool(input_mint, output_mint).await?;
        pid
      }
    };
    let start_time = Instant::now();
    // let timelimit = std::time::Duration::from_millis(timelimit_millis);
    let pool_info = self
      .client
      .amm_client()
      .fetch_pool_by_id(&pool_id)
      .await
      .context("Failed to fetch pool by ID")?;
    let cmp = natural();
    loop {
      let step_time = Instant::now();
      quote =
        self.get_quote(amount_in, Some(0.0), &pool_info, &pool_id).await?;

      if start_time.elapsed().as_millis() > timelimit_millis {
        info!("Timelimit {} ms reached! Quote: {}", timelimit_millis, quote);
        if let Some(tq) = target_quote {
          info!("Target: {} Diff: {}", tq, (quote as f64 / tq as f64 - 1.0));
        } else {
          info!("No target quote was provided");
        }
        break;
      }


      if let Some(tq) = target_quote {
        info!(
          "Quote {} Target: {} Diff: {} elapsed: {} ms",
          quote,
          tq,
          (quote as f64 / tq as f64 - 1.0),
          start_time.elapsed().as_millis()
        );
        if cmp.compare(&quote, &tq) == cmp_order {
          info!("Target quote reached!");
          break;
        }
      }

      let elapsed_millis = step_time.elapsed().as_millis() as u64;
      std::thread::sleep(Duration::from_millis(
        std::cmp::max(poll_period_millis, elapsed_millis) - elapsed_millis,
      ));
    }
    Ok(quote)
  }

  /// Get quote for a swap (without executing)
  pub async fn get_quote(
    &self,
    amount_in: u64,
    slippage: Option<f64>,
    pool_info: &ClmmSinglePoolInfo,
    // pool_id: Option<&Pubkey>,
    pool_id: &Pubkey,
  ) -> Result<u64> {
    info!("Getting quote for swap");

    let slippage = if let Some(slip) = slippage {
      slip
    } else {
      self.config.read().await.slippage
    };

    //TODO: iterate over all pools
    let first_pool = if let Some(poopool) = pool_info.data.first() {
      poopool
    } else {
      return Err(anyhow!("First pool not found"));
    };
    // let pool_id = pool_id.unwrap_or(&Pubkey::from_str(&first_pool.id)?);
    let rpc_data = self
      .client
      .amm_client()
      .get_rpc_pool_info(&pool_id)
      .await
      .context("Failed to get RPC pool info")?;

    let compute_result = self
      .client
      .amm_client()
      .compute_amount_out(&rpc_data, first_pool, amount_in, slippage)
      .context("Failed to compute amount out")?;

    debug!("{}", compute_result);
    Ok(compute_result.min_amount_out)

    // let compute_result = self
    //   .client
    //   .amm_client()
    //   .compute_amount_in(&pool_data.1, first_pool, amount_in / 1000, slippage)
    //   .context("Failed to compute amount out")?;

    // debug!("{}", compute_result);

    // Ok(compute_result.max_amount_in)
  }
}
