use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;
use tracing::info;

pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: Client,
}

impl TelegramNotifier {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        TelegramNotifier {
            bot_token,
            chat_id,
            client: Client::new(),
        }
    }
    
    pub async fn send_message(&self, text: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );
        
        let payload = json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "Markdown",
            "disable_web_page_preview": false,
        });
        
        let response = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send Telegram request")?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Telegram API error: {}", error_text));
        }
        
        info!("Telegram notification sent successfully");
        Ok(())
    }
}