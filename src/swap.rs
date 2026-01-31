use anyhow::{Context, Result, anyhow};
use raydium_amm_swap::{
    interface::{AmmPool, PoolKeys, PoolType},
};
use solana_signature::Signature;
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::Arc, time::{Duration, Instant}};
use tokio::time::sleep;
use tracing::{info, warn, debug};

use crate::{
    client::SwapClient,
    types::SwapResult,
};

/// Swap executor that can be reused with different mints
#[derive(Clone)]
pub struct SwapExecutor {
    client: Arc<SwapClient>,
    slippage: f64,
}

impl SwapExecutor {
    /// Create a new SwapExecutor with an initialized client
    pub fn new(client: SwapClient, slippage: f64) -> Self {
        SwapExecutor {
            client: Arc::new(client),
            slippage,
        }
    }
    
    /// Create from Arc<SwapClient>
    pub fn from_arc(client: Arc<SwapClient>, slippage: f64) -> Self {
        SwapExecutor { client, slippage }
    }
    
    /// Update slippage (creates new executor with updated slippage)
    pub fn with_slippage(&self, slippage: f64) -> Self {
        SwapExecutor {
            client: self.client.clone(),
            slippage,
        }
    }
    
    /// Execute a swap with specified mints
    pub async fn execute_swap(
        &self,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount_in: u64,
    ) -> Result<SwapResult> {
        info!("🚀 Starting swap execution");
        info!("Input: {} -> Output: {}", input_mint, output_mint);
        info!("Amount in: {}, Slippage: {}%", amount_in, self.slippage * 100.0);
        
        // Find pool
        let pool_id = self.find_raydium_pool(&input_mint, &output_mint).await?;
        
        // Get pool info
        let pool_info = self.client.amm_client()
            .fetch_pool_by_id(&pool_id)
            .await
            .context("Failed to fetch pool by ID")?;
            
        let pool_keys: PoolKeys<AmmPool> = self.client.amm_client()
            .fetch_pools_keys_by_id(&pool_id)
            .await
            .context("Failed to fetch pool keys")?;
            
        let rpc_data = self.client.amm_client()
            .get_rpc_pool_info(&pool_id)
            .await
            .context("Failed to get RPC pool info")?;
            
        let pool = pool_info
            .data
            .first()
            .ok_or_else(|| anyhow!("No pool data found"))?;
            
        // Calculate output amount
        let compute_result = self.client.amm_client()
            .compute_amount_out(&rpc_data, pool, amount_in, self.slippage)
            .context("Failed to compute amount out")?;
            
        let amount_out = compute_result.min_amount_out;
        
        info!("Swap parameters:");
        info!("  Input amount: {}", amount_in);
        info!("  Minimum output: {}", amount_out);
        
        // Execute swap
        let key = pool_keys
            .data
            .first()
            .ok_or_else(|| anyhow!("No pool key found"))?;
            
        let input_mint_str = input_mint.to_string();
        let output_mint_str = output_mint.to_string();
        let mint_a_addr = solana_address::Address::from_str_const(&input_mint_str);
        let mint_b_addr = solana_address::Address::from_str_const(&output_mint_str);
        
        info!("Sending swap transaction...");
        let signature = self.client.amm_client()
            .swap_amm(
                key,
                &mint_a_addr,
                &mint_b_addr,
                amount_in,
                amount_out,
            )
            .await
            .context("Failed to execute swap")?;
            
        info!("Transaction sent: {}", signature);
        
        // Wait for confirmation
        self.wait_for_confirmation(&signature).await?;
        
        // Create result
        let result = SwapResult::new(
            signature,
            input_mint,
            output_mint,
            amount_in,
            amount_out,
        );
        
        // Send notification if configured
        if let Some(notifier) = self.client.notifier() {
            let message = result.format_for_telegram(self.client.user_wallet());
            notifier.send_message(&message).await
                .map_err(|e| warn!("Failed to send Telegram notification: {}", e))
                .ok();
        }
        
        Ok(result)
    }
    
    async fn find_raydium_pool(
        &self,
        input_mint: &Pubkey,
        output_mint: &Pubkey,
    ) -> Result<Pubkey> {
        let start = Instant::now();
        
        info!(
            "Searching for pool with mints: {} -> {}",
            input_mint, output_mint
        );
        
        let all_mint_pools = self.client.amm_client()
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
        let pool_id = Pubkey::from_str(&first_pool.id)
            .context("Failed to parse pool ID")?;
            
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
        
        let pool_info = self.client.amm_client()
            .fetch_pool_by_id(&pool_id)
            .await
            .context("Failed to fetch pool by ID")?;
            
        let rpc_data = self.client.amm_client()
            .get_rpc_pool_info(&pool_id)
            .await
            .context("Failed to get RPC pool info")?;
            
        let pool = pool_info
            .data
            .first()
            .ok_or_else(|| anyhow!("No pool data found"))?;
            
        let compute_result = self.client.amm_client()
            .compute_amount_out(&rpc_data, pool, amount_in, self.slippage)
            .context("Failed to compute amount out")?;
            
        Ok(compute_result.min_amount_out)
    }
}