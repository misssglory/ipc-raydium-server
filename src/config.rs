use anyhow::{Context, Result};
use dotenv::dotenv;
use raydium_amm_swap::consts::SOL_MINT;
use solana_program::pubkey::Pubkey;
use std::{env, str::FromStr};
use anyhow::anyhow;
use std::sync::Arc;

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

        Ok(SwapConfig {
            rpc_endpoints,
            keypair: keypair,
            input_mint: Pubkey::from_str(
                &env::var("INPUT_MINT")
                    .unwrap_or_else(|_| SOL_MINT.to_string()),
            )?,
            output_mint: Pubkey::from_str(
                &env::var("OUTPUT_MINT")
                    .unwrap_or_else(|_| "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()),
            )?,
            amount_in: env::var("AMOUNT_IN")
                .unwrap_or_else(|_| "1000000".to_string())
                .parse()
                .context("Invalid AMOUNT_IN")?,
            slippage: env::var("SLIPPAGE")
                .unwrap_or_else(|_| "0.01".to_string())
                .parse()
                .context("Invalid SLIPPAGE")?,
            telegram_token: env::var("TG_TOKEN").ok(),
            telegram_chat_id: env::var("TG_CHAT_ID").ok(),
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
        }
    }

    pub fn change_direction(&mut self, amount_in: u64) {
        std::mem::swap(& mut self.input_mint, &mut self.output_mint);
        self.amount_in = amount_in;
        // let temp = self.input_mint;
        // self.input_mint = self.output_mint;
        // self.output_mint = temp;
    }
}