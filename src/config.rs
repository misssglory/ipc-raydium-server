use anyhow::anyhow;
use anyhow::{Context, Result};
use dotenv::dotenv;
use raydium_amm_swap::consts::SOL_MINT;
use solana_client::rpc_config::CommitmentConfig;
use solana_program::pubkey::Pubkey;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use std::sync::Arc;
use std::{env, str::FromStr};

#[derive(Debug, Clone)]
pub struct SwapConfig {
  pub rpc_endpoints: Vec<String>,
  pub keypair: String,
  pub input_mint: Pubkey,
  pub output_mint: Pubkey,
  pub amount_in: u64,
  pub slippage: f64,
  pub telegram_token: Option<String>,
  pub telegram_chat_id: Option<String>,
  pub commitment_config: CommitmentConfig,
  pub min_profit_percent: f64,
  pub stop_loss_percent: f64,
  pub timelimit_seconds: u64,
}

impl SwapConfig {
  pub fn from_env() -> Result<Self> {
    dotenv().ok();

    let rpc_endpoints = env::var("RPC_ENDPOINTS")
      .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string())
      .split(',')
      .map(|s| s.trim().to_string())
      .collect();

    let keypair = env::var("KEYPAIR_PATH").context("KEYPAIR_PATH not set")?;
    let commitment_str = env::var("COMMITMENT")
      .unwrap_or_else(|_| "confirmed".to_string())
      .to_lowercase();

    let commitment_config = match commitment_str.as_str() {
      "processed" => CommitmentConfig::processed(),
      "confirmed" => CommitmentConfig::confirmed(),
      "finalized" => CommitmentConfig::finalized(),
      _ => {
        eprintln!(
          "Unknown commitment level '{}', defaulting to 'confirmed'",
          commitment_str
        );
        CommitmentConfig::confirmed()
      }
    };

    let amount_in: u64 = if let Ok(amount_in_str) = env::var("AMOUNT_IN") {
      amount_in_str.parse().context(
        "Invalid AMOUNT_IN value, must be a valid integer number of lamports",
      )?
    } else {
      let amount_in_sol: f64 = env::var("AMOUNT_IN_SOL")
        .unwrap_or_else(|_| "1.0".to_string())
        .parse()
        .context(
            "Invalid AMOUNT_IN_SOL value, must be a valid number (e.g., 0.5, 1.23)",
        )?;
      (amount_in_sol * LAMPORTS_PER_SOL as f64) as u64
    };

    Ok(SwapConfig {
      rpc_endpoints,
      keypair: keypair,
      input_mint: Pubkey::from_str(
        &env::var("INPUT_MINT").unwrap_or_else(|_| SOL_MINT.to_string()),
      )?,
      output_mint: Pubkey::from_str(&env::var("OUTPUT_MINT").unwrap_or_else(
        |_| "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
      ))?,
      //TODO: Find actial decimals for input mint
      amount_in,
      slippage: env::var("SLIPPAGE")
        .unwrap_or_else(|_| "0.01".to_string())
        .parse()
        .context("Invalid SLIPPAGE")?,
      telegram_token: env::var("TG_TOKEN").ok(),
      telegram_chat_id: env::var("TG_CHAT_ID").ok(),
      commitment_config,
      min_profit_percent: env::var("TAKE_PROFIT")
        .unwrap_or_else(|_| "1.01".to_string())
        .parse()
        .context("Invalid take profit")?,
      stop_loss_percent: env::var("STOP_LOSS")
        .unwrap_or_else(|_| "1.01".to_string())
        .parse()
        .context("Invalid stop loss")?,
      timelimit_seconds: env::var("TIMELIMIT_SECONDS")
        .unwrap_or_else(|_| "13".to_string())
        .parse()
        .context("Invalid timelimit")?,

    })
  }

  pub fn from_params(
    rpc_endpoints: Vec<String>,
    keypair: String,
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_in: u64,
    slippage: f64,
    telegram_token: Option<String>,
    telegram_chat_id: Option<String>,
    commitment_config: CommitmentConfig,
  ) -> Self {
    SwapConfig {
      rpc_endpoints,
      keypair,
      input_mint,
      output_mint,
      amount_in,
      slippage,
      telegram_token,
      telegram_chat_id,
      commitment_config,
      min_profit_percent: 1.01,
      stop_loss_percent: 1.01,
      timelimit_seconds: 13,
    }
  }

  pub fn change_direction(&mut self, amount_in: u64) {
    std::mem::swap(&mut self.input_mint, &mut self.output_mint);
    self.amount_in = amount_in;
    // let temp = self.input_mint;
    // self.input_mint = self.output_mint;
    // self.output_mint = temp;
  }
}
