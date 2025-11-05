use reqwest;
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;

pub struct JitoClient {
    client: reqwest::Client,
    jito_rpc_url: String,
    auth_header: Option<String>,
    // Jito tip accounts (these are the public keys of the tip accounts)
    tip_accounts: Vec<Pubkey>,
}

impl JitoClient {
    pub fn new() -> Option<Self> {
        let jito_rpc_url = std::env::var("JITO_RPC_URL").ok()?;
        
        // La autenticación para Jito normalmente requiere credenciales
        let auth_header = std::env::var("JITO_AUTH_HEADER").ok();
        
        // Jito tip accounts - these are the official tip account addresses
        // These should work for both mainnet and devnet
        let tip_accounts = vec![
            "96gYZGLnJYVFvJJvLL1JUH6ZVx5AZPfC4DW4wxPqZDAx".parse().unwrap(), // Main tip account
            "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY".parse().unwrap(), // Alternative tip account
            "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh".parse().unwrap(), // Alternative tip account
            "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49".parse().unwrap(), // Alternative tip account
            "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt".parse().unwrap(), // Alternative tip account
        ];
        
        Some(Self {
            client: reqwest::Client::new(),
            jito_rpc_url,
            auth_header,
            tip_accounts,
        })
    }

    pub async fn send_bundle(&self, transactions: &[String]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [transactions]
        });

        let mut request = self.client.post(&self.jito_rpc_url).json(&request_body);
        
        // Agregar header de autenticación si está disponible
        if let Some(auth) = &self.auth_header {
            request = request.header("Authorization", auth);
        }

        let response: Value = request.send().await?.json().await?;

        if let Some(error) = response.get("error") {
            return Err(format!("Jito bundle failed: {}", error).into());
        }

        if let Some(result) = response["result"].as_str() {
            Ok(result.to_string())
        } else {
            Err("Failed to parse Jito response".into())
        }
    }

    pub fn get_tip_accounts(&self) -> &Vec<Pubkey> {
        &self.tip_accounts
    }
    
    pub fn get_random_tip_account(&self) -> &Pubkey {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.tip_accounts.len());
        &self.tip_accounts[index]
    }
}