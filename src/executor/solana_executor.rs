use crate::logging::Logger;
use reqwest;
use serde_json::{json, Value};
use crate::utils::jito::JitoClient;
use crate::utils::profit_calculator::ProfitCalculator;
use solana_sdk::{
    signature::{Keypair, Signer},
    pubkey::Pubkey,
    system_instruction,
    message::Message,
    transaction::Transaction,
    hash::Hash,
};
use std::str::FromStr;
use std::sync::Arc;
use crate::utils::risk_manager::RiskManager;
use crate::utils::analytics::Analytics;


#[derive(Clone)]
pub struct SolanaExecutor {
    client: Arc<reqwest::Client>,
    keypair_data: Vec<u8>,
    rpc_url: String,
    ws_url: String,
    use_jito: bool,
    profit_calculator: ProfitCalculator,
    max_loss_per_bundle: f64,  // Máxima pérdida aceptable por bundle
    min_balance: f64,          // Saldo mínimo para continuar operaciones
    risk_manager: Arc<RiskManager>,  // Wrap in Arc for shared access
    analytics: Arc<tokio::sync::Mutex<Analytics>>,
}

impl SolanaExecutor {
    pub fn new(rpc_url: String, ws_url: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Leer la clave privada desde el archivo
        let keypair_data_str = std::fs::read_to_string("solana-keypair.json")
            .map_err(|e| {
                let error_msg = format!("Failed to read keypair file: {}. Make sure the file exists and has correct permissions.", e);
                Logger::error_occurred(&error_msg);
                error_msg
            })?;
        let keypair_data: Vec<u8> = serde_json::from_str(&keypair_data_str)
            .map_err(|e| {
                let error_msg = format!("Failed to parse keypair: {}. Check that the file contains valid JSON array of bytes.", e);
                Logger::error_occurred(&error_msg);
                error_msg
            })?;

        // Verificar si se debe usar Jito
        let use_jito = std::env::var("USE_JITO")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase() == "true";
            
        // Configurar límites de riesgo desde variables de entorno o valores por defecto
        let max_loss_per_bundle = std::env::var("MAX_LOSS_PER_BUNDLE")
            .unwrap_or_else(|_| "0.1".to_string()) // 0.1 SOL por bundle como máximo de pérdida
            .parse::<f64>()
            .unwrap_or(0.1);
            
        let min_balance = std::env::var("MIN_BALANCE")
            .unwrap_or_else(|_| "0.5".to_string()) // 0.5 SOL como saldo mínimo
            .parse::<f64>()
            .unwrap_or(0.5);

        let risk_manager = Arc::new(RiskManager::new());
        let analytics = Arc::new(tokio::sync::Mutex::new(Analytics::new()));

        Ok(Self {
            client: Arc::new(reqwest::Client::new()),
            keypair_data,
            rpc_url,
            ws_url,
            use_jito,
            profit_calculator: ProfitCalculator::new(),
            max_loss_per_bundle,
            min_balance,
            risk_manager,
            analytics,
        })
    }

    // Fix the fees issue in the frontrun function
    async fn record_transaction_analytics(&self, strategy: &str, success: bool, profit: f64, fees: f64) {
        let mut analytics = self.analytics.lock().await;
        analytics.record_transaction(strategy, success, profit, fees);
    }
    
    async fn record_opportunity_analytics(&self, opportunity_type: &str, executed: bool, profitable: bool, profit: f64, execution_time_ms: f64) {
        let mut analytics = self.analytics.lock().await;
        analytics.record_opportunity(opportunity_type, executed, profitable, profit, execution_time_ms);
    }
} // Close first impl block

impl SolanaExecutor {
    // Método para usar ws_url y keypair_data (eliminar warnings)
    pub fn get_ws_url(&self) -> &str {
        &self.ws_url
    }

    pub fn get_keypair_public_key(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if self.keypair_data.is_empty() {
            return Err("Keypair data is empty".into());
        }
        
        let keypair = Keypair::from_bytes(&self.keypair_data)
            .map_err(|e| format!("Invalid keypair data: {}", e))?;
        let pubkey = keypair.pubkey();
        
        Ok(pubkey.to_string())
    }
    
    // Método para verificar si debemos continuar operando según los parámetros de riesgo
    async fn should_continue_operation(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Obtener el saldo actual (esto debería actualizarse periódicamente en una implementación real)
        let current_balance = self.get_balance().await?;
        
        if current_balance < self.min_balance {
            Logger::error_occurred(&format!(
                "Balance too low: {:.6} SOL (minimum required: {:.6} SOL)", 
                current_balance, 
                self.min_balance
            ));
            return Ok(false);
        }
        
        Logger::status_update(&format!("Current balance: {:.6} SOL, minimum required: {:.6} SOL", 
                                     current_balance, self.min_balance));
        Ok(true)
    }
    
    // Additional risk management checks
    async fn additional_safety_checks(
        &self, 
        estimated_profit: f64, 
        fees: f64, 
        tip_amount: f64
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let total_costs = fees + tip_amount;
        
        // Check that estimated profit is meaningful (not extremely small)
        if estimated_profit < 0.001 {
            Logger::status_update("Skipping opportunity: estimated profit too small (< 0.001 SOL)");
            return Ok(false);
        }
        
        // Check that net profit is reasonable compared to costs
        let net_profit = estimated_profit - total_costs;
        if net_profit <= 0.0 {
            Logger::status_update("Skipping opportunity: net profit is not positive");
            return Ok(false);
        }
        
        // Check profit-to-cost ratio
        if estimated_profit / total_costs < 1.2 { // Require 20% more profit than costs
            Logger::status_update(&format!(
                "Skipping opportunity: profit-to-cost ratio too low ({:.2})", 
                estimated_profit / total_costs
            ));
            return Ok(false);
        }
        
        // Additional check for potential slippage or market impact
        if estimated_profit > 0.5 {  // If potential profit is very high, it might be unrealistic
            Logger::status_update("Skipping opportunity: unusually high estimated profit (>0.5 SOL), likely unrealistic");
            return Ok(false);
        }
        
        Ok(true)
    }
    
    // Método para obtener el saldo actual de la billetera
    async fn get_balance(&self) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // Derivar la clave pública del par de claves
        let keypair = Keypair::from_bytes(&self.keypair_data)
            .map_err(|e| format!("Invalid keypair data: {}", e))?;
        let pubkey = keypair.pubkey();
        let pubkey_str = pubkey.to_string();
        
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBalance",
            "params": [pubkey_str]
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
            return Err(format!("Get balance failed: {}", error).into());
        }

        if let Some(value) = response["result"]["value"].as_f64() {
            // Convertir de lamports a SOL (1 SOL = 1000000000 lamports)
            let sol_balance = value / 1_000_000_000.0;
            Ok(sol_balance)
        } else {
            Err("Failed to parse balance result".into())
        }
    }

    pub async fn execute_frontrun(
        &self, 
        target_tx_signature: &str, 
        estimated_profit: f64,
        target_tx_details: Option<&Value>  // Include target transaction details for better strategy
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Attempting to frontrun transaction: {}, with estimated profit: {:.6} SOL", target_tx_signature, estimated_profit));
        
        let start_time = std::time::Instant::now();
        
        // Verificar si debemos continuar operando según los parámetros de riesgo
        if !self.should_continue_operation().await? {
            self.record_transaction_analytics("frontrun", false, estimated_profit, 0.005).await;
            return Err("Operation halted due to risk management parameters".into());
        }
        
        let fees_result = self.calculate_transaction_fees().await;
        let fees = match fees_result {
            Ok(fee_value) => fee_value,
            Err(e) => {
                let error_msg = format!("Failed to calculate transaction fees: {}", e);
                Logger::error_occurred(&error_msg);
                self.record_transaction_analytics("frontrun", false, -0.005, 0.005).await; // Use default fees value
                return Err(e);
            }
        };
        
        let tip_amount = if self.use_jito { 0.001 } else { 0.0 }; // 0.001 SOL como propina para Jito
        let total_cost = fees + tip_amount;
        
        // Check with risk manager if this transaction should be allowed
        if !self.risk_manager.should_allow_transaction(estimated_profit, total_cost) {
            Logger::status_update("Transaction rejected by risk manager");
            self.record_transaction_analytics("frontrun", false, -total_cost, total_cost).await;
            return Err("Transaction rejected by risk manager".into());
        }
        
        let analysis = self.profit_calculator.calculate_profitability(estimated_profit, fees, tip_amount);
        
        // Additional safety check: prevent execution if estimated profit is non-positive
        if estimated_profit <= 0.0 {
            Logger::status_update(&format!(
                "Skipping opportunity with no positive profit potential: estimated profit {:.6} SOL", 
                estimated_profit
            ));
            self.record_transaction_analytics("frontrun", false, -total_cost, total_cost).await;
            return Err("No positive profit potential".into());
        }
        
        // Run additional safety checks
        let safety_ok = self.additional_safety_checks(estimated_profit, fees, tip_amount).await?;
        if !safety_ok {
            Logger::status_update("Skipping opportunity: failed additional safety checks");
            self.record_transaction_analytics("frontrun", false, -total_cost, total_cost).await;
            return Err("Failed additional safety checks".into());
        }
        
        // Verificar límites de riesgo adicionales
        if !analysis.is_profitable {
            Logger::status_update(&format!(
                "Skipping unprofitable opportunity: net profit {:.6} SOL vs minimum required {:.6} SOL", 
                analysis.net_profit, 
                estimated_profit * self.profit_calculator.min_profit_margin
            ));
            self.record_transaction_analytics("frontrun", false, -total_cost, total_cost).await;
            return Err("Opportunity not profitable".into());
        }
        
        // Verificar que el potencial de pérdida no exceda el límite configurado
        if analysis.net_profit < -self.max_loss_per_bundle {
            Logger::status_update(&format!(
                "Skipping high-risk opportunity: potential loss {:.6} SOL exceeds max allowed loss {:.6} SOL", 
                -analysis.net_profit, 
                self.max_loss_per_bundle
            ));
            self.record_transaction_analytics("frontrun", false, -total_cost, total_cost).await;
            return Err("Opportunity exceeds maximum allowed loss".into());
        }
        
        Logger::status_update(&format!(
            "Profitable opportunity: estimated profit {:.6} SOL, fees {:.6} SOL, net profit {:.6} SOL",
            analysis.estimated_profit,
            analysis.total_costs,
            analysis.net_profit
        ));
        
        let result = if self.use_jito {
            Logger::status_update("Using Jito for transaction priority");
            self.execute_frontrun_with_jito(target_tx_signature, target_tx_details).await
        } else {
            Logger::status_update("Using standard RPC for transaction");
            // Crear una transacción firmada basada en estrategia MEV
            let recent_blockhash_result = self.get_recent_blockhash().await;
            let recent_blockhash = match recent_blockhash_result {
                Ok(hash) => hash,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to get recent blockhash: {}", e));
                    return Err(e);
                }
            };
            
            let transaction_data = match self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await {
                Ok(data) => data,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to create MEV strategy transaction: {}", e));
                    return Err(e);
                }
            };
            
            // Enviar la transacción
            let signature_result = self.send_transaction(&transaction_data).await;
            match signature_result {
                Ok(signature) => {
                    Logger::status_update(&format!("Frontrun transaction sent: {}", signature));
                    Ok(signature)
                },
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to send frontrun transaction: {}", e));
                    Err(e)
                }
            }
        };
        
        // Registrar resultados de la ejecución
        let execution_time = start_time.elapsed().as_millis() as f64;
        match &result {
            Ok(signature) => {
                Logger::status_update(&format!("Frontrun successful: {}", signature));
                // Record success in analytics
                self.record_transaction_analytics("frontrun", true, estimated_profit - total_cost, total_cost).await;
                self.record_opportunity_analytics("frontrun", true, true, estimated_profit, execution_time).await;
            },
            Err(e) => {
                Logger::error_occurred(&format!("Frontrun failed: {}", e));
                self.record_transaction_analytics("frontrun", false, -total_cost, total_cost).await;
                self.record_opportunity_analytics("frontrun", true, false, -total_cost, execution_time).await;
            }
        };
        
        result
    }
    


    async fn execute_frontrun_with_jito(&self, _target_tx_signature: &str, target_tx_details: Option<&Value>) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Preparing Jito bundle for frontrun");
        
        let recent_blockhash_result = self.get_recent_blockhash().await;
        let recent_blockhash = match recent_blockhash_result {
            Ok(hash) => hash,
            Err(e) => {
                let error_msg = format!("Failed to get recent blockhash for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create the main transaction for the frontrun (without tip)
        let main_transaction_data_result = self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await;
        let main_transaction_data = match main_transaction_data_result {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to create MEV strategy transaction for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create a tip transaction to one of Jito's tip accounts
        let tip_transaction_data_result = self.create_tip_transaction(&recent_blockhash)?;
        let tip_transaction_data = tip_transaction_data_result;
        
        // Combine both transactions for the bundle
        let transactions = vec![main_transaction_data.clone(), tip_transaction_data];
        
        // Usar Jito para enviar el bundle si está disponible
        match JitoClient::new() {
            Some(jito_client) => {
                Logger::status_update("Sending bundle via Jito");
                match jito_client.send_bundle(&transactions).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Jito bundle sent successfully: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send Jito bundle: {}, falling back to standard RPC", e);
                        Logger::error_occurred(&error_msg);
                        // Volver al RPC estándar si falla Jito
                        self.send_transaction(&main_transaction_data).await
                    }
                }
            }
            None => {
                Logger::status_update("Jito not configured, using standard RPC");
                match self.send_transaction(&main_transaction_data).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Transaction sent via standard RPC: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send transaction via standard RPC: {}", e);
                        Logger::error_occurred(&error_msg);
                        Err(e)
                    }
                }
            }
        }
    }

    async fn get_recent_blockhash(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": []
        });

        let response_result = self.client
            .post(&self.rpc_url)
            .json(&request_body)
            .send()
            .await;
            
        let response: Value = match response_result {
            Ok(resp) => resp.json().await.map_err(|e| {
                let error_msg = format!("Failed to parse JSON response for blockhash: {}", e);
                Logger::error_occurred(&error_msg);
                error_msg
            })?,
            Err(e) => {
                let error_msg = format!("HTTP request failed to get blockhash: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(error_msg.into());
            }
        };

        if let Some(error) = response.get("error") {
            let error_msg = format!("Get blockhash failed: {}", error);
            Logger::error_occurred(&error_msg);
            return Err(error_msg.into());
        }

        match response["result"]["value"]["blockhash"].as_str() {
            Some(blockhash) => Ok(blockhash.to_string()),
            None => {
                let error_msg = "Failed to parse blockhash result from response".to_string();
                Logger::error_occurred(&error_msg);
                Err(error_msg.into())
            }
        }
    }

    async fn calculate_transaction_fees(&self) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // Obtener el costo actual de las transacciones de la red
        // En una implementación completa, consultaríamos el estado actual de la red
        // Por ahora, retornamos un valor estimado basado en condiciones típicas de la red
        
        // En una implementación completa, haríamos una llamada RPC para obtener tarifas actuales
        match self.fetch_current_fees().await {
            Ok(fees) => Ok(fees),
            Err(_) => {
                // Si falla, usamos un valor predeterminado
                Logger::status_update("Using default transaction fees due to RPC failure");
                Ok(0.005) // 0.005 SOL como tarifa base promedio
            }
        }
    }
    
    async fn fetch_current_fees(&self) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getRecentPrioritizationFees",
            "params": []
        });

        let response_result = self.client
            .post(&self.rpc_url)
            .json(&request_body)
            .send()
            .await;
            
        match response_result {
            Ok(resp) => {
                let response: Value = resp.json().await.map_err(|e| {
                    let error_msg = format!("Failed to parse JSON response for fees: {}", e);
                    Logger::error_occurred(&error_msg);
                    error_msg
                })?;
                
                if let Some(error) = response.get("error") {
                    let error_msg = format!("Get fees failed: {}", error);
                    Logger::error_occurred(&error_msg);
                    return Err(error_msg.into());
                }
                
                // Por simplicidad, retornamos un valor fijo en esta implementación
                Ok(0.005)
            },
            Err(e) => {
                let error_msg = format!("HTTP request failed to get current fees: {}", e);
                Logger::error_occurred(&error_msg);
                Err(error_msg.into())
            }
        }
    }

    fn estimate_profit_from_target(&self, target_tx_signature: &str) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // This function should not be estimating profit based on signature alone
        // In a real MEV bot, this would be handled by the mempool analysis
        // which would determine actual profit potential
        
        // Since this function is being called from the executor, 
        // we should return a conservative estimate or 0
        // The actual profit estimation should happen in the mempool analysis phase
        // where we can analyze the target transaction for real MEV opportunities
        
        Logger::status_update(&format!(
            "WARNING: estimate_profit_from_target called with signature {}, this indicates potential logic error.", 
            target_tx_signature
        ));
        
        // Return 0 to indicate no profit potential from this approach
        // Real profit estimation should happen in the mempool analysis phase
        Ok(0.0)
    }

    fn create_signed_transaction(&self, blockhash: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // ESTA ES LA PARTE CLAVE - IMPLEMENTACIÓN REAL DE TRANSACCIÓN FIRMAADA
        // Now creating a more realistic transaction for MEV strategies
        
        // Usamos keypair_data para demostrar que está siendo usado
        if self.keypair_data.is_empty() {
            return Err("Keypair data is empty".into());
        }
        
        Logger::status_update(&format!("Creating signed transaction for MEV strategy with blockhash: {}", blockhash));
        
        // Usamos solana-sdk para crear una transacción firmada real
        use solana_sdk::{
            signature::{Keypair, Signer},
            pubkey::Pubkey,
            system_instruction,
            message::Message,
            transaction::Transaction,
            hash::Hash,
        };
        
        let keypair = Keypair::from_bytes(&self.keypair_data)
            .map_err(|e| format!("Invalid keypair data: {}", e))?;
        
        // For a more realistic MEV strategy, we'd create a swap transaction
        // but since we don't have context about the target, we'll create a minimal transaction
        // with a slightly more realistic approach
        let recipient = keypair.pubkey(); // Send to self instead of random address
        let instruction = system_instruction::transfer(
            &keypair.pubkey(),
            &recipient,
            1000, // 0.000001 SOL - minimal transfer to show activity
        );
        
        let message = Message::new(
            &[instruction],
            Some(&keypair.pubkey()),
        );
        
        let blockhash = Hash::from_str(blockhash)
            .map_err(|e| format!("Invalid blockhash: {}", e))?;
        
        let transaction = Transaction::new(
            &[&keypair],
            message,
            blockhash,
        );
        
        let serialized_tx = bincode::serialize(&transaction)
            .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
        
        let encoded_tx = bs58::encode(serialized_tx).into_string();
        
        Ok(encoded_tx)
    }

    async fn create_mev_strategy_transaction(
        &self,
        blockhash: &str,
        target_tx_details: Option<&Value>
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Creating MEV strategy transaction based on target transaction details");
        
        if self.keypair_data.is_empty() {
            return Err("Keypair data is empty".into());
        }

        use solana_sdk::{
            signature::{Keypair, Signer},
            message::Message,
            transaction::Transaction,
            hash::Hash,
        };
        
        let keypair = Keypair::from_bytes(&self.keypair_data)
            .map_err(|e| format!("Invalid keypair data: {}", e))?;
        
        // Analyze the target transaction to determine the best strategy
        let instructions = if let Some(target_details) = target_tx_details {
            // Extract information from the target transaction to build an appropriate response
            self.create_strategy_instructions(&keypair, target_details).await?
        } else {
            // Default fallback if no target transaction details available
            vec![system_instruction::transfer(
                &keypair.pubkey(),
                &keypair.pubkey(), // Send to self to minimize risk
                1000, // Minimal amount
            )]
        };
        
        let message = Message::new(
            &instructions,
            Some(&keypair.pubkey()),
        );
        
        // Parse blockhash faster
        use std::str::FromStr;
        let blockhash = Hash::from_str(blockhash)
            .map_err(|e| format!("Invalid blockhash: {}", e))?;
        
        let transaction = Transaction::new(
            &[&keypair],
            message,
            blockhash,
        );
        
        let serialized_tx = bincode::serialize(&transaction)
            .map_err(|e| format!("Failed to serialize MEV strategy transaction: {}", e))?;
        
        let encoded_tx = bs58::encode(serialized_tx).into_string();
        
        Logger::status_update(&format!("MEV strategy transaction created with length: {}", encoded_tx.len()));
        
        Ok(encoded_tx)
    }
    
    async fn create_strategy_instructions(
        &self,
        keypair: &Keypair,
        target_tx_details: &Value,
    ) -> Result<Vec<solana_sdk::instruction::Instruction>, Box<dyn std::error::Error + Send + Sync>> {
        // Analyze the target transaction to determine which MEV strategy to implement
        
        // Check if it's a swap transaction by looking at the instructions
        if let Some(transaction) = target_tx_details.get("transaction") {
            if let Some(message) = transaction.get("message") {
                if let Some(instructions) = message.get("instructions") {
                    if let Some(instr_array) = instructions.as_array() {
                        for instruction in instr_array {
                            if let Some(accounts) = instruction.get("accounts").and_then(|v| v.as_array()) {
                                // DEX swaps typically have multiple accounts (token accounts, vaults, etc.)
                                if accounts.len() >= 4 {
                                    // This looks like a swap transaction - implement appropriate strategy
                                    return self.create_arbitrage_or_frontrun_instructions(keypair, target_tx_details).await;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Default to a simple transfer if no specific strategy can be determined
        Ok(vec![system_instruction::transfer(
            &keypair.pubkey(),
            &keypair.pubkey(), // Send to self to minimize risk
            1000, // Minimal amount
        )])
    }
    
    async fn create_arbitrage_or_frontrun_instructions(
        &self,
        keypair: &Keypair,
        target_tx_details: &Value,
    ) -> Result<Vec<solana_sdk::instruction::Instruction>, Box<dyn std::error::Error + Send + Sync>> {
        // This would create actual DEX swap instructions for arbitrage or frontrunning
        // For now, we'll create more realistic placeholder instructions
        
        // In a real implementation, this would:
        // 1. Analyze the target swap
        // 2. Get current pool states from Raydium, Orca, etc.
        // 3. Create swap instructions to exploit price differences
        // 4. Use Jupiter API for optimal routing if needed
        
        use solana_sdk::system_instruction;
        
        // Example: Create a sequence of instructions that would perform an arbitrage
        // This is still a placeholder but more representative of what real MEV would look like
        let instructions = vec![
            system_instruction::transfer(
                &keypair.pubkey(),
                &keypair.pubkey(), // Placeholder for swap input
                5000, // More substantial amount
            ),
            system_instruction::transfer(
                &keypair.pubkey(),
                &keypair.pubkey(), // Placeholder for swap output
                1000, // Placeholder for output 
            )
        ];
        
        Ok(instructions)
    }

    fn create_tip_transaction(&self, blockhash: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Creating tip transaction for Jito bundle");
        
        if self.keypair_data.is_empty() {
            return Err("Keypair data is empty".into());
        }

        use solana_sdk::{
            signature::{Keypair, Signer},
            system_instruction,
            message::Message,
            transaction::Transaction,
            hash::Hash,
        };
        
        let keypair = Keypair::from_bytes(&self.keypair_data)
            .map_err(|e| format!("Invalid keypair data: {}", e))?;
        
        // Get a Jito tip account from the JitoClient
        let jito_client = JitoClient::new().ok_or("Jito client not initialized")?;
        let tip_recipient = jito_client.get_random_tip_account();
        
        Logger::status_update(&format!("Using tip account: {}", tip_recipient));
        
        // Send a small tip (0.001 SOL) to the Jito tip account
        let tip_amount = 1_000_000; // 0.001 SOL in lamports
        let tip_instruction = system_instruction::transfer(
            &keypair.pubkey(),
            tip_recipient,
            tip_amount,
        );
        
        let message = Message::new(
            &[tip_instruction],
            Some(&keypair.pubkey()),
        );
        
        let blockhash = Hash::from_str(blockhash)
            .map_err(|e| format!("Invalid blockhash: {}", e))?;
        
        let transaction = Transaction::new(
            &[&keypair],
            message,
            blockhash,
        );
        
        let serialized_tx = bincode::serialize(&transaction)
            .map_err(|e| format!("Failed to serialize tip transaction: {}", e))?;
        
        let encoded_tx = bs58::encode(serialized_tx).into_string();
        
        Logger::status_update(&format!("Tip transaction created with length: {}", encoded_tx.len()));
        
        Ok(encoded_tx)
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

    pub async fn execute_sandwich(
        &self, 
        target_tx_signature: &str, 
        estimated_profit: f64,
        target_tx_details: Option<&Value>  // Include target transaction details for better strategy
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Attempting to execute sandwich for transaction: {}, with estimated profit: {:.6} SOL", target_tx_signature, estimated_profit));
        
        let start_time = std::time::Instant::now();
        
        // NEW ARCHITECTURE: This functionality should be handled by SolanaMempool
        // For now, fall back to the original implementation
        Logger::status_update("Executing sandwich using fallback logic");
        
        let start_time = std::time::Instant::now();
        
        // Verificar si debemos continuar operando según los parámetros de riesgo
        if !self.should_continue_operation().await? {
            self.record_transaction_analytics("sandwich", false, estimated_profit, 0.005).await;
            return Err("Operation halted due to risk management parameters".into());
        }
        
        let fees_result = self.calculate_transaction_fees().await;
        let fees = match fees_result {
            Ok(fee_value) => fee_value,
            Err(e) => {
                let error_msg = format!("Failed to calculate transaction fees: {}", e);
                Logger::error_occurred(&error_msg);
                self.record_transaction_analytics("sandwich", false, -0.005, 0.005).await; // Use default fees value
                return Err(e);
            }
        };
        
        let tip_amount = if self.use_jito { 0.001 } else { 0.0 }; // 0.001 SOL como propina para Jito
        let total_cost = fees + tip_amount;
        
        // Check with risk manager if this transaction should be allowed
        if !self.risk_manager.should_allow_transaction(estimated_profit, total_cost) {
            Logger::status_update("Transaction rejected by risk manager");
            self.record_transaction_analytics("sandwich", false, -total_cost, total_cost).await;
            return Err("Transaction rejected by risk manager".into());
        }
        
        let analysis = self.profit_calculator.calculate_profitability(estimated_profit, fees, tip_amount);
        
        // Additional safety check: prevent execution if estimated profit is non-positive
        if estimated_profit <= 0.0 {
            Logger::status_update(&format!(
                "Skipping opportunity with no positive profit potential: estimated profit {:.6} SOL", 
                estimated_profit
            ));
            self.record_transaction_analytics("sandwich", false, -total_cost, total_cost).await;
            return Err("No positive profit potential".into());
        }
        
        // Run additional safety checks
        let safety_ok = self.additional_safety_checks(estimated_profit, fees, tip_amount).await?;
        if !safety_ok {
            Logger::status_update("Skipping opportunity: failed additional safety checks");
            self.record_transaction_analytics("sandwich", false, -total_cost, total_cost).await;
            return Err("Failed additional safety checks".into());
        }
        
        // Verificar límites de riesgo adicionales
        if !analysis.is_profitable {
            Logger::status_update(&format!(
                "Skipping unprofitable opportunity: net profit {:.6} SOL vs minimum required {:.6} SOL", 
                analysis.net_profit, 
                estimated_profit * self.profit_calculator.min_profit_margin
            ));
            self.record_transaction_analytics("sandwich", false, -total_cost, total_cost).await;
            return Err("Opportunity not profitable".into());
        }
        
        // Verificar que el potencial de pérdida no exceda el límite configurado
        if analysis.net_profit < -self.max_loss_per_bundle {
            Logger::status_update(&format!(
                "Skipping high-risk opportunity: potential loss {:.6} SOL exceeds max allowed loss {:.6} SOL", 
                -analysis.net_profit, 
                self.max_loss_per_bundle
            ));
            self.record_transaction_analytics("sandwich", false, -total_cost, total_cost).await;
            return Err("Opportunity exceeds maximum allowed loss".into());
        }
        
        Logger::status_update(&format!(
            "Profitable opportunity: estimated profit {:.6} SOL, fees {:.6} SOL, net profit {:.6} SOL",
            analysis.estimated_profit,
            analysis.total_costs,
            analysis.net_profit
        ));
        
        let result = if self.use_jito {
            Logger::status_update("Using Jito for transaction priority");
            self.execute_sandwich_with_jito(target_tx_signature, target_tx_details).await
        } else {
            Logger::status_update("Using standard RPC for transaction");
            // Crear una transacción firmada basada en estrategia MEV
            let recent_blockhash_result = self.get_recent_blockhash().await;
            let recent_blockhash = match recent_blockhash_result {
                Ok(hash) => hash,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to get recent blockhash: {}", e));
                    return Err(e);
                }
            };
            
            let transaction_data = match self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await {
                Ok(data) => data,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to create MEV strategy transaction: {}", e));
                    return Err(e);
                }
            };
            
            // Enviar la transacción
            let signature_result = self.send_transaction(&transaction_data).await;
            match signature_result {
                Ok(signature) => {
                    Logger::status_update(&format!("Sandwich transaction sent: {}", signature));
                    Ok(signature)
                },
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to send sandwich transaction: {}", e));
                    Err(e)
                }
            }
        };
        
        // Registrar resultados de la ejecución
        let execution_time = start_time.elapsed().as_millis() as f64;
        match &result {
            Ok(signature) => {
                Logger::status_update(&format!("Sandwich successful: {}", signature));
                // Record success in analytics
                self.record_transaction_analytics("sandwich", true, estimated_profit - total_cost, total_cost).await;
                self.record_opportunity_analytics("sandwich", true, true, estimated_profit, execution_time).await;
            },
            Err(e) => {
                Logger::error_occurred(&format!("Sandwich failed: {}", e));
                self.record_transaction_analytics("sandwich", false, -total_cost, total_cost).await;
                self.record_opportunity_analytics("sandwich", true, false, -total_cost, execution_time).await;
            }
        };
        
        result
    }

    async fn execute_sandwich_with_jito(&self, _target_tx_signature: &str, target_tx_details: Option<&Value>) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Preparing Jito bundle for sandwich");
        
        let recent_blockhash_result = self.get_recent_blockhash().await;
        let recent_blockhash = match recent_blockhash_result {
            Ok(hash) => hash,
            Err(e) => {
                let error_msg = format!("Failed to get recent blockhash for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create the main transaction for the sandwich (without tip)
        let main_transaction_data_result = self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await;
        let main_transaction_data = match main_transaction_data_result {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to create MEV strategy transaction for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create a tip transaction to one of Jito's tip accounts
        let tip_transaction_data_result = self.create_tip_transaction(&recent_blockhash)?;
        let tip_transaction_data = tip_transaction_data_result;
        
        // Combine both transactions for the bundle
        let transactions = vec![main_transaction_data.clone(), tip_transaction_data];
        
        // Usar Jito para enviar el bundle si está disponible
        match JitoClient::new() {
            Some(jito_client) => {
                Logger::status_update("Sending sandwich bundle via Jito");
                match jito_client.send_bundle(&transactions).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Jito sandwich bundle sent successfully: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send Jito bundle: {}, falling back to standard RPC", e);
                        Logger::error_occurred(&error_msg);
                        // Volver al RPC estándar si falla Jito
                        self.send_transaction(&main_transaction_data).await
                    }
                }
            }
            None => {
                Logger::status_update("Jito not configured, using standard RPC for sandwich");
                match self.send_transaction(&main_transaction_data).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Sandwich transaction sent via standard RPC: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send sandwich transaction via standard RPC: {}", e);
                        Logger::error_occurred(&error_msg);
                        Err(e)
                    }
                }
            }
        }
    }

    pub async fn execute_arbitrage(
        &self, 
        target_tx_signature: &str, 
        estimated_profit: f64,
        target_tx_details: Option<&Value>  // Include target transaction details for better strategy
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Attempting to execute arbitrage for transaction: {}, with estimated profit: {:.6} SOL", target_tx_signature, estimated_profit));
        
        let start_time = std::time::Instant::now();
        
        // NEW ARCHITECTURE: This functionality should be handled by SolanaMempool
        // For now, fall back to the original implementation
        Logger::status_update("Executing arbitrage using fallback logic");
        
        let start_time = std::time::Instant::now();
        
        // Verificar si debemos continuar operando según los parámetros de riesgo
        if !self.should_continue_operation().await? {
            self.record_transaction_analytics("arbitrage", false, estimated_profit, 0.005).await;
            return Err("Operation halted due to risk management parameters".into());
        }
        
        let fees_result = self.calculate_transaction_fees().await;
        let fees = match fees_result {
            Ok(fee_value) => fee_value,
            Err(e) => {
                let error_msg = format!("Failed to calculate transaction fees: {}", e);
                Logger::error_occurred(&error_msg);
                self.record_transaction_analytics("arbitrage", false, -0.005, 0.005).await; // Use default fees value
                return Err(e);
            }
        };
        
        let tip_amount = if self.use_jito { 0.001 } else { 0.0 }; // 0.001 SOL como propina para Jito
        let total_cost = fees + tip_amount;
        
        // Check with risk manager if this transaction should be allowed
        if !self.risk_manager.should_allow_transaction(estimated_profit, total_cost) {
            Logger::status_update("Transaction rejected by risk manager");
            self.record_transaction_analytics("arbitrage", false, -total_cost, total_cost).await;
            return Err("Transaction rejected by risk manager".into());
        }
        
        let analysis = self.profit_calculator.calculate_profitability(estimated_profit, fees, tip_amount);
        
        // Additional safety check: prevent execution if estimated profit is non-positive
        if estimated_profit <= 0.0 {
            Logger::status_update(&format!(
                "Skipping opportunity with no positive profit potential: estimated profit {:.6} SOL", 
                estimated_profit
            ));
            self.record_transaction_analytics("arbitrage", false, -total_cost, total_cost).await;
            return Err("No positive profit potential".into());
        }
        
        // Run additional safety checks
        let safety_ok = self.additional_safety_checks(estimated_profit, fees, tip_amount).await?;
        if !safety_ok {
            Logger::status_update("Skipping opportunity: failed additional safety checks");
            self.record_transaction_analytics("arbitrage", false, -total_cost, total_cost).await;
            return Err("Failed additional safety checks".into());
        }
        
        // Verificar límites de riesgo adicionales
        if !analysis.is_profitable {
            Logger::status_update(&format!(
                "Skipping unprofitable opportunity: net profit {:.6} SOL vs minimum required {:.6} SOL", 
                analysis.net_profit, 
                estimated_profit * self.profit_calculator.min_profit_margin
            ));
            self.record_transaction_analytics("arbitrage", false, -total_cost, total_cost).await;
            return Err("Opportunity not profitable".into());
        }
        
        // Verificar que el potencial de pérdida no exceda el límite configurado
        if analysis.net_profit < -self.max_loss_per_bundle {
            Logger::status_update(&format!(
                "Skipping high-risk opportunity: potential loss {:.6} SOL exceeds max allowed loss {:.6} SOL", 
                -analysis.net_profit, 
                self.max_loss_per_bundle
            ));
            self.record_transaction_analytics("arbitrage", false, -total_cost, total_cost).await;
            return Err("Opportunity exceeds maximum allowed loss".into());
        }
        
        Logger::status_update(&format!(
            "Profitable opportunity: estimated profit {:.6} SOL, fees {:.6} SOL, net profit {:.6} SOL",
            analysis.estimated_profit,
            analysis.total_costs,
            analysis.net_profit
        ));
        
        let result = if self.use_jito {
            Logger::status_update("Using Jito for transaction priority");
            self.execute_arbitrage_with_jito(target_tx_signature, target_tx_details).await
        } else {
            Logger::status_update("Using standard RPC for transaction");
            // Crear una transacción firmada basada en estrategia MEV
            let recent_blockhash_result = self.get_recent_blockhash().await;
            let recent_blockhash = match recent_blockhash_result {
                Ok(hash) => hash,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to get recent blockhash: {}", e));
                    return Err(e);
                }
            };
            
            let transaction_data = match self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await {
                Ok(data) => data,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to create MEV strategy transaction: {}", e));
                    return Err(e);
                }
            };
            
            // Enviar la transacción
            let signature_result = self.send_transaction(&transaction_data).await;
            match signature_result {
                Ok(signature) => {
                    Logger::status_update(&format!("Arbitrage transaction sent: {}", signature));
                    Ok(signature)
                },
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to send arbitrage transaction: {}", e));
                    Err(e)
                }
            }
        };
        
        // Registrar resultados de la ejecución
        let execution_time = start_time.elapsed().as_millis() as f64;
        match &result {
            Ok(signature) => {
                Logger::status_update(&format!("Arbitrage successful: {}", signature));
                // Record success in analytics
                self.record_transaction_analytics("arbitrage", true, estimated_profit - total_cost, total_cost).await;
                self.record_opportunity_analytics("arbitrage", true, true, estimated_profit, execution_time).await;
            },
            Err(e) => {
                Logger::error_occurred(&format!("Arbitrage failed: {}", e));
                self.record_transaction_analytics("arbitrage", false, -total_cost, total_cost).await;
                self.record_opportunity_analytics("arbitrage", true, false, -total_cost, execution_time).await;
            }
        };
        
        result
    }

    async fn execute_arbitrage_with_jito(&self, _target_tx_signature: &str, target_tx_details: Option<&Value>) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Preparing Jito bundle for arbitrage");
        
        let recent_blockhash_result = self.get_recent_blockhash().await;
        let recent_blockhash = match recent_blockhash_result {
            Ok(hash) => hash,
            Err(e) => {
                let error_msg = format!("Failed to get recent blockhash for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create the main transaction for the arbitrage (without tip)
        let main_transaction_data_result = self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await;
        let main_transaction_data = match main_transaction_data_result {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to create MEV strategy transaction for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create a tip transaction to one of Jito's tip accounts
        let tip_transaction_data_result = self.create_tip_transaction(&recent_blockhash)?;
        let tip_transaction_data = tip_transaction_data_result;
        
        // Combine both transactions for the bundle
        let transactions = vec![main_transaction_data.clone(), tip_transaction_data];
        
        // Usar Jito para enviar el bundle si está disponible
        match JitoClient::new() {
            Some(jito_client) => {
                Logger::status_update("Sending arbitrage bundle via Jito");
                match jito_client.send_bundle(&transactions).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Jito arbitrage bundle sent successfully: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send Jito bundle: {}, falling back to standard RPC", e);
                        Logger::error_occurred(&error_msg);
                        // Volver al RPC estándar si falla Jito
                        self.send_transaction(&main_transaction_data).await
                    }
                }
            }
            None => {
                Logger::status_update("Jito not configured, using standard RPC for arbitrage");
                match self.send_transaction(&main_transaction_data).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Arbitrage transaction sent via standard RPC: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send arbitrage transaction via standard RPC: {}", e);
                        Logger::error_occurred(&error_msg);
                        Err(e)
                    }
                }
            }
        }
    }    

    pub async fn execute_snipe(
        &self, 
        target_tx_signature: &str, 
        estimated_profit: f64,
        target_tx_details: Option<&Value>  // Include target transaction details for better strategy
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Attempting to snipe transaction: {}, with estimated profit: {:.6} SOL", target_tx_signature, estimated_profit));
        
        // Verificar si debemos continuar operando según los parámetros de riesgo
        if !self.should_continue_operation().await? {
            return Err("Operation halted due to risk management parameters".into());
        }
        
        let fees = self.calculate_transaction_fees().await?;
        let tip_amount = if self.use_jito { 0.001 } else { 0.0 }; // 0.001 SOL como propina para Jito
        
        // Additional safety check: prevent execution if estimated profit is non-positive
        if estimated_profit <= 0.0 {
            Logger::status_update(&format!(
                "Skipping snipe opportunity with no positive profit potential: estimated profit {:.6} SOL", 
                estimated_profit
            ));
            return Err("No positive profit potential".into());
        }
        
        // Run additional safety checks
        let safety_ok = self.additional_safety_checks(estimated_profit, fees, tip_amount).await?;
        if !safety_ok {
            Logger::status_update("Skipping snipe opportunity: failed additional safety checks");
            return Err("Failed additional safety checks".into());
        }
        
        let analysis = self.profit_calculator.calculate_profitability(estimated_profit, fees, tip_amount);
        
        if !analysis.is_profitable {
            Logger::status_update(&format!(
                "Skipping unprofitable snipe opportunity: net profit {:.6} SOL", 
                analysis.net_profit
            ));
            return Err("Snipe opportunity not profitable".into());
        }
        
        // Verificar que el potencial de pérdida no exceda el límite configurado
        if analysis.net_profit < -self.max_loss_per_bundle {
            Logger::status_update(&format!(
                "Skipping high-risk snipe opportunity: potential loss {:.6} SOL exceeds max allowed loss {:.6} SOL", 
                -analysis.net_profit, 
                self.max_loss_per_bundle
            ));
            return Err("Snipe opportunity exceeds maximum allowed loss".into());
        }
        
        Logger::status_update(&format!(
            "Profitable snipe opportunity: estimated profit {:.6} SOL, fees {:.6} SOL, net profit {:.6} SOL",
            analysis.estimated_profit,
            analysis.total_costs,
            analysis.net_profit
        ));
        
        // El método de ejecución es similar al frontrun pero conceptualmente diferente
        let result = if self.use_jito {
            Logger::status_update("Using Jito for snipe transaction priority");
            self.execute_snipe_with_jito(target_tx_signature, target_tx_details).await
        } else {
            Logger::status_update("Using standard RPC for snipe transaction");
            // Crear una transacción firmada basada en estrategia MEV
            let recent_blockhash_result = self.get_recent_blockhash().await;
            let recent_blockhash = match recent_blockhash_result {
                Ok(hash) => hash,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to get recent blockhash: {}", e));
                    return Err(e);
                }
            };
            
            let transaction_data = match self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await {
                Ok(data) => data,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to create MEV strategy transaction: {}", e));
                    return Err(e);
                }
            };
            
            // Enviar la transacción
            let signature_result = self.send_transaction(&transaction_data).await;
            match signature_result {
                Ok(signature) => {
                    Logger::status_update(&format!("Snipe transaction sent: {}", signature));
                    Ok(signature)
                },
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to send snipe transaction: {}", e));
                    Err(e)
                }
            }
        };
        
        // Registrar resultados de la ejecución
        match &result {
            Ok(signature) => {
                Logger::status_update(&format!("Snipe successful: {}", signature));
            },
            Err(e) => {
                Logger::error_occurred(&format!("Snipe failed: {}", e));
            }
        };
        
        result
    }

    async fn execute_snipe_with_jito(&self, _target_tx_signature: &str, target_tx_details: Option<&Value>) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Preparing Jito bundle for snipe");
        
        let recent_blockhash_result = self.get_recent_blockhash().await;
        let recent_blockhash = match recent_blockhash_result {
            Ok(hash) => hash,
            Err(e) => {
                let error_msg = format!("Failed to get recent blockhash for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create the main transaction for the snipe (without tip)
        let main_transaction_data_result = self.create_mev_strategy_transaction(&recent_blockhash, target_tx_details).await;
        let main_transaction_data = match main_transaction_data_result {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to create MEV strategy transaction for Jito bundle: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        // Create a tip transaction to one of Jito's tip accounts
        let tip_transaction_data_result = self.create_tip_transaction(&recent_blockhash)?;
        let tip_transaction_data = tip_transaction_data_result;
        
        // Combine both transactions for the bundle
        let transactions = vec![main_transaction_data.clone(), tip_transaction_data];
        
        // Usar Jito para enviar el bundle si está disponible
        match JitoClient::new() {
            Some(jito_client) => {
                Logger::status_update("Sending snipe bundle via Jito");
                match jito_client.send_bundle(&transactions).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Jito snipe bundle sent successfully: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send Jito bundle: {}, falling back to standard RPC", e);
                        Logger::error_occurred(&error_msg);
                        // Volver al RPC estándar si falla Jito
                        self.send_transaction(&main_transaction_data).await
                    }
                }
            }
            None => {
                Logger::status_update("Jito not configured, using standard RPC for snipe");
                match self.send_transaction(&main_transaction_data).await {
                    Ok(signature) => {
                        Logger::status_update(&format!("Snipe transaction sent via standard RPC: {}", signature));
                        Ok(signature)
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send snipe transaction via standard RPC: {}", e);
                        Logger::error_occurred(&error_msg);
                        Err(e)
                    }
                }
            }
        }
    }
}
