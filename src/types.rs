use anyhow::Result;
use solana_signature::Signature;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Clone)]
pub struct SwapResult {
    pub signature: Signature,
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub amount_in: u64,
    pub estimated_amount_out: u64,
    pub jupiter_link: String,
    pub explorer_link: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl SwapResult {
    pub fn new(
        signature: Signature,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount_in: u64,
        estimated_amount_out: u64,
    ) -> Self {
        let jupiter_link = format!("https://jup.ag/tokens/{}", output_mint);
        let explorer_link = format!(
            "https://explorer.solana.com/tx/{}?cluster=mainnet-beta",
            signature
        );
        
        SwapResult {
            signature,
            input_mint,
            output_mint,
            amount_in,
            estimated_amount_out,
            jupiter_link,
            explorer_link,
            timestamp: chrono::Utc::now(),
        }
    }
    
    pub fn format_for_telegram(&self, user_wallet: Option<&str>) -> String {
        let wallet_info = user_wallet
            .map(|w| format!("👛 Wallet: `{}`\n", w))
            .unwrap_or_default();
            
        format!(
            "✅ *Swap Executed Successfully!*\n\n\
            {}\
            📊 *Transaction Details:*\n\
            • Signature: `{}`\n\
            • Input: {} tokens of `{}`\n\
            • Output: ~{} tokens of `{}`\n\
            • Time: {}\n\n\
            🔗 *Links:*\n\
            • [View on Explorer]({})\n\
            • [Token on Jupiter]({})",
            wallet_info,
            self.signature,
            self.amount_in,
            self.input_mint,
            self.estimated_amount_out,
            self.output_mint,
            self.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            self.explorer_link,
            self.jupiter_link
        )
    }
}

#[derive(Debug, Clone)]
pub struct SwapParams {
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub amount_in: u64,
    pub slippage: f64,
}

impl SwapParams {
    pub fn new(input_mint: Pubkey, output_mint: Pubkey, amount_in: u64, slippage: f64) -> Self {
        SwapParams {
            input_mint,
            output_mint,
            amount_in,
            slippage,
        }
    }
}