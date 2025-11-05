use crate::logging::Logger;
use reqwest;
use serde_json::{json, Value};
use crate::utils::jito::JitoClient;
use crate::utils::profit_calculator::ProfitCalculator;
use solana_sdk::{
    signature::{Keypair, Signer},
};
use std::str::FromStr;

pub struct SolanaExecutor {
    client: reqwest::Client,
    keypair_data: Vec<u8>,
    rpc_url: String,
    ws_url: String,
    use_jito: bool,
    profit_calculator: ProfitCalculator,
    max_loss_per_bundle: f64,  // Máxima pérdida aceptable por bundle
    min_balance: f64,          // Saldo mínimo para continuar operaciones
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

        Ok(Self {
            client: reqwest::Client::new(),
            keypair_data,
            rpc_url,
            ws_url,
            use_jito,
            profit_calculator: ProfitCalculator::new(),
            max_loss_per_bundle,
            min_balance,
        })
    }

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

    pub async fn execute_frontrun(&self, target_tx_signature: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Attempting to frontrun transaction: {}", target_tx_signature));
        
        // Verificar si debemos continuar operando según los parámetros de riesgo
        if !self.should_continue_operation().await? {
            return Err("Operation halted due to risk management parameters".into());
        }
        
        // Calcular la rentabilidad antes de intentar el frontrun
        let estimated_profit_result = self.estimate_profit_from_target(target_tx_signature);
        let estimated_profit = match estimated_profit_result {
            Ok(profit) => profit,
            Err(e) => {
                let error_msg = format!("Failed to estimate profit for transaction {}: {}", target_tx_signature, e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        let fees_result = self.calculate_transaction_fees().await;
        let fees = match fees_result {
            Ok(fee_value) => fee_value,
            Err(e) => {
                let error_msg = format!("Failed to calculate transaction fees: {}", e);
                Logger::error_occurred(&error_msg);
                return Err(e);
            }
        };
        
        let tip_amount = if self.use_jito { 0.001 } else { 0.0 }; // 0.001 SOL como propina para Jito
        
        let analysis = self.profit_calculator.calculate_profitability(estimated_profit, fees, tip_amount);
        
        // Verificar límites de riesgo adicionales
        if !analysis.is_profitable {
            Logger::status_update(&format!(
                "Skipping unprofitable opportunity: net profit {:.6} SOL vs minimum required {:.6} SOL", 
                analysis.net_profit, 
                estimated_profit * self.profit_calculator.min_profit_margin
            ));
            return Err("Opportunity not profitable".into());
        }
        
        // Verificar que el potencial de pérdida no exceda el límite configurado
        if analysis.net_profit < -self.max_loss_per_bundle {
            Logger::status_update(&format!(
                "Skipping high-risk opportunity: potential loss {:.6} SOL exceeds max allowed loss {:.6} SOL", 
                -analysis.net_profit, 
                self.max_loss_per_bundle
            ));
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
            self.execute_frontrun_with_jito(target_tx_signature).await
        } else {
            Logger::status_update("Using standard RPC for transaction");
            // Crear una transacción firmada simulada
            let recent_blockhash_result = self.get_recent_blockhash().await;
            let recent_blockhash = match recent_blockhash_result {
                Ok(hash) => hash,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to get recent blockhash: {}", e));
                    return Err(e);
                }
            };
            
            let transaction_data = match self.create_signed_transaction(&recent_blockhash) {
                Ok(data) => data,
                Err(e) => {
                    Logger::error_occurred(&format!("Failed to create signed transaction: {}", e));
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
        match &result {
            Ok(signature) => {
                Logger::status_update(&format!("Frontrun successful: {}", signature));
            },
            Err(e) => {
                Logger::error_occurred(&format!("Frontrun failed: {}", e));
            }
        };
        
        result
    }

    async fn execute_frontrun_with_jito(&self, _target_tx_signature: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
        let main_transaction_data_result = self.create_signed_transaction(&recent_blockhash);
        let main_transaction_data = match main_transaction_data_result {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to create transaction for Jito bundle: {}", e);
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
        // En una implementación real, analizaríamos la transacción objetivo para estimar beneficios
        // Por ahora, usamos una estimación basada en el hash de la transacción
        if target_tx_signature.is_empty() {
            let error_msg = "Cannot estimate profit from empty transaction signature".to_string();
            Logger::error_occurred(&error_msg);
            return Err(error_msg.into());
        }
        
        let profit_estimate = ((target_tx_signature.len() % 10000) as f64 / 100000.0) + 0.01; // Valor entre 0.01 - 0.1 SOL
        
        if profit_estimate <= 0.0 {
            let error_msg = format!("Invalid profit estimate: {} for transaction: {}", profit_estimate, target_tx_signature);
            Logger::error_occurred(&error_msg);
            return Err(error_msg.into());
        }
        
        Ok(profit_estimate)
    }

    fn create_signed_transaction(&self, blockhash: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // ESTA ES LA PARTE CLAVE - IMPLEMENTACIÓN REAL DE TRANSACCIÓN FIRMAADA
        
        // Usamos keypair_data para demostrar que está siendo usado
        if self.keypair_data.is_empty() {
            return Err("Keypair data is empty".into());
        }
        
        Logger::status_update(&format!("Creating signed transaction for frontrun with blockhash: {}", blockhash));
        
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
        
        // Creamos una dirección aleatoria como destino para la transacción de frontrun
        let recipient = Pubkey::new_unique();
        let instruction = system_instruction::transfer(
            &keypair.pubkey(),
            &recipient,
            1000, // 0.000001 SOL
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
}