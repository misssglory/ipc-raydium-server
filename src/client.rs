use anyhow::{Context, Result, anyhow};
use raydium_amm_swap::amm::client::AmmSwapClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    signature::{Keypair, read_keypair_file},
    pubkey::Pubkey,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::{config::SwapConfig, telegram::TelegramNotifier};

/// Main client that holds initialized components and can be reused
pub struct SwapClient {
    rpc_client: Arc<RpcClient>,
    amm_client: Arc<AmmSwapClient>,
    keypair: Arc<Keypair>,
    notifier: Option<Arc<TelegramNotifier>>,
    user_wallet: Option<String>,
}

impl SwapClient {
    /// Create a new SwapClient from configuration
    pub async fn new(config: &SwapConfig) -> Result<Self> {
        info!("Initializing SwapClient...");
        
        // Initialize RPC client with first endpoint
        let rpc_url = config.rpc_endpoints.first()
            .context("No RPC endpoints provided")?
            .clone();
        
        // let rpc_client = Arc::new(RpcClient::new(rpc_url));
        let rpc_client= RpcClient::new(rpc_url);
        
        // Load keypair
        let keypair = read_keypair_file(&config.keypair_path)
            .map_err(|e| anyhow!("Failed to read keypair file: {}", e))?;
        
        // info!("Loaded keypair: {}", keypair.pubkey());
        
        // Create AMM client
        let amm_client = Arc::new(AmmSwapClient::new(
            rpc_client,
            keypair,
        ));
        
        // Initialize Telegram notifier if configured
        let notifier = if let (Some(token), Some(chat_id)) = (&config.telegram_token, &config.telegram_chat_id) {
            Some(Arc::new(TelegramNotifier::new(token.clone(), chat_id.clone())))
        } else {
            None
        };

        let rpc_url = config.rpc_endpoints.first()
            .context("No RPC endpoints provided")?
            .clone();
        // let new_rpc_client = RpcClient::new(rpc_url);
        let rpc_client = Arc::new(RpcClient::new(rpc_url));
        let keypair= read_keypair_file(&config.keypair_path)
            .map_err(|e| anyhow!("Failed to read keypair file: {}", e))?;
        
        Ok(SwapClient {
            rpc_client,
            amm_client,
            keypair: Arc::new(keypair),
            notifier,
            user_wallet: config.user_wallet.clone(),
        })
    }
    
    /// Get a reference to the RPC client
    pub fn rpc_client(&self) -> Arc<RpcClient> {
        self.rpc_client.clone()
    }
    
    /// Get a reference to the AMM client
    pub fn amm_client(&self) -> Arc<AmmSwapClient> {
        self.amm_client.clone()
    }
    
    /// Get a reference to the keypair
    pub fn keypair(&self) -> Arc<Keypair> {
        self.keypair.clone()
    }
    
    /// Get the user wallet address
    pub fn user_wallet(&self) -> Option<String> {
        self.user_wallet.clone()
    }
    
    /// Get the Telegram notifier (if configured)
    pub fn notifier(&self) -> Option<Arc<TelegramNotifier>> {
        self.notifier.clone()
    }
    
    // Change RPC endpoint (recreates clients)
    // pub async fn change_rpc_endpoint(&mut self, endpoint: String) -> Result<()> {
    //     info!("Changing RPC endpoint to: {}", endpoint);
        
    //     // Create new RPC client
    //     // let new_rpc_client = Arc::new(RpcClient::new(endpoint));
    //     let new_rpc_client = RpcClient::new(endpoint);
        
    //     // Recreate AMM client with new RPC client
    //     let new_amm_client = Arc::new(AmmSwapClient::new(
    //         new_rpc_client.clone(),
    //         // self.keypair.clone().as_ref().clone(),
    //         keypair
    //     ));
        
    //     // Update clients
    //     self.rpc_client = new_rpc_client;
    //     self.amm_client = new_amm_client;
        
    //     info!("RPC endpoint changed successfully");
    //     Ok(())
    // }
}