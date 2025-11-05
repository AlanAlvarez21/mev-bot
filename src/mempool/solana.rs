use crate::config::Network;
use crate::logging::Logger;
use reqwest;
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{SinkExt, StreamExt};
use futures::StreamExt as FuturesStreamExt;
use crate::executor::solana_executor::SolanaExecutor;
use crate::utils::profitability_calculator::{ProfitabilityCalculator, OpportunityAnalysis};

pub struct SolanaMempool {
    client: reqwest::Client,
    rpc_url: String,
    ws_url: String,
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

        let ws_url = match network {
            Network::Devnet => std::env::var("SOLANA_WS_URL").unwrap_or_else(|_| "wss://api.devnet.solana.com".to_string()),
            Network::Testnet => std::env::var("SOLANA_WS_URL").unwrap_or_else(|_| "wss://api.testnet.solana.com".to_string()),
            Network::Mainnet => std::env::var("SOLANA_WS_URL").unwrap_or_else(|_| "wss://api.mainnet-beta.solana.com".to_string()),
        };

        Self {
            client: reqwest::Client::new(),
            rpc_url,
            ws_url,
            network: network.clone(),
        }
    }

    pub async fn start(&self) {
        Logger::status_update(&format!("Solana mempool monitoring active on {:?}", self.network));
        
        // Initialize Solana Executor
        let executor = match SolanaExecutor::new(self.rpc_url.clone(), self.ws_url.clone()) {
            Ok(exec) => exec,
            Err(e) => {
                Logger::error_occurred(&format!("Failed to initialize Solana Executor: {}", e));
                return;
            }
        };

        // Attempt to connect to WebSocket for real-time transaction monitoring
        match self.connect_ws(&executor).await {
            Ok(_) => {
                Logger::status_update("WebSocket connection established successfully");
            },
            Err(e) => {
                Logger::error_occurred(&format!("Failed to connect to WebSocket, falling back to slot monitoring: {}", e));
                // Fallback to the old method if WebSocket fails
                self.start_slot_monitoring(&executor).await;
            }
        }
    }

    async fn connect_ws(&self, executor: &SolanaExecutor) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (ws_stream, _) = connect_async(&self.ws_url).await
            .map_err(|e| format!("WebSocket connection failed: {}", e))?;
        
        let (mut ws_sender, ws_receiver) = ws_stream.split();
        
        // Subscribe to all transactions (this is a simplified approach)
        let subscription_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "logsSubscribe",
            "params": [
                "all",
                {
                    "commitment": "processed"
                }
            ]
        });
        
        ws_sender.send(Message::Text(subscription_request.to_string())).await
            .map_err(|e| format!("Failed to send subscription: {}", e))?;
        
        Logger::status_update("Subscribed to Solana transaction logs");
        
        // Process incoming messages
        let mut ws_receiver = ws_receiver;
        loop {
            match ws_receiver.next().await {
                Some(Ok(msg)) => {
                    if let Message::Text(text) = msg {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            if let Some(method) = value["method"].as_str() {
                                if method == "logsNotification" {
                                    if let Some(params) = value["params"].as_object() {
                                        if let Some(result) = params["result"].as_object() {
                                            if let Some(signature) = result["value"]["signature"].as_str() {
                                                Logger::status_update(&format!("Transaction detected: {}", signature));
                                                self.analyze_and_execute_opportunity(executor, signature).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some(Err(e)) => {
                    Logger::error_occurred(&format!("WebSocket error: {}", e));
                    break;
                }
                None => {
                    Logger::error_occurred("WebSocket stream ended unexpectedly");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn analyze_and_execute_opportunity(&self, executor: &SolanaExecutor, signature: &str) {
        // Simple detection logic - in a real implementation, this would analyze the transaction
        // for potential MEV opportunities like swaps, arbitrage, etc.
        Logger::opportunity_detected("Solana", signature);
        
        // Calculate if the opportunity is profitable before executing
        let opportunity_analysis = self.estimate_profitability(signature).await;
        
        if ProfitabilityCalculator::should_execute(&opportunity_analysis) {
            // Execute frontrun strategy based on detected opportunity
            match executor.execute_frontrun(signature).await {
                Ok(exec_signature) => {
                    Logger::bundle_sent("Solana", true);
                    Logger::status_update(&format!("Frontrun executed for transaction {}: {}", signature, exec_signature));
                }
                Err(e) => {
                    Logger::error_occurred(&format!("Frontrun failed for transaction {}: {}", signature, e));
                }
            }
        } else {
            Logger::status_update(&format!("Skipping unprofitable opportunity: {}", signature));
        }
    }
    
    async fn estimate_profitability(&self, signature: &str) -> OpportunityAnalysis {
        // En una implementación real, analizaríamos la transacción específica
        // para estimar el potencial de beneficio
        // Por ahora, simulamos el análisis basado en el hash de la transacción
        
        Logger::status_update(&format!("Analyzing profitability for transaction: {}", signature));
        
        // Simular el análisis de la transacción para estimar beneficios
        // En una implementación real, esto leería los datos de la transacción
        let fees = 0.005; // 0.005 SOL en fees promedio (taxas + Jito tips)
        
        // Simular el cálculo de potencial de beneficio basado en el hash
        let hash_numeric = self.signature_to_numeric(signature);
        let potential_profit = (hash_numeric % 10000) as f64 / 100000.0; // Convertir a SOL (0.0000 a 0.0999 SOL)
        
        // Asegurar un mínimo de beneficio potencial
        let potential_profit = potential_profit.max(0.001); // Mínimo 0.001 SOL
        
        Logger::status_update(&format!("Estimated profit potential: {:.6} SOL", potential_profit));
        
        ProfitabilityCalculator::analyze_frontrun(0.0, potential_profit, fees)
    }
    
    fn signature_to_numeric(&self, signature: &str) -> u64 {
        // Convertir parte del string de la firma a un número para simulación
        let cleaned = signature.chars().take(16).collect::<String>();
        let mut result = 0u64;
        
        for c in cleaned.chars() {
            result = result.wrapping_add(c as u64).wrapping_mul(31);
        }
        
        result
    }

    // Fallback method using slot monitoring
    async fn start_slot_monitoring(&self, executor: &SolanaExecutor) {
        Logger::status_update("Starting slot-based monitoring as fallback");
        
        let mut last_slot = 0;
        loop {
            match self.get_slot().await {
                Ok(current_slot) => {
                    if current_slot > last_slot {
                        // Simulate checking for transactions in the slot
                        if current_slot % 50 == 0 { // Every 50 slots, simulate an opportunity
                            Logger::opportunity_detected("Solana", &format!("simulated_tx_{}", current_slot));
                            
                            // Execute frontrun strategy
                            match executor.execute_frontrun(&format!("simulated_tx_{}", current_slot)).await {
                                Ok(signature) => {
                                    Logger::bundle_sent("Solana", true);
                                    Logger::status_update(&format!("Frontrun executed with signature: {}", signature));
                                }
                                Err(e) => {
                                    Logger::error_occurred(&format!("Frontrun failed: {}", e));
                                }
                            }
                        }
                        
                        // For now, just show we're actively monitoring
                        if current_slot % 10 == 0 { // Every 10 slots, show activity
                            Logger::status_update(&format!("Monitoring Solana {:?} - Current slot: {}", self.network, current_slot));
                        }
                        
                        last_slot = current_slot;
                    }
                }
                Err(_) => {
                    // Just continue the loop if there's an error getting the slot
                }
            }
            
            // Sleep for a short time before checking again
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }
    
    async fn get_slot(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
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
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(result) = response["result"].as_u64() {
            Ok(result)
        } else {
            Err("Failed to get slot".into())
        }
    }
}