use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use crate::logging::Logger;
use crate::rpc::rpc_manager::{RpcManager, RpcEndpointType};

#[derive(Debug, Clone)]
pub struct JitoHealthStatus {
    pub is_healthy: bool,
    pub latency_ms: f64,
    pub success_rate: f64,
    pub last_check: Instant,
    pub available_tip_accounts: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TipOptimizationResult {
    pub optimal_tip: f64,
    pub recommended_tip_account: String,
    pub confidence: f64,
    pub expected_success_rate: f64,
}

#[derive(Debug, Clone)]
pub struct BundleTimingStrategy {
    pub delay_micros: u64,
    pub retry_count: u8,
    pub propagation_wait_ms: u64,
}

pub struct JitoOptimizer {
    rpc_manager: Arc<RpcManager>,
    health_status: Arc<RwLock<JitoHealthStatus>>,
    tip_accounts: Vec<Pubkey>,
    current_tip: f64,
    health_check_interval: Duration,
    tip_adjustment_history: Arc<RwLock<Vec<(Instant, f64, bool)>>>, // (time, tip_amount, success)
}

impl JitoOptimizer {
    pub async fn new(rpc_manager: Arc<RpcManager>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Load tip accounts from environment variable
        let tip_accounts_str = std::env::var("JITO_TIP_ACCOUNT")
            .map_err(|_| "JITO_TIP_ACCOUNT environment variable not set")?;
        
        let tip_accounts: Vec<Pubkey> = tip_accounts_str
            .split(',')
            .filter_map(|addr| Pubkey::from_str(addr.trim()).ok())
            .collect();
        
        if tip_accounts.is_empty() {
            return Err("No valid Jito tip accounts provided in JITO_TIP_ACCOUNT".into());
        }
        
        let optimizer = Self {
            rpc_manager: Arc::new(rpc_manager),
            health_status: Arc::new(RwLock::new(JitoHealthStatus {
                is_healthy: false,
                latency_ms: 0.0,
                success_rate: 0.0,
                last_check: Instant::now(),
                available_tip_accounts: tip_accounts.iter().map(|pk| pk.to_string()).collect(),
            })),
            tip_accounts,
            current_tip: 0.001, // Start with 0.001 SOL default tip
            health_check_interval: Duration::from_secs(15), // Check every 15 seconds
            tip_adjustment_history: Arc::new(RwLock::new(Vec::new())),
        };
        
        // Start health checks
        optimizer.start_health_checks().await;
        
        Ok(optimizer)
    }
    
    pub async fn check_jito_health(&self) -> Result<JitoHealthStatus, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        
        // Make a test request to Jito RPC
        let test_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": []
        });
        
        let success = match self.rpc_manager.make_request(RpcEndpointType::Jito, test_request).await {
            Ok(response) => {
                response["result"]["value"]["blockhash"].as_str().is_some()
            },
            Err(_) => false,
        };
        
        let latency = start_time.elapsed().as_millis() as f64;
        
        let health_status = JitoHealthStatus {
            is_healthy: success && latency < 1500.0, // Healthy if under 1.5s latency and successful
            latency_ms: latency,
            success_rate: if success { 1.0 } else { 0.0 },
            last_check: Instant::now(),
            available_tip_accounts: self.get_available_tip_accounts().await,
        };
        
        // Update internal health status
        {
            let mut status = self.health_status.write().await;
            *status = health_status.clone();
        }
        
        Ok(health_status)
    }
    
    async fn start_health_checks(&self) {
        let self_clone = self.clone_for_spawn();
        
        tokio::spawn(async move {
            loop {
                match self_clone.check_jito_health().await {
                    Ok(health) => {
                        Logger::status_update(&format!(
                            "Jito health check: healthy={}, latency={}ms, success_rate={:.1}%", 
                            health.is_healthy, 
                            health.latency_ms as u64, 
                            health.success_rate * 100.0
                        ));
                    },
                    Err(e) => {
                        Logger::error_occurred(&format!("Jito health check failed: {}", e));
                    }
                }
                
                tokio::time::sleep(self_clone.health_check_interval).await;
            }
        });
    }
    
    pub async fn calculate_optimal_tip(
        &self,
        opportunity_value: f64,
        network_congestion: f64, // 0.0 to 1.0 scale
        competition_level: f64   // 0.0 to 1.0 scale
    ) -> Result<TipOptimizationResult, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Calculating optimal Jito tip based on opportunity value and network conditions");
        
        // Calculate base tip based on opportunity value
        let base_tip = self.calculate_base_tip(opportunity_value).await;
        
        // Adjust for network congestion
        let congestion_adjustment = 1.0 + (network_congestion * 0.5); // Up to 50% increase for high congestion
        
        // Adjust for competition level
        let competition_adjustment = 1.0 + (competition_level * 0.8); // Up to 80% increase for high competition
        
        // Calculate final tip
        let final_tip = base_tip * congestion_adjustment * competition_adjustment;
        
        // Ensure tip is within reasonable bounds
        let optimal_tip = final_tip.clamp(0.0001, 0.01); // Between 0.0001 and 0.01 SOL
        
        // Select the best tip account based on load balancing
        let recommended_tip_account = self.select_best_tip_account().await;
        
        // Calculate confidence based on historical success
        let confidence = self.calculate_tip_confidence(opportunity_value, optimal_tip).await;
        
        // Estimate success rate based on tip amount
        let expected_success_rate = self.estimate_success_rate_from_tip(optimal_tip).await;
        
        let result = TipOptimizationResult {
            optimal_tip,
            recommended_tip_account,
            confidence,
            expected_success_rate,
        };
        
        Logger::status_update(&format!(
            "Optimal tip: {:.6} SOL, success_rate: {:.1}%, confidence: {:.1}%", 
            result.optimal_tip, 
            result.expected_success_rate * 100.0, 
            result.confidence * 100.0
        ));
        
        Ok(result)
    }
    
    async fn calculate_base_tip(&self, opportunity_value: f64) -> f64 {
        // Calculate base tip based on opportunity value
        // Higher value opportunities get higher tips to ensure inclusion
        
        if opportunity_value > 1.0 {
            0.003 // High-value: 0.003 SOL tip
        } else if opportunity_value > 0.5 {
            0.002 // Medium-high value: 0.002 SOL tip
        } else if opportunity_value > 0.1 {
            0.0015 // Medium value: 0.0015 SOL tip
        } else if opportunity_value > 0.01 {
            0.001 // Low-medium value: 0.001 SOL tip
        } else {
            0.0005 // Low value: minimum tip
        }
    }
    
    async fn calculate_tip_confidence(&self, opportunity_value: f64, tip_amount: f64) -> f64 {
        // Calculate confidence based on historical data and current conditions
        // In a real implementation, this would use historical tip-success data
        
        // Base confidence on opportunity value (higher value = higher confidence in success)
        let value_confidence = if opportunity_value > 1.0 { 0.9 } else if opportunity_value > 0.1 { 0.7 } else { 0.5 };
        
        // Confidence based on tip amount (higher tip = higher success probability)
        let tip_confidence = (tip_amount / 0.005).min(1.0); // Normalize against 0.005 SOL reference
        
        // Combine both factors
        (value_confidence * 0.6 + tip_confidence * 0.4).min(1.0)
    }
    
    async fn estimate_success_rate_from_tip(&self, tip_amount: f64) -> f64 {
        // Estimate success rate based on tip amount
        // Higher tips generally have higher success rates
        
        // This is a simplified model - in reality, success rates depend on many factors
        if tip_amount >= 0.003 {
            0.95 // Very high tip = very high success
        } else if tip_amount >= 0.0015 {
            0.85 // High tip = high success
        } else if tip_amount >= 0.001 {
            0.75 // Medium tip = medium-high success
        } else {
            0.60 // Low tip = lower success
        }
    }
    
    pub async fn get_bundle_timing_strategy(&self) -> BundleTimingStrategy {
        // Determine optimal timing strategy for bundle submission
        // This includes micro-delays, retry logic, and propagation waits
        
        // Get current network conditions
        let network_speed = self.assess_network_speed().await;
        
        let delay_micros = match network_speed {
            NetworkSpeed::Fast => 50_000,   // 50ms delay for fast networks
            NetworkSpeed::Medium => 100_000, // 100ms delay for medium networks
            NetworkSpeed::Slow => 200_000,   // 200ms delay for slow networks
        };
        
        // Determine retry count based on opportunity value
        let retry_count = if self.current_tip > 0.002 { 3 } else { 2 }; // More retries for higher value ops
        
        // Propagation wait time
        let propagation_wait_ms = match network_speed {
            NetworkSpeed::Fast => 100,
            NetworkSpeed::Medium => 200,
            NetworkSpeed::Slow => 400,
        };
        
        BundleTimingStrategy {
            delay_micros,
            retry_count,
            propagation_wait_ms,
        }
    }
    
    async fn assess_network_speed(&self) -> NetworkSpeed {
        // Assess network speed based on recent health checks
        let health = self.health_status.read().await;
        
        if health.latency_ms < 300.0 {
            NetworkSpeed::Fast
        } else if health.latency_ms < 800.0 {
            NetworkSpeed::Medium
        } else {
            NetworkSpeed::Slow
        }
    }
    
    pub async fn select_best_tip_account(&self) -> String {
        // Select the best tip account based on load balancing
        // In a real implementation, this would track usage of each tip account
        
        // For now, use round-robin selection
        use tokio::time::{sleep, Instant};
        let selection_time = Instant::now().elapsed().as_millis() as usize;
        let idx = selection_time % self.tip_accounts.len();
        
        self.tip_accounts[idx].to_string()
    }
    
    async fn get_available_tip_accounts(&self) -> Vec<String> {
        self.tip_accounts.iter().map(|pk| pk.to_string()).collect()
    }
    
    pub async fn should_fallback_to_drpc(&self, jito_tip_result: &TipOptimizationResult, drpc_expected_profit: f64) -> bool {
        // Decide whether to use DRPC instead of Jito based on cost-benefit analysis
        // Compare expected profit after Jito costs vs DRPC costs
        
        let jito_expected_net = drpc_expected_profit - jito_tip_result.optimal_tip;
        
        // If DRPC profit is close to or better than Jito net profit, consider DRPC
        // But also consider other factors like success rate
        let jito_effective_profit = jito_expected_net * jito_tip_result.expected_success_rate;
        let drpc_effective_profit = drpc_expected_profit * 0.85; // DRPC assumed 85% success rate
        
        // Use DRPC if Jito is unavailable OR if DRPC is more profitable after accounting for success rates
        !self.is_healthy().await || (drpc_effective_profit > jito_effective_profit && jito_tip_result.optimal_tip > 0.0015)
    }
    
    async fn is_healthy(&self) -> bool {
        let health = self.health_status.read().await;
        health.is_healthy
    }
    
    pub async fn record_tip_result(&self, tip_amount: f64, success: bool) {
        // Record the result of a tip for historical analysis
        let mut history = self.tip_adjustment_history.write().await;
        history.push((Instant::now(), tip_amount, success));
        
        // Keep only recent history (last 100 entries)
        if history.len() > 100 {
            let to_remove = history.len() - 100;
            history.drain(0..to_remove);
        }
    }
    
    pub async fn adjust_tip_based_on_history(&mut self) {
        // Adjust current tip based on historical success/failure patterns
        let history = self.tip_adjustment_history.read().await;
        
        if history.is_empty() {
            return;
        }
        
        // Calculate recent success rate
        let recent_successes: usize = history.iter()
            .filter(|(_, _, success)| *success)
            .count();
        
        let success_rate = recent_successes as f64 / history.len() as f64;
        
        // Adjust tip based on success rate
        if success_rate < 0.7 { // Low success rate, increase tip
            self.current_tip = (self.current_tip * 1.1).min(0.01); // Max 0.01 SOL
        } else if success_rate > 0.9 { // High success rate, can reduce tip
            self.current_tip = (self.current_tip * 0.95).max(0.0001); // Min 0.0001 SOL
        }
    }
    
    pub async fn prepare_bundle_for_submission(
        &self,
        transactions: Vec<String>,
        tip_amount: f64,
        tip_account: &str
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Preparing bundle with tip transaction for Jito submission");
        
        // Create the tip transaction
        let tip_tx = self.create_tip_transaction(tip_amount, tip_account).await?;
        
        // Combine the original transactions with the tip transaction
        let mut bundle_transactions = transactions;
        bundle_transactions.push(tip_tx);
        
        // Validate bundle size (Jito has limits)
        if bundle_transactions.len() > 5 {  // Jito typically allows up to 5 transactions per bundle
            Logger::status_update("Bundle size exceeds typical Jito limits, consider splitting");
        }
        
        Ok(bundle_transactions)
    }
    
    async fn create_tip_transaction(&self, tip_amount: f64, tip_account: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // In a real implementation, this would create an actual tip transaction using Solana SDK
        // For now, return a placeholder transaction
        
        // Convert tip amount to lamports
        let tip_lamports = (tip_amount * 1_000_000_000.0) as u64;
        
        // This would be implemented using Solana SDK to create:
        // 1. A transfer from the bot's wallet to the tip account
        // 2. Properly signed and serialized
        
        // Placeholder implementation
        Ok(format!("tip_transaction_{}_to_{}", tip_lamports, tip_account))
    }
    
    // Method to implement micro-delay strategies
    pub async fn implement_micro_delay(&self, strategy: &BundleTimingStrategy) {
        if strategy.delay_micros > 0 {
            tokio::time::sleep(Duration::from_micros(strategy.delay_micros)).await;
        }
    }
    
    // Method to check if Jito is available and preferable
    pub async fn is_jito_preferred(&self, opportunity_value: f64) -> bool {
        let health = self.health_status.read().await;
        
        // Use Jito if:
        // 1. It's healthy
        // 2. Opportunity value is high enough to justify premium service
        // 3. Network conditions favor Jito (low latency)
        
        health.is_healthy && 
        opportunity_value >= 0.01 &&  // At least 0.01 SOL opportunity
        health.latency_ms < 1000.0     // Reasonable latency
    }
    
    fn clone_for_spawn(&self) -> JitoOptimizer {
        JitoOptimizer {
            rpc_manager: Arc::clone(&self.rpc_manager),
            health_status: Arc::clone(&self.health_status),
            tip_accounts: self.tip_accounts.clone(),
            current_tip: self.current_tip,
            health_check_interval: self.health_check_interval,
            tip_adjustment_history: Arc::clone(&self.tip_adjustment_history),
        }
    }
}

#[derive(Debug, Clone)]
enum NetworkSpeed {
    Fast,    // < 300ms latency
    Medium,  // 300-800ms latency
    Slow,    // > 800ms latency
}

impl Clone for JitoOptimizer {
    fn clone(&self) -> Self {
        JitoOptimizer {
            rpc_manager: Arc::clone(&self.rpc_manager),
            health_status: Arc::clone(&self.health_status),
            tip_accounts: self.tip_accounts.clone(),
            current_tip: self.current_tip,
            health_check_interval: self.health_check_interval,
            tip_adjustment_history: Arc::clone(&self.tip_adjustment_history),
        }
    }
}

// Additional utilities for Jito optimization
pub mod jito_utils {
    use super::*;
    
    #[derive(Debug, Clone)]
    pub struct JitoBundle {
        pub transactions: Vec<String>,
        pub tip_amount: f64,
        pub tip_account: String,
        pub submission_time: std::time::SystemTime,
        pub expected_profit: f64,
    }
    
    impl JitoBundle {
        pub fn new(transactions: Vec<String>, tip_amount: f64, tip_account: String) -> Self {
            Self {
                transactions,
                tip_amount,
                tip_account,
                submission_time: std::time::SystemTime::now(),
                expected_profit: 0.0, // To be set later
            }
        }
        
        pub fn add_expected_profit(&mut self, profit: f64) {
            self.expected_profit = profit;
        }
        
        pub fn total_cost(&self) -> f64 {
            // Convert tip from SOL to a cost value
            self.tip_amount
        }
        
        pub fn net_expected_profit(&self) -> f64 {
            self.expected_profit - self.total_cost()
        }
    }
    
    pub struct JitoBundleOptimizer {
        pub min_net_profit: f64, // Minimum net profit after tips to submit bundle
    }
    
    impl JitoBundleOptimizer {
        pub fn new() -> Self {
            Self {
                min_net_profit: 0.005, // Require at least 0.005 SOL net profit
            }
        }
        
        pub fn should_submit_bundle(&self, bundle: &JitoBundle) -> bool {
            bundle.net_expected_profit() >= self.min_net_profit
        }
        
        pub fn optimize_bundle(&self, mut bundle: JitoBundle, opportunity_value: f64) -> JitoBundle {
            // Adjust the bundle based on opportunity value to maximize profit
            if opportunity_value > 1.0 {
                // High-value opportunities: use higher tip for better inclusion probability
                bundle.tip_amount = (bundle.tip_amount * 1.2).min(0.005);
            } else if opportunity_value < 0.01 {
                // Low-value opportunities: use minimal tip to preserve profit
                bundle.tip_amount = (bundle.tip_amount * 0.8).max(0.0001);
            }
            
            bundle
        }
    }
}