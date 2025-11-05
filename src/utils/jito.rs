use reqwest;
use serde_json::{json, Value};

pub struct JitoClient {
    client: reqwest::Client,
    jito_rpc_url: String,
}

impl JitoClient {
    pub fn new() -> Self {
        let jito_rpc_url = std::env::var("JITO_RPC_URL")
            .unwrap_or_else(|_| "https://mainnet.block-engine.jito.wtf:443/api/v1/bundles".to_string());
        
        Self {
            client: reqwest::Client::new(),
            jito_rpc_url,
        }
    }

    pub async fn send_bundle(&self, transactions: &[String]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [transactions]
        });

        let response: Value = self.client
            .post(&self.jito_rpc_url)
            .json(&request_body)
            .send()
            .await?
            .json()
            .await?;

        if let Some(error) = response.get("error") {
            return Err(format!("Jito bundle failed: {}", error).into());
        }

        if let Some(result) = response["result"].as_str() {
            Ok(result.to_string())
        } else {
            Err("Failed to parse Jito response".into())
        }
    }
}