use reqwest;
use serde_json::{json, Value};

pub struct JitoClient {
    client: reqwest::Client,
    jito_rpc_url: String,
    auth_header: Option<String>,
}

impl JitoClient {
    pub fn new() -> Option<Self> {
        let jito_rpc_url = std::env::var("JITO_RPC_URL").ok()?;
        
        // La autenticación para Jito normalmente requiere credenciales
        let auth_header = std::env::var("JITO_AUTH_HEADER").ok();
        
        Some(Self {
            client: reqwest::Client::new(),
            jito_rpc_url,
            auth_header,
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
}