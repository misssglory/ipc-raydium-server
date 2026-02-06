use anyhow::Result;
use raydium_amm_swap::interface::{ClmmPool, ClmmSinglePoolInfo};
use solana_sdk::pubkey::Pubkey;
use solana_signature::Signature;
use tracing::{ info, debug };

#[derive(Debug)]
pub enum FormatError {
  TimestampFormatting(String),
  UrlConstruction(String),
  GeneralFormatting(String),
}

impl std::fmt::Display for FormatError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      FormatError::TimestampFormatting(msg) => {
        write!(f, "Failed to format timestamp: {}", msg)
      }
      FormatError::UrlConstruction(msg) => {
        write!(f, "Failed to construct URL: {}", msg)
      }
      FormatError::GeneralFormatting(msg) => {
        write!(f, "Formatting failed: {}", msg)
      }
    }
  }
}

impl std::error::Error for FormatError {}

impl From<chrono::format::ParseError> for FormatError {
  fn from(err: chrono::format::ParseError) -> Self {
    FormatError::TimestampFormatting(err.to_string())
  }
}

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
  pub amm_input_mint_reserve: Option<u64>,
  pub amm_output_mint_reserve: Option<u64>,
}

impl SwapResult {
  pub fn new(
    signature: Signature,
    input_mint: Pubkey,
    output_mint: Pubkey,
    pool_id: Pubkey,
    amount_in: u64,
    amount_out: u64,
    amm_input_mint_reserve: Option<u64>,
    amm_output_mint_reserve: Option<u64>,
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
      amm_input_mint_reserve,
      amm_output_mint_reserve,
    }
  }

  // pub fn flipped(
  //   &self
  // ) {
  //   //TODO: Fix links
  //   let result = self.clone();
  //   let tmp = result.amount_in;
  //   result.amount_in = result.amount_out;
  //   result.amount_out = result.amount_in;
  //   let tmp = result.input_mint;
  //   result.input_mint = result.output_mint;
  //   result.
  // }

  // pub fn format_for_telegram(
  //     &self,
  //     // user_wallet: Option<&str>
  // ) -> String {
  //     // let wallet_info = user_wallet
  //     //     .map(|w| format!("👛 Wallet: `{}`\n", w))
  //     //     .unwrap_or_default();

  //     format!(
  //         "✅ *Swap Executed Successfully!*\n\n\
  //         📊 *Transaction Details:*\n\n\
  //         • Signature: `{}`\n\n\
  //         • Input: {} tokens of `{}`\n\n\
  //         • Output: ~{} tokens of `{}`\n\n\
  //         • Pool ID : `{}`\n\n\
  //         • Time: {}\n\n\
  //         🔗 *Links:*\n\
  //         • [Token on Raydium]({})\n\
  //         • [Token on Jupiter]({})\n\
  //         • [View on Explorer]({})\n\
  //         ",
  //         // wallet_info,
  //         self.signature.to_string(),
  //         self.amount_in,
  //         self.input_mint.to_string(),
  //         self.amount_out.to_string(),
  //         self.output_mint.to_string(),
  //         self.pool_id.to_string(),
  //         self.timestamp.format("%y-%m-%d %H:%M:%S%.4f UTC"),
  //         format!("https://raydium.io/swap/?inputMint={}&outputMint={}", self.output_mint.to_string(), self.input_mint.to_string()),
  //         self.jupiter_link,
  //         self.explorer_link,
  //     )
  // }
  pub fn format_for_telegram(
    &self,
  ) -> Result<
    String,
    //   Box<dyn std::error::Error>
  > {
    // Convert all to strings first to catch any potential panics
    let signature = self.signature.to_string();
    let input_mint = self.input_mint.to_string();
    let output_mint = self.output_mint.to_string();
    let pool_id = self.pool_id.to_string();
    let amount_out = self.amount_out.to_string();

    let timestamp_result = std::panic::catch_unwind(|| {
      self.timestamp.format("%y-%m-%d %H:%M:%S.%f UTC").to_string()
    })
    .map_err(|_| "Failed to format timestamp");

    // match timestamp_result {

    // }
    let timestamp_str = if let Ok(ts_str) = timestamp_result {
      ts_str
    } else {
      "Time format error".to_string()
    };

    let raydium_url = format!(
      "https://raydium.io/swap/?inputMint={}&outputMint={}",
      output_mint, input_mint
    );

    Ok(format!(
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
      signature,
      self.amount_in,
      input_mint,
      amount_out,
      output_mint,
      pool_id,
      timestamp_str,
      raydium_url,
      self.jupiter_link,
      self.explorer_link,
    ))
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
  pub fn new(
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_in: u64,
    slippage: f64,
  ) -> Self {
    SwapParams { input_mint, output_mint, amount_in, slippage }
  }
}

#[derive(Debug)]
pub struct QuoteParams {
  pub amount_in: u64,
  pub cmp_order: std::cmp::Ordering,
  pub target_quote: Option<u64>,
  // pub pool_info: &ClmmSinglePoolInfo,
}

impl QuoteParams {
  pub fn new(
    amount_in: u64,
    cmp_order: std::cmp::Ordering,
    target_quote: Option<u64>,
  ) -> Self {
    info!(
      "Quote params: amount in: {}, cmp order is Less {}, target quote: {}",
      amount_in,
      cmp_order == std::cmp::Ordering::Less,
      target_quote.unwrap_or(0)
    );
    QuoteParams { amount_in, cmp_order, target_quote }
  }
}

// impl QuoteParams {
//   pub async fn from_swap_result(
//     swap_result: &SwapResult,
//     // pool: &ClmmPool,
//     pool_info: Option<&ClmmSinglePoolInfo>,
//     min_profit_percent: f64,
//   ) -> Result<Self> {
//     // pub async fn get_quote_params(
//     // &self,
//     // let pool_info = pool_info.unwrap_or()
//     let pool_info = match pool_info {
//       Some(p_info) => p_info,
//       None => {

//     let pool_info = self
//       .client
//       .amm_client()
//       .fetch_pool_by_id(&swap_result.pool_id)
//       .await
//       .context("Failed to fetch pool by ID")?;
//       }
//     }
//     let pool = None;
//     // ) -> Result<(u64, Ordering)> {
//     // ) -> Result<(QuoteParams, ClmmSinglePoolInfo)> {
//     let mint_a_address = pool.mint_a.address.clone();
//     debug!("Pool mint A address: {}", mint_a_address);

//     if mint_a_address == swap_result.input_mint.to_string() {
//       debug!("Mint A IS input mint");
//       // return Ok((swap_result.amount_in, Less));
//       let target_quote = Some(
//         (swap_result.amount_out as f64
//           / min_profit_percent) as u64,
//       );
//       return Ok((
//         QuoteParams {
//           amount_in: swap_result.amount_in,
//           cmp_order: Less,
//           target_quote,
//         },
//         pool_info,
//       ));
//     } else {
//       debug!("Mint A is NOT input mint");
//       let target_quote = Some(
//         (swap_result.amount_in as f64
//           * min_profit_percent) as u64,
//       );
//       // return Ok((swap_result.amount_out, Greater));
//       return Ok((
//         QuoteParams {
//           amount_in: swap_result.amount_out,
//           cmp_order: Greater,
//           target_quote,
//         },
//         pool_info,
//       ));
//     };
//   }
// }
