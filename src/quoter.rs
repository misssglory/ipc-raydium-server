use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use raydium_amm_swap::interface::ClmmPool;
use solana_sdk::pubkey::Pubkey;
use tracing::{error, info};

use crate::client::SwapClient;

#[derive(Clone)]
pub struct Quoter {
  pool_id: Pubkey,
  pool: Arc<ClmmPool>,
  client: Arc<SwapClient>,
  initial_quote: f64,
  quote_mint_is_a_mint: bool,
  poll_interval_millis: i128,
}

impl Quoter {
  pub async fn new(
    pool_id: Pubkey,
    client: Arc<SwapClient>,
    quote_mint: Pubkey,
    initial_quote: Option<f64>,
  ) -> Result<Self> {
    let pool_info = client
      .amm_client()
      .fetch_pool_by_id(&pool_id)
      .await
      .context("Failed to fetch pool by ID")?;

    let pool_id_str = pool_id.to_string();
    let pool = pool_info
      .data
      .iter()
      .find(|pool| pool.id == pool_id_str)
      .context("No pool found")?
      .clone();
    let pool = Arc::new(pool);

    let quote_mint_is_a_mint = pool.mint_a.address == quote_mint.to_string();
    if !quote_mint_is_a_mint && pool.mint_b.address != quote_mint.to_string() {
      error!(
        "Pool mint A: {} Pool mint B: {} Quote mint: {}",
        pool.mint_a.address, pool.mint_b.address, quote_mint
      );
      return Err(anyhow!("base mint is wrong for pool"));
    }

    let initial_quote = match initial_quote {
      Some(quote) => quote,
      None => 0.0,
    };

    let mut quoter = Quoter {
      pool_id,
      pool,
      client: client.clone(),
      quote_mint_is_a_mint,
      initial_quote,
      poll_interval_millis: 1500,
    };

    if initial_quote == 0.0 {
      quoter.set_initial_quote(None).await?;
    }

    Ok(quoter)
  }

  pub async fn set_initial_quote(&mut self, quote: Option<f64>) -> Result<()> {
    if let Some(quote) = quote {
      self.initial_quote = quote;
    } else {
      self.initial_quote = self.get_quote().await?;
    }
    Ok(())
  }

  pub async fn set_poll_interval(&mut self, interval_millis: u64) {
    self.poll_interval_millis = interval_millis as i128;
  }

  pub async fn get_quote(&self) -> Result<f64> {
    let rpc_data = self
      .client
      .amm_client()
      .get_rpc_pool_info(&self.pool_id)
      .await
      .context("Failed to get RPC pool info")?;

    if self.quote_mint_is_a_mint {
      Ok(rpc_data.base_reserve as f64 / rpc_data.quote_reserve as f64)
    } else {
      Ok(rpc_data.quote_reserve as f64 / rpc_data.base_reserve as f64)
    }
  }

  pub async fn get_until_target(&self) -> Result<f64> {
    let quote = self.get_quote().await?;
    let target = 
    // if higher {
      quote as f64 / self.initial_quote as f64;
    // } else {
    //   self.initial_quote as f64 / quote as f64
    // };
    // let target = target_sqrt * target_sqrt;
    info!(
      "Quote: {} Initial: {} Target: {}",
      quote, self.initial_quote, target
    );
    Ok(target)
  }

  pub async fn quote_loop(&self, target_pct: f64, timelimit_millis: u128) -> Result<()> {
    let start_loop = Instant::now();
    loop {
      let start_quote_time = Instant::now();
      let target = self.get_until_target().await?;

      if target_pct > 1.0 && target > target_pct {
        info!("Target reached: {} Target: {}", target_pct, target);
        return Ok(());
      }

      if target_pct < 1.0 && target < target_pct {
        info!("Target reached: {} Target: {}", target_pct, target);
        return Ok(());
      }

      let elapsed_millis = start_loop.elapsed().as_millis();
      if elapsed_millis > timelimit_millis {
        info!("Timelimit reached! {} ms", elapsed_millis);
        return Ok(());
      } else {
        info!("Elapsed: {} ms", elapsed_millis);
      }

      let sleep_time_millis = std::cmp::max(
        0i128,
        self.poll_interval_millis
          - start_quote_time.elapsed().as_millis() as i128,
      ) as u64;
      std::thread::sleep(Duration::from_millis(sleep_time_millis));
    }
  }
}
