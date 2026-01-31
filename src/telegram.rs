use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::info;
use std::sync::Arc;


#[derive(Clone)]
pub struct TelegramNotifier {
    inner: Arc<TelegramNotifierInner>,
}

struct TelegramNotifierInner {
    bot_token: String,
    chat_id: String,
    client: Client,
}

impl TelegramNotifier {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        // Create HTTP client with timeout
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
            
        TelegramNotifier {
            inner: Arc::new(TelegramNotifierInner {
                bot_token,
                chat_id,
                client,
            }),
        }
    }
    
    pub async fn send_message(&self, text: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.inner.bot_token
        );
        
        let payload = json!({
            "chat_id": &self.inner.chat_id,
            "text": text,
            "parse_mode": "Markdown",
            "disable_web_page_preview": false,
        });
        
        let response = self.inner.client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send Telegram request")?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!(
                "Telegram API error ({}): {}",
                status,
                error_text
            ));
        }
        
        info!("Telegram notification sent successfully");
        Ok(())
    }
}