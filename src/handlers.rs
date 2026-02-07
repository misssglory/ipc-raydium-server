use std::sync::Arc;

use crate::{SwapExecutor, SwapResult, quoter::Quoter};
use chrono::DateTime;
use solana_program::pubkey::Pubkey;
use raydium_amm_swap::consts::SOL_MINT;
use solana_signature::Signature;
use tracing::info;

pub async fn round_trip(
  executor: Arc<SwapExecutor>,
  address: Option<&Pubkey>,
  ) {
  let result = executor
    .execute_round_trip_with_notification(None, address, 500)
    .await;
  let mut config = executor.config_mut().await;
  config.min_profit_percent = 1.0 + (config.min_profit_percent - 1.0) * 2.0;
  config.timelimit_seconds = config.timelimit_seconds * 4;
}

pub async fn test_quote(
  executor: Arc<SwapExecutor>,
  address: Option<&Pubkey>,
  i: u64,
) {
  let mut input_mint = Pubkey::from_str_const(SOL_MINT);
  let mut output_mint = address.unwrap().clone();
  let pool_id =
    executor.find_raydium_pool(&input_mint, &output_mint).await.unwrap();
  let pool_info = executor
    .client()
    .amm_client()
    .fetch_pool_by_id(&pool_id)
    .await
    // .context("Failed to fetch pool by ID")
    .unwrap();

  let input_mint_is_a_mint =
    pool_info.data.first().unwrap().mint_a.address == input_mint.to_string();
  let fee = pool_info.data.first().unwrap().fee_rate.unwrap();
  let mut amount_in = executor.config_mut().await.amount_in;
  let mut amount_out = executor
    .get_quote(
      &pool_info,
      &pool_id,
      amount_in,
      Some(0.0),
      (i % 2 == 1) ^ input_mint_is_a_mint,
    )
    .await
    .unwrap();
  if i % 2 == 1 {
    std::mem::swap(&mut input_mint, &mut output_mint);
    // let tmp = amount_in;
    // amount_in = (amount_out as f64 * (1.0 + fee)) as u64;
    // amount_out = (tmp as f64 * (1.0 - fee)) as u64;
  }

  info!("Initial amount in: {} Mint: {}", amount_in, input_mint.to_string());
  info!("Initial amount out: {} Mint: {}", amount_out, output_mint.to_string());
  let mut mock_swap_result = SwapResult {
    signature: Signature::new_unique(),
    input_mint: input_mint,
    output_mint: output_mint,
    pool_id: pool_id,
    amount_in: amount_in,
    amount_out: amount_out,
    jupiter_link: "".to_string(),
    explorer_link: "".to_string(),
    timestamp: DateTime::from_timestamp_secs(17000000).unwrap(),
    amm_input_mint_reserve: None,
    amm_output_mint_reserve: None,
  };

  let quoter = Quoter::new(
    pool_id,
    executor.client().clone(),
    mock_swap_result.input_mint,
    None,
  )
  .await
  .unwrap();

  // let target_pct = if i % 2 == 0 { 1.01 } else { 0.99 };
  let target_pct = 1.002;
  quoter.quote_loop(target_pct, 10000u128).await.unwrap();

  let (mock_quote_params, pool_info) =
    executor.get_quote_params(&mock_swap_result, true).await.unwrap();

  // mock_swap_result.amount_out = amount_out;

  let result = executor
    .quote_loop(
      Some(pool_id),
      &mock_quote_params,
      // Some(&Pubkey::from_str(SOL_MINT).unwrap()),
      Some(&pool_info),
      None,
      None,
      // Some(&address),
      // Some(address),
      None,
      1500,
    )
    .await;
  info!("{:?}", result);
}
