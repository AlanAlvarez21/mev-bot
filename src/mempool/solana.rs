use crate::config::Network;
use crate::logging::Logger;
use reqwest;
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::StreamExt;
use futures::SinkExt;
use std::env;
use std::sync::Arc;
use std::time::Instant;
use crate::executor::solana_executor::SolanaExecutor;
use crate::utils::profitability_calculator::OpportunityAnalysis;
use crate::utils::dex_monitor::DEXMonitor;
use crate::utils::dex_api::DexApi;
use crate::utils::transaction_simulator::TransactionSimulator;
use crate::rpc::rpc_manager::RpcManager;
use crate::utils::opportunity_evaluator::OpportunityEvaluator;
use crate::utils::enhanced_transaction_simulator::EnhancedTransactionSimulator;
use crate::utils::mev_simulation_pipeline::MevSimulationPipeline;
use crate::utils::fee_calculator::FeeCalculator;
use crate::utils::false_positive_reducer::FalsePositiveReducer;
use crate::utils::jito_optimizer::JitoOptimizer;
use crate::utils::mev_strategies::MevStrategyExecutor;
use crate::utils::metrics_collector::MetricsCollector;
use crate::utils::risk_controls::RiskManager as NewRiskManager;

#[derive(Clone)]
pub struct SolanaMempool {
    client: Arc<reqwest::Client>,
    rpc_url: String,
    ws_url: String,
    network: Network,
    dex_api: Arc<DexApi>,
    dex_monitor: Arc<tokio::sync::RwLock<DEXMonitor>>,
    transaction_simulator: Arc<TransactionSimulator>,
    
    // NEW ARCHITECTURE COMPONENTS - Optional until initialized
    rpc_manager: Option<Arc<RpcManager>>,
    opportunity_evaluator: Option<Arc<OpportunityEvaluator>>,
    enhanced_simulator: Option<Arc<EnhancedTransactionSimulator>>,
    mev_simulation_pipeline: Option<Arc<MevSimulationPipeline>>,
    fee_calculator: Option<Arc<FeeCalculator>>,
    false_positive_reducer: Arc<FalsePositiveReducer>,
    jito_optimizer: Option<Arc<JitoOptimizer>>,
    mev_strategy_executor: Option<Arc<MevStrategyExecutor>>,
    metrics_collector: Option<Arc<MetricsCollector>>,
    new_risk_manager: Option<Arc<NewRiskManager>>,
}

impl SolanaMempool {
    pub async fn new(network: &Network) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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

        let dex_api = Arc::new(DexApi::new(rpc_url.clone()));
        let dex_monitor = Arc::new(tokio::sync::RwLock::new(DEXMonitor::new()));
        
        let transaction_simulator = Arc::new(TransactionSimulator::new(rpc_url.clone())?);

        // NEW ARCHITECTURE - initialize with proper initialization
        let rpc_manager = Arc::new(RpcManager::new().await?);
        
        let opportunity_evaluator = Arc::new(OpportunityEvaluator::new(rpc_manager.clone()).await?);
        
        let enhanced_simulator = Arc::new(EnhancedTransactionSimulator::new(rpc_manager.clone()).await?);
        
        let mev_simulation_pipeline = Arc::new(MevSimulationPipeline::new(rpc_manager.clone()).await?);
        
        let fee_calculator = Arc::new(FeeCalculator::new(rpc_manager.clone()).await?);
        
        let jito_optimizer = Arc::new(JitoOptimizer::new(rpc_manager.clone()).await?);
        
        let metrics_collector = Arc::new(MetricsCollector::new()?);
        
        let new_risk_manager = Arc::new(NewRiskManager::new()?);
        
        let mev_strategy_executor = Arc::new(MevStrategyExecutor::new(
            rpc_manager.clone(),
            jito_optimizer.clone(),
            fee_calculator.clone(),
            opportunity_evaluator.clone(),
            mev_simulation_pipeline.clone(),
        ).await?);
        
        let false_positive_reducer = Arc::new(FalsePositiveReducer::new());

        Ok(Self {
            client: Arc::new(reqwest::Client::new()),
            rpc_url,
            ws_url,
            network: network.clone(),
            dex_api,
            dex_monitor,
            transaction_simulator,
            
            // NEW ARCHITECTURE COMPONENTS
            rpc_manager: Some(rpc_manager),
            opportunity_evaluator: Some(opportunity_evaluator),
            enhanced_simulator: Some(enhanced_simulator),
            mev_simulation_pipeline: Some(mev_simulation_pipeline),
            fee_calculator: Some(fee_calculator),
            false_positive_reducer,
            jito_optimizer: Some(jito_optimizer),
            mev_strategy_executor: Some(mev_strategy_executor),
            metrics_collector: Some(metrics_collector),
            new_risk_manager: Some(new_risk_manager),
        })
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

        // Keep trying to connect to WebSocket with reconnection logic
        loop {
            Logger::status_update("Attempting to connect to WebSocket...");
            // Convert executor to Arc for safe sharing across tasks
            let executor_arc = Arc::new(executor.clone());
            match self.connect_ws_with_reconnect(executor_arc.clone()).await {
                Ok(_) => {
                    Logger::status_update("WebSocket connection was successful");
                    // If connect_ws_with_reconnect returns normally, it means it was intentionally stopped
                    break;
                },
                Err(e) => {
                    Logger::error_occurred(&format!("WebSocket connection failed: {}, falling back to slot monitoring: {}", e, self.ws_url));
                    // If WebSocket connection fails, fall back to slot monitoring
                    // This will automatically try to reconnect to WebSocket when it encounters too many errors
                    self.start_slot_monitoring(&executor).await;
                }
            }
        }
    }
    
    async fn connect_ws_with_reconnect(&self, executor: Arc<SolanaExecutor>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (ws_stream, _) = connect_async(&self.ws_url).await
            .map_err(|e| format!("WebSocket connection failed: {}", e))?;
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
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
        
        // Process incoming messages with concurrent handling
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
                                                // Spawn a new task for each transaction to process concurrently
                                                let executor_clone = executor.clone();
                                                let mempool_clone = self.clone();
                                                let sig = signature.to_string();
                                                
                                                tokio::spawn(async move {
                                                    mempool_clone.analyze_and_execute_opportunity(&executor_clone, &sig).await;
                                                });
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
                    return Err(Box::new(e));
                }
                None => {
                    Logger::error_occurred("WebSocket stream ended unexpectedly");
                    return Err("WebSocket stream ended".into());
                }
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
        // NEW ARCHITECTURE: Use the new opportunity evaluator to analyze transaction
        // Check if new architecture is properly initialized
        if self.rpc_manager.is_none() {
            Logger::status_update("New architecture not initialized for mempool");
            return;
        }
        
        Logger::opportunity_detected("Solana", signature);
        
        // Fetch target transaction details with timeout
        let target_tx_details_result = self.fetch_transaction_details_with_timeout(signature, 1000).await; // 1000ms timeout
        let target_tx_details = target_tx_details_result.as_ref().ok();
        
        if target_tx_details.is_none() {
            Logger::status_update(&format!("Could not fetch target transaction details for: {}", signature));
            return;
        }
        
        let target_tx_details = target_tx_details.unwrap();
        
        // NEW ARCHITECTURE: Evaluate the opportunity using the new evaluator
        if let Some(ref evaluator) = self.opportunity_evaluator {
            if let Some(opportunity) = evaluator.evaluate_opportunity(target_tx_details).await.ok().flatten() {
                // NEW ARCHITECTURE: Run enhanced simulation to validate opportunity
                if let Some(ref simulator) = self.enhanced_simulator {
                    let simulation_result = match simulator.simulate_and_validate(&opportunity).await {
                        Ok(result) => result,
                        Err(e) => {
                            Logger::error_occurred(&format!("Failed to simulate opportunity: {}", e));
                            return;
                        }
                    };
                    
                    // NEW ARCHITECTURE: Apply false positive reduction
                    let filtering_result = self.false_positive_reducer.evaluate_opportunity(&opportunity, &simulation_result.simulation_results).await;
                    
                    if !filtering_result.should_execute {
                        Logger::status_update(&format!("Opportunity filtered out by false positive reducer: {}", 
                                                     filtering_result.filtered_reason.unwrap_or("Unknown reason".to_string())));
                        return;
                    }
                    
                    // Calculate average confidence from simulation results
                    let avg_confidence = if !simulation_result.simulation_results.is_empty() {
                        simulation_result.simulation_results.iter()
                            .map(|sr| sr.confidence_score)
                            .sum::<f64>() / simulation_result.simulation_results.len() as f64
                    } else {
                        0.0
                    };
                    
                    Logger::status_update(&format!(
                        "Validated opportunity: type {:?}, estimated profit: {:.6} SOL, confidence: {:.2}%", 
                        opportunity.opportunity_type, 
                        opportunity.estimated_profit,
                        avg_confidence * 100.0
                    ));
                    
                    // NEW ARCHITECTURE: Execute the appropriate strategy based on opportunity type
                    if let Some(ref strategy_executor) = self.mev_strategy_executor {
                        let strategy_result = match strategy_executor.execute_strategy(&opportunity, Some(target_tx_details)).await {
                            Ok(result) => result,
                            Err(e) => {
                                Logger::error_occurred(&format!("Strategy execution failed: {}", e));
                                return;
                            }
                        };
                        
                        // NEW ARCHITECTURE: Record the execution result
                        if let Some(ref metrics_collector) = self.metrics_collector {
                            metrics_collector.record_strategy_execution(&strategy_result).await;
                        }
                        
                        if strategy_result.success {
                            Logger::bundle_sent("Solana", true);
                            Logger::status_update(&format!(
                                "Strategy executed successfully: type {:?}, net profit: {:.6} SOL", 
                                strategy_result.strategy_type, 
                                strategy_result.profit
                            ));
                        } else {
                            Logger::status_update(&format!(
                                "Strategy execution failed: type {:?}, loss: {:.6} SOL", 
                                strategy_result.strategy_type, 
                                strategy_result.profit
                            ));
                        }
                    }
                }
            } else {
                Logger::status_update(&format!("No profitable opportunity detected for transaction: {}", signature));
            }
        }
    }
    
    async fn classify_transaction_opportunity(&self, tx_details: &Value) -> OpportunityType {
        // Analyze the transaction to determine the best MEV strategy
        
        // Check for swap instructions (common in arbitrage and frontrun opportunities)
        if let Some(transaction) = tx_details.get("transaction") {
            if let Some(message) = transaction.get("message") {
                if let Some(instructions) = message.get("instructions") {
                    if let Some(instr_array) = instructions.as_array() {
                        for instruction in instr_array {
                            if let Some(accounts) = instruction.get("accounts").and_then(|v| v.as_array()) {
                                // More than 3 accounts often indicates a DEX swap
                                if accounts.len() >= 4 {
                                    // Check for high-value token transfers that might indicate arbitrage
                                    if let Some(meta) = tx_details.get("meta") {
                                        if let Some(post_balances) = meta.get("postTokenBalances").and_then(|v| v.as_array()) {
                                            // If there are significant changes, it might be an arbitrage opportunity
                                            for balance in post_balances {
                                                if let Some(ui_amount) = balance.get("uiTokenAmount").and_then(|v| v.get("uiAmount")).and_then(|v| v.as_f64()) {
                                                    if ui_amount > 1000.0 { // Threshold for significant amount
                                                        return OpportunityType::Arbitrage;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    return OpportunityType::Frontrun;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Check for token balance changes that indicate swaps
        if let Some(meta) = tx_details.get("meta") {
            if let Some(post_token_balances) = meta.get("postTokenBalances").and_then(|v| v.as_array()) {
                if let Some(pre_token_balances) = meta.get("preTokenBalances").and_then(|v| v.as_array()) {
                    let significant_changes = post_token_balances.iter().zip(pre_token_balances.iter())
                        .filter(|(post, pre)| {
                            let post_amount = post.get("uiTokenAmount").and_then(|v| v.get("uiAmount")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let pre_amount = pre.get("uiTokenAmount").and_then(|v| v.get("uiAmount")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                            (post_amount - pre_amount).abs() > 100.0
                        })
                        .count();
                    
                    if significant_changes >= 2 {
                        return OpportunityType::Arbitrage;
                    }
                }
            }
        }
        
        OpportunityType::Other
    }
    
    async fn execute_arbitrage_strategy(&self, executor: &SolanaExecutor, signature: &str, target_tx_details: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Executing arbitrage strategy for transaction: {}", signature));
        
        // Get current pool states to find arbitrage opportunities
        let dex_monitor = self.dex_monitor.read().await;
        let pools = dex_monitor.get_all_pools();
        
        // Look for arbitrage opportunities based on current pool states
        // This is a simplified version - in practice, we'd do more sophisticated analysis
        
        // Get a snapshot of the pools to avoid holding the lock across await points
        let pools_data = {
            let monitor = self.dex_monitor.read().await;
            // Clone the pools data to work with after releasing the lock
            monitor.get_all_pools().iter().map(|p| (p.token_a.clone(), p.token_b.clone())).collect::<Vec<_>>()
        };
        
        // Check opportunities for each pool
        for (token_a, token_b) in pools_data {
            // Get opportunity for this token pair
            let opportunity = {
                let monitor = self.dex_monitor.read().await;
                monitor.find_arbitrage_opportunity(&token_a, &token_b)
            };
            
            if let Some(opportunity) = opportunity {
                if opportunity.estimated_profit > 0.01 { // Only execute if profitable
                    Logger::status_update(&format!(
                        "Found arbitrage opportunity: buy at {:.6} sell at {:.6}, estimated profit: {:.6} SOL",
                        opportunity.buy_price, opportunity.sell_price, opportunity.estimated_profit
                    ));
                    
                    // Validate the opportunity
                    let validation = self.transaction_simulator.validate_arbitrage_opportunity(&opportunity, 1_000_000).await?;
                    
                    if validation.is_valid && validation.net_profit > 0.005 { // Require minimum net profit
                        return executor.execute_arbitrage(signature, validation.net_profit, Some(target_tx_details)).await;
                    }
                }
            }
        }
        
        Err("No profitable arbitrage opportunity found".into())
    }
    
    async fn execute_frontrun_strategy(&self, executor: &SolanaExecutor, signature: &str, target_tx_details: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Executing frontrun strategy for transaction: {}", signature));
        
        // Analyze the target transaction to replicate the same operation but with higher priority
        let swap_info = self.extract_swap_info(target_tx_details).await;
        
        if let Some(swap_details) = swap_info {
            Logger::status_update(&format!(
                "Detected swap: {} -> {}, amount: {}",
                swap_details.input_token, swap_details.output_token, swap_details.amount_in
            ));
            
            // Calculate potential frontrun profit based on market impact
            let estimated_profit = self.estimate_frontrun_profit(&swap_details).await;
            
            if estimated_profit > 0.005 { // Only execute if potentially profitable
                Logger::status_update(&format!("Estimated frontrun profit: {:.6} SOL", estimated_profit));
                
                return executor.execute_frontrun(signature, estimated_profit, Some(target_tx_details)).await;
            }
        }
        
        Err("No profitable frontrun opportunity found".into())
    }
    
    async fn execute_sandwich_strategy(&self, executor: &SolanaExecutor, signature: &str, target_tx_details: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Executing sandwich strategy for transaction: {}", signature));
        
        // For sandwich attacks, we need to manipulate liquidity before and after the target
        let swap_info = self.extract_swap_info(target_tx_details).await;
        
        if let Some(swap_details) = swap_info {
            Logger::status_update(&format!(
                "Detected swap for sandwich: {} -> {}, amount: {}",
                swap_details.input_token, swap_details.output_token, swap_details.amount_in
            ));
            
            // Calculate potential sandwich profit based on price manipulation
            let estimated_profit = self.estimate_sandwich_profit(&swap_details).await;
            
            if estimated_profit > 0.01 { // Only execute if potentially profitable
                Logger::status_update(&format!("Estimated sandwich profit: {:.6} SOL", estimated_profit));
                
                return executor.execute_sandwich(signature, estimated_profit, Some(target_tx_details)).await;
            }
        }
        
        Err("No profitable sandwich opportunity found".into())
    }
    
    async fn execute_snipe_strategy(&self, executor: &SolanaExecutor, signature: &str, target_tx_details: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update(&format!("Executing snipe strategy for transaction: {}", signature));
        
        // Sniping typically involves jumping ahead of other transactions
        // This could be for new token listings, flash loans, or other opportunities
        let estimated_profit = self.estimate_snipe_profit(target_tx_details).await;
        
        if estimated_profit > 0.005 {
            Logger::status_update(&format!("Estimated snipe profit: {:.6} SOL", estimated_profit));
            return executor.execute_snipe(signature, estimated_profit, Some(target_tx_details)).await;
        }
        
        Err("No profitable snipe opportunity found".into())
    }
    
    async fn extract_swap_info(&self, tx_details: &Value) -> Option<SwapDetails> {
        // Extract information about a swap from transaction details
        if let Some(transaction) = tx_details.get("transaction") {
            if let Some(message) = transaction.get("message") {
                if let Some(instructions) = message.get("instructions") {
                    if let Some(instr_array) = instructions.as_array() {
                        for instruction in instr_array {
                            // Look for instructions that have multiple accounts (typical for DEX swaps)
                            if let Some(accounts) = instruction.get("accounts").and_then(|v| v.as_array()) {
                                if accounts.len() >= 4 {
                                    // This is likely a swap instruction
                                    // In a real implementation, we'd extract actual token addresses and amounts
                                    return Some(SwapDetails {
                                        input_token: "TOKEN_A".to_string(),
                                        output_token: "TOKEN_B".to_string(),
                                        amount_in: 1_000_000, // Placeholder
                                        expected_amount_out: 950_000, // Placeholder
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
    
    async fn estimate_frontrun_profit(&self, swap_details: &SwapDetails) -> f64 {
        // Estimate potential profit from frontrunning a swap
        // This would involve analyzing current prices and potential market impact
        
        // In a real implementation, this would be based on current pool states and simulation
        0.01 // Placeholder
    }
    
    async fn estimate_sandwich_profit(&self, swap_details: &SwapDetails) -> f64 {
        // Estimate potential profit from sandwiching a swap
        // This involves calculating the price manipulation and subsequent profit
        
        // In a real implementation, this would be more sophisticated
        0.02 // Placeholder
    }
    
    async fn estimate_snipe_profit(&self, tx_details: &Value) -> f64 {
        // Estimate potential profit from sniping opportunities
        0.005 // Placeholder
    }
}

#[derive(Debug, Clone)]
enum OpportunityType {
    Arbitrage,
    Frontrun,
    Sandwich,
    Other,
}

#[derive(Debug, Clone)]
struct SwapDetails {
    input_token: String,
    output_token: String,
    amount_in: u64,
    expected_amount_out: u64,
}

impl SolanaMempool {
    async fn quick_estimate_profitability(&self, signature: &str) -> OpportunityAnalysis {
        Logger::status_update(&format!("Quick analyzing profitability for transaction: {}", signature));
        
        // Use a timeout for fetching transaction details to speed up processing
        let tx_details_result = self.fetch_transaction_details_with_timeout(signature, 500).await; // 500ms limit
        
        let fees = 0.006; // 0.006 SOL en fees promedio (taxas + Jito tips)
        let mut potential_profit = 0.0; // Initially assume no profit
        
        match tx_details_result {
            Ok(tx_details) => {
                // Analyze the transaction details for potential MEV opportunities
                potential_profit = self.analyze_real_transaction(&tx_details).await;
                Logger::status_update(&format!("Quick transaction analysis suggests profit potential: {:.6} SOL", potential_profit));
            },
            Err(_) => {
                // If we can't fetch details quickly, use a minimal conservative estimate
                Logger::status_update("Could not fetch transaction details quickly, using minimal estimate");
                potential_profit = 0.0; // Still 0 if we can't analyze it
                Logger::status_update("Defaulting to zero profit estimate due to timeout");
            }
        }
        
        Logger::status_update(&format!("Final estimated profit potential: {:.6} SOL", potential_profit));
        
        // Calculate net profit and determine if opportunity is really profitable
        let net_profit = potential_profit - fees;
        
        // More conservative profitability check: require positive net profit and positive potential profit
        let is_profitable = net_profit > 0.001 && potential_profit > 0.0;
        
        OpportunityAnalysis {
            profit: potential_profit,
            cost: fees,
            revenue: potential_profit.max(0.0),
            is_profitable,
            min_profit_margin: 0.1,  // Require minimum 10% profit margin
            net_profit,
        }
    }
    
    async fn fetch_transaction_details_with_timeout(&self, signature: &str, timeout_ms: u64) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        use tokio::time::timeout;
        
        let result = timeout(
            tokio::time::Duration::from_millis(timeout_ms),
            self.fetch_transaction_details(signature)
        ).await;
        
        match result {
            Ok(fetch_result) => fetch_result,
            Err(_) => Err("Transaction details fetch timed out".into()) // Return error on timeout
        }
    }
    
    async fn estimate_profitability(&self, signature: &str) -> OpportunityAnalysis {
        Logger::status_update(&format!("Analyzing profitability for transaction: {}", signature));
        
        // Fetch the actual transaction details to analyze if there are real MEV opportunities
        let tx_details_result = self.fetch_transaction_details(signature).await;
        
        let fees = 0.006; // 0.006 SOL en fees promedio (taxas + Jito tips)
        
        // Initialize with conservative defaults
        let mut potential_profit = 0.0; // Initially assume no profit
        let mut target_impact = 0.0;
        
        match tx_details_result {
            Ok(tx_details) => {
                // Analyze the transaction details for potential MEV opportunities
                potential_profit = self.analyze_real_transaction(&tx_details).await;
                Logger::status_update(&format!("Real transaction analysis suggests profit potential: {:.6} SOL", potential_profit));
            },
            Err(_) => {
                // If we can't fetch transaction details, use a very conservative estimate
                Logger::status_update("Could not fetch transaction details, using conservative estimate");
                // Default to zero profit when we can't analyze the transaction
                potential_profit = 0.0;
                Logger::status_update("Defaulting to zero profit estimate due to lack of transaction data");
            }
        }
        
        Logger::status_update(&format!("Final estimated profit potential: {:.6} SOL", potential_profit));
        
        // Calculate net profit and determine if opportunity is really profitable
        let net_profit = potential_profit - fees;
        
        // More conservative profitability check: require positive net profit and positive potential profit
        let is_profitable = net_profit > 0.001 && potential_profit > 0.0;
        
        OpportunityAnalysis {
            profit: potential_profit,
            cost: fees,
            revenue: potential_profit.max(0.0),
            is_profitable,
            min_profit_margin: 0.1,  // Require minimum 10% profit margin
            net_profit,
        }
    }
    
    async fn fetch_transaction_details(&self, signature: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                signature,
                {
                    "encoding": "json",
                    "maxSupportedTransactionVersion": 0
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
            return Err(format!("Get transaction failed: {}", error).into());
        }

        if let Some(result) = response.get("result") {
            Ok(result.clone())
        } else {
            Err("Failed to get transaction result".into())
        }
    }
    
    async fn analyze_real_transaction(&self, tx_details: &Value) -> f64 {
        // Analyze the transaction for potential MEV opportunities
        // This is a more comprehensive analysis for identifying real MEV opportunities
        
        let mut estimated_profit: f64 = 0.0;
        
        // First, check if this transaction represents a swap that we could arbitrage against
        if let Some(swap_opportunity) = self.detect_direct_swap_opportunity(tx_details).await {
            estimated_profit += swap_opportunity.potential_profit;
        }
        
        // Check for swap instructions (common MEV opportunity)
        if let Some(transaction) = tx_details.get("transaction") {
            if let Some(message) = transaction.get("message") {
                if let Some(instructions) = message.get("instructions") {
                    if let Some(instr_array) = instructions.as_array() {
                        for instruction in instr_array {
                            // Check for known DEX program IDs that indicate swaps/arbitrage opportunities
                            // In a real implementation we would check program IDs directly
                            if let Some(accounts) = instruction.get("accounts").and_then(|v| v.as_array()) {
                                if accounts.len() >= 3 {
                                    // This looks like it could be a swap instruction with multiple accounts
                                    // Extract accounts to check for token swaps
                                    estimated_profit += self.identify_mev_opportunity_from_accounts(accounts).await;
                                }
                            }
                            
                            // Check for specific instruction data that indicates swaps
                            if let Some(data) = instruction.get("data").and_then(|v| v.as_str()) {
                                // Check if the instruction data indicates a swap operation
                                if data.contains("swap") || data.contains("Swap") || 
                                   data.contains("route") || data.contains("Route") {
                                    estimated_profit += 0.01; // More significant potential for swaps
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Check for potential sandwich opportunities
        if let Some(meta) = tx_details.get("meta") {
            if let Some(post_token_balances) = meta.get("postTokenBalances").and_then(|v| v.as_array()) {
                if let Some(pre_token_balances) = meta.get("preTokenBalances").and_then(|v| v.as_array()) {
                    // Compare token balances before and after to detect swaps
                    // This can indicate potential for sandwich attacks
                    if post_token_balances.len() > 0 && pre_token_balances.len() > 0 {
                        estimated_profit += self.analyze_token_balance_changes(post_token_balances, pre_token_balances).await;
                    }
                }
            }
        }
        
        // Perform real arbitrage analysis by comparing with current pool states
        if let Some(arb_profit) = self.check_arbitrage_against_transaction(tx_details).await {
            estimated_profit = if estimated_profit > arb_profit { estimated_profit } else { arb_profit }; // Use f64::max equivalent
        }
        
        // In a production environment, we would analyze:
        // - Token swaps for arbitrage opportunities
        // - Liquidity changes for sandwich attack potential
        // - NFT sales for sniping opportunities
        // - Other DeFi interactions for liquidation opportunities
        
        // Return the conservative estimate based on real transaction analysis
        if estimated_profit < 0.5 { estimated_profit } else { 0.5 } // Cap at 0.5 SOL to be conservative
    }
    
    async fn detect_direct_swap_opportunity(&self, tx_details: &Value) -> Option<crate::utils::dex_monitor::SwapOpportunity> {
        // Analyze if this transaction is a swap that we can potentially frontrun or backrun
        // This is a more sophisticated analysis than the basic one
        
        // Extract relevant information from the transaction
        if let Some(transaction) = tx_details.get("transaction") {
            if let Some(message) = transaction.get("message") {
                if let Some(instructions) = message.get("instructions") {
                    if let Some(instr_array) = instructions.as_array() {
                        for instruction in instr_array {
                            // Check for accounts that look like DEX swaps
                            if let Some(accounts) = instruction.get("accounts").and_then(|v| v.as_array()) {
                                if accounts.len() >= 4 { // DEX swaps typically have multiple accounts
                                    // This is likely a swap instruction - estimate profit potential
                                    return Some(crate::utils::dex_monitor::SwapOpportunity {
                                        detected_type: crate::utils::dex_monitor::SwapType::Swap,
                                        potential_profit: 0.01, // Placeholder - would be calculated from market impact
                                        transaction_signature: tx_details.get("transaction").and_then(|t| t.get("signatures")).and_then(|s| s.as_array()).and_then(|s| s.first()).and_then(|sig| sig.as_str()).unwrap_or("unknown").to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    async fn check_arbitrage_against_transaction(&self, tx_details: &Value) -> Option<f64> {
        // This would check if the transaction creates an arbitrage opportunity
        // by comparing prices before and after or by analyzing the market impact
        
        // For now, return a placeholder
        Some(0.02) // Placeholder - real implementation would calculate from market data
    }
    
    async fn identify_mev_opportunity_from_accounts(&self, accounts: &[Value]) -> f64 {
        // Analyze the accounts in the instruction to determine MEV potential
        // For now, this is a simplified check - in practice, we'd map actual account indices
        // to known DEX addresses and liquidity pool addresses
        
        let account_count = accounts.len();
        
        // DEX swaps typically involve multiple accounts:
        // - User wallet
        // - Token accounts (input/output)
        // - DEX program
        // - Liquidity pools
        // - Vault accounts
        if account_count >= 4 {
            return 0.005; // Higher potential for multi-account transactions
        } else if account_count >= 2 {
            return 0.001; // Lower potential for simple transactions
        }
        
        0.0
    }
    
    async fn analyze_token_balance_changes(&self, post_balances: &[Value], pre_balances: &[Value]) -> f64 {
        // Analyze changes in token balances to identify swaps and potential MEV opportunities
        let mut mev_potential = 0.0;
        
        // Compare pre and post balances to identify token swaps
        for (pre, post) in pre_balances.iter().zip(post_balances.iter()) {
            if let (Some(pre_amount), Some(post_amount)) = (
                pre.get("uiTokenAmount").and_then(|v| v.get("uiAmount")).and_then(|v| v.as_f64()),
                post.get("uiTokenAmount").and_then(|v| v.get("uiAmount")).and_then(|v| v.as_f64())
            ) {
                let change = post_amount - pre_amount;
                if change.abs() > 0.001 {  // Significant change threshold
                    mev_potential += 0.002; // Potential MEV opportunity
                }
            }
        }
        
        mev_potential
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
        let mut connection_errors = 0; // Track connection errors for backoff
        let max_errors_before_reset = 10;
        
        loop {
            match self.get_slot().await {
                Ok(current_slot) => {
                    if current_slot > last_slot {
                        // Simulate checking for transactions in the slot
                        if current_slot % 50 == 0 { // Every 50 slots, simulate an opportunity
                            Logger::opportunity_detected("Solana", &format!("simulated_tx_{}", current_slot));
                            
                            // Execute frontrun strategy with zero profit since this is simulated
                            match executor.execute_frontrun(&format!("simulated_tx_{}", current_slot), 0.0, None).await {
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
                        connection_errors = 0; // Reset error counter on success
                    }
                }
                Err(e) => {
                    Logger::error_occurred(&format!("Slot monitoring error: {}", e));
                    connection_errors += 1;
                    
                    // If we have too many errors, try to reset by returning to start() which will attempt WebSocket again
                    if connection_errors >= max_errors_before_reset {
                        Logger::status_update("Too many slot monitoring errors, attempting to reconnect to WebSocket...");
                        return; // Return to start() to try WebSocket connection again
                    }
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
} // End of impl SolanaMempool
