use ipc_server_rs::SwapExecutor;
use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use std::error::Error;
use std::path::Path;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tracing::{error, info};
use ipc_server_rs::{
    config::SwapConfig,
    client::SwapClient
};

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
    fn success(token_address: Option<String>, correlation_id: Option<String>) -> Self {
        Self {
            success: true,
            token_address,
            error: None,
            correlation_id,
        }
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

/// Extracts SPL token addresses from text
/// Returns Some(address) if exactly one valid address is found, None otherwise
fn extract_spl_token_address(text: &str, executor: &SwapExecutor) -> Option<String> {
    // Patterns to look for SPL token addresses
    let patterns = [
        // Base58 encoded addresses (44-45 chars)
        r"[1-9A-HJ-NP-Za-km-z]{43,44}",
        // Hexadecimal addresses with 0x prefix
        // r"0x[a-fA-F0-9]{64}",
    ];

    let mut found_addresses = Vec::new();

    for pattern in patterns.iter() {
        let re = regex::Regex::new(pattern).unwrap();
        for capture in re.find_iter(text) {
            let potential_addr = capture.as_str();

            if Pubkey::from_str(potential_addr).is_ok() {
                found_addresses.push(potential_addr.to_string());
            }
        }
    }

    match found_addresses.len() {
        1 => {
            executor.execute_swap(input);
            Some(found_addresses[0].clone()),
        }
        _ => None,
    }
}

async fn handle_client(
    mut socket: tokio::net::UnixStream,
    executor: &SwapExecutor,
    config: &SwapConfig
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
                        let response = IpcResponse::error(format!("Invalid JSON: {}", e), None);
                        let response_json = serde_json::to_string(&response)? + "\n";
                        write_half.write_all(response_json.as_bytes()).await?;
                        continue;
                    }
                };

                // Extract SPL token address
                let token_address = extract_spl_token_address(&request.message, executor);
                info!("Token address: {:?}", token_address);

                // Send response
                let response = IpcResponse::success(token_address, request.correlation_id);
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        // .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_level(false)
        .init();

    let socket_path = "/tmp/spl_token_ipc.sock";

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    let mut config = SwapConfig::from_env()?;
    // config.output_mint = Pubkey::from_str(output_mint)?;
    let client = SwapClient::new(&config).await?;

    let executor = SwapExecutor::new(client, config.slippage);

    // Bind to Unix socket
    let listener = UnixListener::bind(socket_path)?;
    info!("IPC server listening on {}", socket_path);

    // Accept connections
    loop {
        match listener.accept().await {
            Ok((socket, _addr)) => {
                info!("New client connected");

                tokio::spawn(async move {
                    if let Err(e) = handle_client(socket, &executor, &config).await {
                        error!("Error handling client: {}", e);
                    }
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
