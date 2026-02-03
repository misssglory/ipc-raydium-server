// holders_fetcher.rs
// use solana_client::rpc_client::RpcClient;
use anyhow::anyhow;
use dotenv::dotenv;
use solana_account_decoder::{UiAccount, UiAccountData};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::CommitmentConfig;
use solana_client::rpc_filter::{Memcmp, RpcFilterType};
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
// use spl_token::{state::Mint};
use std::env;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_postgres::{Client, NoTls};
use tracing::{error, info, instrument, warn};
use spl_token_2022::state::Account as TokenAccount;


/// Configuration for database connection
#[derive(Debug, Clone)]
struct DbConfig {
  host: String,
  port: u16,
  user: String,
  name: String,
  password: String,
}

impl DbConfig {
  fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
    Ok(DbConfig {
      host: env::var("DB_HOST")?,
      port: env::var("DB_PORT")?.parse()?,
      user: env::var("DB_USER")?,
      name: env::var("DB_NAME")?,
      password: env::var("DB_PASSWORD")?,
    })
  }
}

/// Token holder information
#[derive(Debug)]
pub struct TokenHolder {
  owner: String,
  ata: String,
  mint: String,
  amount: u64,
  last_updated: i64,
}

/// Main fetcher struct
pub struct TokenHoldersFetcher {
  rpc_client: RpcClient,
  db_client: Client,
}

impl TokenHoldersFetcher {
  /// Create a new TokenHoldersFetcher instance
  #[instrument]
  pub async fn new(rpc_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
    dotenv().ok();
    info!("Initializing TokenHoldersFetcher");

    // Load database configuration
    let db_config = DbConfig::from_env()?;
    info!("Database config loaded: {:?}", db_config);

    // Connect to database
    let connection_string = format!(
      "host={} port={} user={} password={} dbname={}",
      db_config.host,
      db_config.port,
      db_config.user,
      db_config.password,
      db_config.name
    );

    let (db_client, connection) =
      tokio_postgres::connect(&connection_string, NoTls).await?;

    // Spawn connection task
    tokio::spawn(async move {
      if let Err(e) = connection.await {
        error!("Database connection error: {}", e);
      }
    });

    info!("Database connection established");

    // Initialize database schema
    Self::init_database(&db_client).await?;

    // Create RPC client
    let rpc_client = RpcClient::new_with_commitment(
      rpc_url.to_string(),
      CommitmentConfig::confirmed(),
    );

    info!("RPC client connected to: {}", rpc_url);

    Ok(Self { rpc_client, db_client })
  }

  async fn get_holders(
    mint_address: &str,
  ) -> anyhow::Result<
    Vec<(Pubkey, UiAccount)>,
    // solana_client::client_error::ClientError,
  > {
    // 1. Initialize RPC Client (using Mainnet Beta as an example)
    let rpc_url = "https://api.mainnet-beta.solana.com".to_string();
    let client = RpcClient::new(rpc_url);
    let mint_pubkey = Pubkey::from_str(mint_address)?;

    // 2. Set up filters
    let filters = vec![
      // Filter by size (Token accounts are always 165 bytes)
      RpcFilterType::DataSize(165),
      // Filter by Mint address (starts at offset 0)
      RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
        0,
        mint_pubkey.as_ref(),
      )),
    ];

    // 3. Query all accounts owned by the SPL Token Program with these filters
    let accounts = client
      .get_program_ui_accounts_with_config(
        &spl_token_2022::id(),
        solana_client::rpc_config::RpcProgramAccountsConfig {
          filters: Some(filters),
          account_config: solana_client::rpc_config::RpcAccountInfoConfig {
            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
            ..Default::default()
          },
          ..Default::default()
        },
      )
      .await;

    // accounts
    match accounts {
      Ok(acc) => Ok(acc),
      Err(err) => Err(anyhow!("RPC client error")),
    }
    // 4. Parse results to see holders and balances
    // for (pubkey, account) in accounts {
    // //   let token_account = spl_token_2022::state::Account::unpack(&account.data)?;
    //   println!(
    //     "Token Account: {}, Owner: {}, Balance: {}",
    //     // pubkey, token_account.owner, token_account.amount
    //     pubkey, account.owner, account.lamports
    //   );
    // }

    // Ok(())
  }

  pub async fn fetch_holders(
    &mut self,
    mint_address: &str,
  ) -> anyhow::Result<Vec<TokenHolder>> {
    let accounts = TokenHoldersFetcher::get_holders(mint_address).await?;
    let mut holders: Vec<TokenHolder> = Vec::new();
    // let now = SystemTime::now().timestamp_millis();
    let ts = chrono::Utc::now().timestamp_millis();

    for (pubkey, acc) in accounts {
      let data_bytes = match acc.data {
        UiAccountData::Binary(data_str, _) => base64::decode(data_str)?,
        _ => continue, // Skip if data isn't in binary format
      };

      let token_account = TokenAccount::unpack(&data_bytes)?;
      holders.push(TokenHolder {
        owner: token_account.owner.to_string(),
        ata: pubkey.to_string(),
        mint: mint_address.to_string(),
        amount: token_account.amount,
        last_updated: ts,
      });
    }
    info!("Found {} holders with non-zero balance", holders.len());
    self.save_holders(&holders).await?;
    Ok(holders)
    // Ok(())
  }

  /// Initialize database schema
  #[instrument(skip(client))]
  async fn init_database(
    client: &Client,
  ) -> Result<(), Box<dyn std::error::Error>> {
    info!("Initializing database schema");

    client
      .batch_execute(
        r#"
            CREATE TABLE IF NOT EXISTS token_holders (
                id SERIAL PRIMARY KEY,
                owner VARCHAR(44) NOT NULL,
                ata VARCHAR(44) NOT NULL,
                mint VARCHAR(44) NOT NULL,
                amount BIGINT NOT NULL,
                last_updated BIGINT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            
            CREATE INDEX IF NOT EXISTS idx_token_holders_owner 
            ON token_holders(owner);
            
            CREATE INDEX IF NOT EXISTS idx_token_holders_mint 
            ON token_holders(mint);
            
            CREATE INDEX IF NOT EXISTS idx_token_holders_owner_mint 
            ON token_holders(owner, mint);
            
            CREATE UNIQUE INDEX IF NOT EXISTS unique_owner_mint 
            ON token_holders(owner, mint);
            "#,
      )
      .await?;

    info!("Database schema initialized");
    Ok(())
  }

  /// Save token holders to database
  #[instrument(skip(self, holders))]
  async fn save_holders(
    &mut self,
    holders: &[TokenHolder],
  ) -> anyhow::Result<
    (),
    //   Box<dyn std::error::Error>
  > {
    if holders.is_empty() {
      info!("No holders to save");
      return Ok(());
    }

    info!("Saving {} holders to database", holders.len());

    let transaction = self.db_client.transaction().await?;

    for holder in holders {
      transaction.execute(
                r#"
                INSERT INTO token_holders (owner, mint, amount, last_updated, ata)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (owner, mint) 
                DO UPDATE SET 
                    amount = EXCLUDED.amount,
                    last_updated = EXCLUDED.last_updated,
                    updated_at = CURRENT_TIMESTAMP
                "#,
                &[
                    &holder.owner,
                    &holder.mint,
                    &(holder.amount as i64),
                    &holder.last_updated,
                    &holder.ata,
                ],
            ).await?;
    }

    transaction.commit().await?;

    info!("Successfully saved {} holders to database", holders.len());
    Ok(())
  }

  /// Get total count of holders from database
  #[instrument(skip(self))]
  pub async fn get_total_holders_count(
    &self,
  ) -> Result<i64, Box<dyn std::error::Error>> {
    let row = self
      .db_client
      .query_one("SELECT COUNT(*) as count FROM token_holders", &[])
      .await?;

    let count: i64 = row.get("count");
    info!("Total holders in database: {}", count);

    Ok(count)
  }

  /// Clean up old records (optional utility method)
  #[instrument(skip(self))]
  pub async fn cleanup_old_records(
    &self,
    days_old: i32,
  ) -> Result<u64, Box<dyn std::error::Error>> {
    info!("Cleaning up records older than {} days", days_old);

    let result = self
      .db_client
      .execute(
        r#"
            DELETE FROM token_holders 
            WHERE last_updated < EXTRACT(EPOCH FROM NOW()) - ($1 * 86400)
            "#,
        &[&(days_old as i64)],
      )
      .await?;

    info!("Deleted {} old records", result);
    Ok(result)
  }
}

/// Helper function to create a simple fetcher for single mint
#[instrument]
pub async fn fetch_and_save_holders(
  rpc_url: &str,
  mint_address: &str,
) -> Result<(), Box<dyn std::error::Error>> {
  info!("Starting holder fetch for {}", mint_address);

  let fetcher = TokenHoldersFetcher::new(rpc_url).await?;
  //   fetcher.fetch_token_holders(mint_address).await?;
  TokenHoldersFetcher::get_holders(mint_address).await?;

  let count = fetcher.get_total_holders_count().await?;
  info!("Total holders after fetch: {}", count);

  Ok(())
}
