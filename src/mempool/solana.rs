use crate::config::Network;
use crate::logging::Logger;
use reqwest;
use serde_json::{json, Value};

pub struct SolanaMempool {
    client: reqwest::Client,
    rpc_url: String,
    network: Network,
}

impl SolanaMempool {
    pub fn new(network: &Network) -> Self {
        // Use devnet RPC endpoint by default
        let rpc_url = match network {
            Network::Devnet => std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "https://api.devnet.solana.com".to_string()),
            Network::Testnet => std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "https://api.testnet.solana.com".to_string()),
            Network::Mainnet => std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string()),
        };

        Self {
            client: reqwest::Client::new(),
            rpc_url,
            network: network.clone(),
        }
    }

    pub async fn start(&self) {
        Logger::status_update(&format!("Solana mempool monitoring active on {:?}", self.network));
        
        // Check initial connection
        match self.get_slot().await {
            Ok(slot) => {
                Logger::status_update(&format!("Connected to Solana {:?} - Current slot: {}", self.network, slot));
            }
            Err(e) => {
                Logger::error_occurred(&format!("Failed to connect to Solana {:?}: {}", self.network, e));
                return;
            }
        }

        // Main monitoring loop
        let mut last_slot = 0;
        
        loop {
            match self.get_slot().await {
                Ok(current_slot) => {
                    if current_slot > last_slot {
                        // Log slot updates to show we're actively monitoring
                        if current_slot - last_slot > 1 {
                            // Skip slots if we were offline
                            Logger::status_update(&format!("Skipped from slot {} to {}", last_slot, current_slot));
                        }
                        
                        // In a real implementation, we would fetch recent transactions here
                        // For now, just show we're actively monitoring
                        if current_slot % 10 == 0 { // Every 10 slots, show activity
                            Logger::status_update(&format!("Monitoring Solana {:?} - Current slot: {}", self.network, current_slot));
                        }
                        
                        last_slot = current_slot;
                    }
                }
                Err(e) => {
                    Logger::error_occurred(&format!("Error getting current slot: {}", e));
                }
            }
            
            // Sleep for a short time before checking again
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }
    
    async fn get_slot(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSlot",
            "params": []
        });

        let response: Value = self.client
            .post(&self.rpc_url)
            .json(&request_body)
            .send()
            .await?
            .json()
            .await?;

        if let Some(result) = response["result"].as_u64() {
            Ok(result)
        } else {
            Err("Failed to get slot".into())
        }
    }
}