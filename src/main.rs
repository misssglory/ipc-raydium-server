use chrono::{DateTime, Local};
use ipc_server_rs::hash_cache::HashCache;
use ipc_server_rs::holders_fetcher::TokenHoldersFetcher;
use ipc_server_rs::quoter::Quoter;
use ipc_server_rs::{SwapExecutor, SwapResult};
use raydium_amm_swap::consts::SOL_MINT;
use tracing_subscriber::filter::LevelFilter;
// use ipc_server_rs::pump_swap::PumpSwapExecutor;
use ipc_server_rs::{client::SwapClient, config::SwapConfig};
use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_signature::Signature;
use std::cmp::Ordering::{Equal, Greater, Less};
use std::env;
use std::error::Error;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tracing::{debug, error, info};
use tracing_subscriber::{Layer, Registry, fmt, prelude::*};

#[derive(Debug, Serialize, Deserialize)]
struct IpcRequest {
  message: String,
  #[serde(default)]
  correlation_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcResponse {
  success: bool,
  token_address: Option<String>,
  error: Option<String>,
  correlation_id: Option<String>,
}

impl IpcResponse {
  fn success(
    token_address: Option<String>,
    correlation_id: Option<String>,
  ) -> Self {
    Self { success: true, token_address, error: None, correlation_id }
  }

  fn error(error: String, correlation_id: Option<String>) -> Self {
    Self {
      success: false,
      token_address: None,
      error: Some(error),
      correlation_id,
    }
  }
}

async fn extract_spl_token_address(
  text: &str,
  // state: Arc<AppState>,
  executor: Arc<SwapExecutor>,
  // first_conn: bool,
  hash_cache: Arc<HashCache<Pubkey>>,
) -> Option<String> {
  let patterns = [r"[1-9A-HJ-NP-Za-km-z]{43,44}"];

  let mut found_addresses = Vec::new();

  for pattern in patterns.iter() {
    let re = regex::Regex::new(pattern).unwrap();
    for capture in re.find_iter(text) {
      let potential_addr = capture.as_str();

      if let Ok(pubkey) = Pubkey::from_str(potential_addr) {
        found_addresses.push(pubkey);
        break;
      }
    }
  }

  match found_addresses.len() {
    1 => {
      let address = *found_addresses.first()?;

      if hash_cache.insert(address) {
        for i in 0..2 {
          // let result = executor
          //   .execute_round_trip_with_notification(
          //     None,
          //     found_addresses.first(),
          //     500,
          //   )
          //   .await;
          // let mut config = executor.config_mut().await;
          // config.min_profit_percent =
          //   1.0 + (config.min_profit_percent - 1.0) * 2.0;
          // config.timelimit_seconds = config.timelimit_seconds * 4;

          let mut input_mint = Pubkey::from_str(SOL_MINT).unwrap();
          let mut output_mint = address;
          let pool_id = executor
            .find_raydium_pool(&input_mint, &output_mint)
            .await
            .unwrap();
          let pool_info = executor
            .client()
            .amm_client()
            .fetch_pool_by_id(&pool_id)
            .await
            // .context("Failed to fetch pool by ID")
            .unwrap();

          let input_mint_is_a_mint =
            pool_info.data.first().unwrap().mint_a.address
              == input_mint.to_string();
          let fee = pool_info.data.first().unwrap().fee_rate.unwrap();
          let mut amount_in = executor.config_mut().await.amount_in;
          let mut amount_out = executor
            .get_quote(
              &pool_info,
              &pool_id,
              amount_in,
              Some(0.0),
              (i % 2 == 1) ^ input_mint_is_a_mint,
            )
            .await
            .unwrap();
          if i % 2 == 1 {
            std::mem::swap(&mut input_mint, &mut output_mint);
            // let tmp = amount_in;
            // amount_in = (amount_out as f64 * (1.0 + fee)) as u64;
            // amount_out = (tmp as f64 * (1.0 - fee)) as u64;
          }

          info!(
            "Initial amount in: {} Mint: {}",
            amount_in,
            input_mint.to_string()
          );
          info!(
            "Initial amount out: {} Mint: {}",
            amount_out,
            output_mint.to_string()
          );
          let mut mock_swap_result = SwapResult {
            signature: Signature::new_unique(),
            input_mint: input_mint,
            output_mint: output_mint,
            pool_id: pool_id,
            amount_in: amount_in,
            amount_out: amount_out,
            jupiter_link: "".to_string(),
            explorer_link: "".to_string(),
            timestamp: DateTime::from_timestamp_secs(17000000).unwrap(),
            amm_input_mint_reserve: None,
            amm_output_mint_reserve: None,
          };

          let quoter = Quoter::new(
            pool_id,
            executor.client().clone(),
            mock_swap_result.input_mint,
            None,
          )
          .await.unwrap();

          // let target_pct = if i % 2 == 0 { 1.01 } else { 0.99 };
          let target_pct = 1.002;
          quoter.quote_loop(target_pct, 10000u128).await.unwrap();

          // let (mock_quote_params, pool_info) =
          //   executor.get_quote_params(&mock_swap_result, true).await.unwrap();

          // // mock_swap_result.amount_out = amount_out;

          // let result = executor
          //   .quote_loop(
          //     Some(pool_id),
          //     &mock_quote_params,
          //     // Some(&Pubkey::from_str(SOL_MINT).unwrap()),
          //     Some(&pool_info),
          //     None,
          //     None,
          //     // Some(&address),
          //     // Some(address),
          //     None,
          //     1500,
          //   )
          //   .await;
          // info!("{:?}", result);
        }
      }
      Some(found_addresses[0].to_string())
    }
    _ => None,
  }
}

async fn handle_client(
  mut socket: tokio::net::UnixStream,
  // state: Arc<AppState>,
  executor: Arc<SwapExecutor>,
  // first_conn: bool,
  hash_cache: Arc<HashCache<Pubkey>>,
) -> Result<(), Box<dyn Error>> {
  let (read_half, mut write_half) = socket.split();
  let mut reader = BufReader::new(read_half);
  let mut buffer = String::new();

  loop {
    buffer.clear();

    // Read a line from the client
    match reader.read_line(&mut buffer).await {
      Ok(0) => break, // EOF
      Ok(_) => {
        // Parse the request
        let request: IpcRequest = match serde_json::from_str(&buffer) {
          Ok(req) => req,
          Err(e) => {
            let response =
              IpcResponse::error(format!("Invalid JSON: {}", e), None);
            let response_json = serde_json::to_string(&response)? + "\n";
            write_half.write_all(response_json.as_bytes()).await?;
            continue;
          }
        };

        let hash_cache = hash_cache.clone();
        let token_address = extract_spl_token_address(
          &request.message,
          executor.clone(),
          hash_cache,
        )
        .await;
        info!("Token address: {:?}", token_address);

        // Send response
        let response =
          IpcResponse::success(token_address, request.correlation_id);
        let response_json = serde_json::to_string(&response)? + "\n";
        write_half.write_all(response_json.as_bytes()).await?;
        write_half.flush().await?;
      }
      Err(e) => {
        error!("Error reading from socket: {}", e);
        break;
      }
    }
  }

  Ok(())
}

struct AppState {
  executor: Arc<SwapExecutor>,
  config: Arc<SwapConfig>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let mut config = SwapConfig::from_env()?;

  let binary_name = env::current_exe()
    .ok()
    .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
    .unwrap_or_else(|| "app".to_string());

  let start_time = Local::now().format("%y%m%d_%H-%M-%S");

  let file_name = format!("{}_{}", binary_name, start_time);
  let file_appender = tracing_appender::rolling::never("logs", &file_name);
  let (non_blocking_file, _guard) =
    tracing_appender::non_blocking(file_appender);
  let file_layer = fmt::Layer::default()
    .with_target(false)
    .with_level(true)
    .with_ansi(false) // Recommended for file logs to avoid raw escape codes
    .with_writer(non_blocking_file)
    .with_filter(config.log_level);

  let stdout_layer = fmt::Layer::default()
    .with_target(false)
    .with_level(true)
    .with_writer(std::io::stdout)
    .with_filter(config.log_level);

  let file_appender_info =
    tracing_appender::rolling::never("info-logs", &file_name);
  let (non_blocking_file_info, _guard) =
    tracing_appender::non_blocking(file_appender_info);
  let file_layer_info = fmt::Layer::default()
    .with_target(false)
    .with_level(true)
    .with_ansi(false) // Recommended for file logs to avoid raw escape codes
    .with_writer(non_blocking_file_info)
    .with_filter(LevelFilter::INFO);

  Registry::default()
    .with(stdout_layer)
    .with(file_layer)
    .with(file_layer_info)
    .init();

  debug!("Amount in: {}", config.amount_in);

  let socket_path = "/tmp/spl_token_ipc.sock";

  // Remove socket file if it exists
  if Path::new(socket_path).exists() {
    std::fs::remove_file(socket_path)?;
  }

  // config.output_mint = Pubkey::from_str(output_mint)?;
  let client = SwapClient::new(&config).await?;

  // let mut fetcher =
  // TokenHoldersFetcher::new(&config.rpc_endpoints[0].clone()).await?;
  let executor = Arc::new(SwapExecutor::new(client, config));

  // let state = Arc::new(AppState {
  //     executor: Arc::new(executor),
  //     // config: config.clone(), // Clone config for sharing
  //     config: Arc::new(config),
  // });

  // Bind to Unix socket

  //   let pump_executor = PumpSwapExecutor::new(client, config);
  //   let result = pump_executor
  //     .execute_swap(
  //       Some(&input_mint),
  //       Some(&output_mint),
  //       Some(amount),
  //       None,
  //       true, // is_buy = true
  //     )
  //     .await?;

  // fetcher.fetch_holders("8aZEym6Uv5vuy2LQ9BYNSiSiiKS3JKJEhbiUgpQppump").await?;
  // Ok(())

  let listener = UnixListener::bind(socket_path)?;
  info!("IPC server listening on {}", socket_path);

  let is_first_connection = Arc::new(AtomicBool::new(true));
  let hash_cache = Arc::new(HashCache::<Pubkey>::new());
  // Accept connections
  loop {
    match listener.accept().await {
      Ok((socket, _addr)) => {
        info!("New client connected");

        // let state = state.clone();
        let executor = executor.clone();
        let hash_cache = hash_cache.clone();
        let is_first_connection = is_first_connection.clone();

        tokio::spawn(async move {
          // let first_conn = is_first_connection.load(Ordering::SeqCst);
          // let first_conn = is_first_connection.swap(false, Ordering::SeqCst);
          let first_conn = true;
          info!("First conn: {}", first_conn);

          if let Err(e) = handle_client(socket, executor, hash_cache).await {
            error!("Error handling client: {}", e);
          }
          // if first_conn {
          //     is_first_connection.store(false, Ordering::SeqCst);
          // }
        });
      }
      Err(e) => {
        error!("Error accepting connection: {}", e);
        break;
      }
    }
  }

  Ok(())
}
