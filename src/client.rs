use anyhow::{Context, Result, anyhow};
// use raydium_amm_swap::amm::client::AmmSwapClient;
use crate::amm::client::AmmSwapClient;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::CommitmentConfig};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, read_keypair_file},
};
use std::sync::Arc;
use tracing::info;

use crate::{config::SwapConfig, telegram::TelegramNotifier};

/// Main client that holds initialized components and can be reused
#[derive(Clone)]
pub struct SwapClient {
    inner: Arc<SwapClientInner>,
}

struct SwapClientInner {
    rpc_client: RpcClient,
    amm_client: AmmSwapClient,
    kp: Keypair,
    notifier: Option<TelegramNotifier>,
}

impl SwapClient {
    /// Create a new SwapClient from configuration
    pub async fn new(config: &SwapConfig) -> Result<Self> {
        info!("Initializing SwapClient...");

        // Initialize RPC client with first endpoint
        let rpc_url = config
            .rpc_endpoints
            .first()
            .context("No RPC endpoints provided")?
            .clone();

        let rpc_client = RpcClient::new_with_commitment(rpc_url, config.commitment_config);

        // Load keypair
        let keypair = read_keypair_file(&config.keypair)
            .map_err(|e| anyhow!("Failed to read keypair file: {}", e))?;

        // info!("Loaded keypair: {}", keypair.pubkey());

        // Create AMM client
        let amm_client = AmmSwapClient::new(rpc_client, keypair);

        // Initialize Telegram notifier if configured
        let notifier = if let (Some(token), Some(chat_id)) =
            (&config.telegram_token, &config.telegram_chat_id)
        {
            Some(TelegramNotifier::new(token.clone(), chat_id.clone()))
        } else {
            None
        };

        let rpc_url = config
            .rpc_endpoints
            .first()
            .context("No RPC endpoints provided")?
            .clone();
        let rpc_client = RpcClient::new_with_commitment(rpc_url, config.commitment_config);

        // Load keypair
        let kp = read_keypair_file(&config.keypair)
            .map_err(|e| anyhow!("Failed to read keypair file: {}", e))?;

        Ok(SwapClient {
            inner: Arc::new(SwapClientInner {
                rpc_client,
                amm_client,
                kp,
                notifier,
            }),
        })
    }

    /// Get a reference to the RPC client
    pub fn rpc_client(&self) -> &RpcClient {
        &self.inner.rpc_client
    }

    /// Get a reference to the AMM client
    pub fn amm_client(&self) -> &AmmSwapClient {
        &self.inner.amm_client
    }

    /// Get a reference to the keypair
    pub fn keypair(&self) -> &Keypair {
        &self.inner.kp
    }

    /// Get the user wallet address
    // pub fn user_wallet(&self) -> Option<&str> {
    //     self.inner.user_wallet.as_deref()
    // }

    /// Get the Telegram notifier (if configured)
    pub fn notifier(&self) -> Option<&TelegramNotifier> {
        self.inner.notifier.as_ref()
    }

    /// Change RPC endpoint (recreates clients)
    pub async fn change_rpc_endpoint(&self, endpoint: String) -> Result<()> {
        info!("Changing RPC endpoint to: {}", endpoint);

        // This would need to create a new SwapClientInner
        // For now, we'll return an error since this is complex with Arc
        Err(anyhow::anyhow!(
            "Changing RPC endpoint requires recreating SwapClient"
        ))
    }

}
