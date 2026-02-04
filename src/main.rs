use chrono::Local;
use ipc_server_rs::SwapExecutor;
use ipc_server_rs::hash_cache::HashCache;
use ipc_server_rs::holders_fetcher::TokenHoldersFetcher;
use raydium_amm_swap::consts::SOL_MINT;
// use ipc_server_rs::pump_swap::PumpSwapExecutor;
use ipc_server_rs::{client::SwapClient, config::SwapConfig};
use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
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
          // config.min_profit_percent = 1.0 + (config.min_profit_percent - 1.0) * 2.0;
          let result = executor
            .quote_loop(
              None,
              Some(&Pubkey::from_str(SOL_MINT).unwrap()),
              Some(&address),
              None,
              None,
              None,
              1500,
              Less,
            )
            .await;
          info!("{:?}", result);
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
  let file_name = format!("{}_{}.log", binary_name, start_time);

  let file_appender = tracing_appender::rolling::never("logs", &file_name);
  let (non_blocking_file, _guard) =
    tracing_appender::non_blocking(file_appender);

  let stdout_layer = fmt::Layer::default()
    .with_target(false)
    .with_level(true)
    .with_writer(std::io::stdout)
    .with_filter(config.log_level);

  let file_layer = fmt::Layer::default()
    .with_target(false)
    .with_level(true)
    .with_ansi(false) // Recommended for file logs to avoid raw escape codes
    .with_writer(non_blocking_file)
    .with_filter(config.log_level);

  Registry::default().with(stdout_layer).with(file_layer).init();

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
