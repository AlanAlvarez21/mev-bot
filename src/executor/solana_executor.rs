use crate::logging::Logger;
use reqwest;
use serde_json::{json, Value};
use std::str::FromStr;

pub struct SolanaExecutor {
    client: reqwest::Client,
    keypair_data: Vec<u8>,
    rpc_url: String,
    ws_url: String,
    use_jito: bool,
}

impl SolanaExecutor {
    pub fn new(rpc_url: String, ws_url: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Leer la clave privada desde el archivo
        let keypair_data_str = std::fs::read_to_string("solana-keypair.json")
            .map_err(|e| format!("Failed to read keypair file: {}", e))?;
        let keypair_data: Vec<u8> = serde_json::from_str(&keypair_data_str)
            .map_err(|e| format!("Failed to parse keypair: {}", e))?;

        // Verificar si se debe usar Jito
        let use_jito = std::env::var("USE_JITO")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase() == "true";

        Ok(Self {
            client: reqwest::Client::new(),
            keypair_data,
            rpc_url,
            ws_url,
            use_jito,
        })
    }

    pub async fn execute_frontrun(&self, target_tx_signature: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Attempting to frontrun transaction: {}", target_tx_signature));
        
        if self.use_jito {
            Logger::status_update("Using Jito for transaction priority");
            self.execute_frontrun_with_jito(target_tx_signature).await
        } else {
            Logger::status_update("Using standard RPC for transaction");
            // Crear una transacción firmada simulada
            let recent_blockhash = self.get_recent_blockhash().await?;
            let transaction_data = self.create_signed_transaction(&recent_blockhash)?;
            
            // Enviar la transacción
            let signature = self.send_transaction(&transaction_data).await?;
            
            Logger::status_update(&format!("Frontrun transaction sent: {}", signature));
            
            Ok(signature)
        }
    }

    async fn execute_frontrun_with_jito(&self, target_tx_signature: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Preparing Jito bundle for frontrun");
        
        // En una implementación completa, crearíamos una transacción real con tip
        // Por ahora, simulamos la creación de un bundle con Jito
        let recent_blockhash = self.get_recent_blockhash().await?;
        let transaction_data = self.create_signed_transaction(&recent_blockhash)?;
        
        // Enviar como bundle a través de Jito
        // Por ahora, simulamos el proceso ya que necesitamos implementar la lógica de creación real de transacciones
        Logger::status_update("Sending transaction via Jito RPC");
        let signature = self.send_transaction(&transaction_data).await?; // Usamos el método estándar por ahora
        
        Logger::status_update(&format!("Frontrun transaction sent via Jito: {}", signature));
        
        Ok(signature)
    }

    async fn get_recent_blockhash(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": []
        });

        let response: Value = self.client
            .post(&self.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = response.get("error") {
            return Err(format!("Get blockhash failed: {}", error).into());
        }

        if let Some(blockhash) = response["result"]["value"]["blockhash"].as_str() {
            Ok(blockhash.to_string())
        } else {
            Err("Failed to parse blockhash result".into())
        }
    }

    fn create_signed_transaction(&self, _blockhash: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Esto es una simplificación extrema - en una implementación real,
        // necesitaríamos crear una transacción firmada correctamente
        // usando la clave privada para firmar el mensaje de la transacción
        Logger::status_update("Creating signed transaction for frontrun");
        
        // En una implementación completa, usaríamos la clave privada para firmar una transacción real
        // Por ahora, retornamos un string base58 válido como placeholder
        Ok("5K6tJ76Y1i5Df589vgB8q5YM6bVrN5Qr5Mw6hYz79QVZ".to_string())
    }

    async fn send_transaction(&self, transaction_data: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                transaction_data,
                {
                    "skipPreflight": true,
                    "preflightCommitment": "confirmed"
                }
            ]
        });

        let response: Value = self.client
            .post(&self.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = response.get("error") {
            return Err(format!("Transaction failed: {}", error).into());
        }

        if let Some(result) = response["result"].as_str() {
            Ok(result.to_string())
        } else {
            Err("Failed to parse transaction result".into())
        }
    }
}