use anyhow::{Context, Result};
use dotenv::dotenv;
use solana_program::pubkey::Pubkey;
use std::{env, str::FromStr};

#[derive(Debug, Clone)]
pub struct SwapConfig {
    pub rpc_endpoints: Vec<String>,
    pub keypair_path: String,
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub amount_in: u64,
    pub slippage: f64,
    pub telegram_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub user_wallet: Option<String>, // For tracking
}

impl SwapConfig {
    pub fn from_env() -> Result<Self> {
        dotenv().ok();

        let rpc_endpoints = env::var("RPC_ENDPOINTS")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        Ok(SwapConfig {
            rpc_endpoints,
            keypair_path: env::var("KEYPAIR_PATH").context("KEYPAIR_PATH not set")?,
            input_mint: Pubkey::from_str(
                &env::var("INPUT_MINT")
                    .unwrap_or_else(|_| "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()),
            )?,
            output_mint: Pubkey::from_str(
                &env::var("OUTPUT_MINT")
                    .context("OUTPUT_MINT must be provided")?,
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
            telegram_chat_id: env::var("CHAT_ID").ok(),
            user_wallet: env::var("USER_WALLET").ok(),
        })
    }

    pub fn from_params(
        rpc_endpoints: Vec<String>,
        keypair_path: String,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount_in: u64,
        slippage: f64,
        telegram_token: Option<String>,
        telegram_chat_id: Option<String>,
        user_wallet: Option<String>,
    ) -> Self {
        SwapConfig {
            rpc_endpoints,
            keypair_path,
            input_mint,
            output_mint,
            amount_in,
            slippage,
            telegram_token,
            telegram_chat_id,
            user_wallet,
        }
    }
}