use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use solana_signature::Signature;

#[derive(Debug, Clone)]
pub struct SwapResult {
    pub signature: Signature,
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub pool_id: Pubkey,
    pub amount_in: u64,
    pub amount_out: u64,
    pub jupiter_link: String,
    pub explorer_link: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl SwapResult {
    pub fn new(
        signature: Signature,
        input_mint: Pubkey,
        output_mint: Pubkey,
        pool_id: Pubkey,
        amount_in: u64,
        amount_out: u64,
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
            pool_id, 
            amount_in,
            amount_out,
            jupiter_link,
            explorer_link,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn format_for_telegram(
        &self,
        // user_wallet: Option<&str>
    ) -> String {
        // let wallet_info = user_wallet
        //     .map(|w| format!("👛 Wallet: `{}`\n", w))
        //     .unwrap_or_default();

        format!(
            "✅ *Swap Executed Successfully!*\n\n\
            📊 *Transaction Details:*\n\n\
            • Signature: `{}`\n\n\
            • Input: {} tokens of `{}`\n\n\
            • Output: ~{} tokens of `{}`\n\n\
            • Pool ID : `{}`\n\n\
            • Time: {}\n\n\
            🔗 *Links:*\n\
            • [Token on Raydium]({})\n\
            • [Token on Jupiter]({})\n\
            • [View on Explorer]({})\n\
            ",
            // wallet_info,
            self.signature,
            self.amount_in,
            self.input_mint,
            self.amount_out,
            self.output_mint,
            self.pool_id, 
            self.timestamp.format("%y-%m-%d %H:%M:%S%.4f UTC"),
            format!("https://raydium.io/swap/?inputMint={}&outputMint={}", self.output_mint, self.input_mint),
            self.jupiter_link,
            self.explorer_link,
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
