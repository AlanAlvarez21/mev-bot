use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Arc, Mutex};
use crate::logging::Logger;

#[derive(Debug)]
pub struct RiskManager {
    pub max_loss_per_bundle: f64,           // Max loss allowed per bundle
    pub max_daily_loss: f64,               // Max loss allowed per day
    pub max_consecutive_losses: u32,       // Max number of consecutive losses before pause
    pub volatility_threshold: f64,         // Threshold for market volatility
    pub min_profitability_ratio: f64,      // Minimum profit/cost ratio
    pub position_size_limit: f64,          // Max position size in SOL
    
    // Runtime state wrapped in Arc<Mutex<>> for shared mutable access
    state: Arc<Mutex<RiskState>>,
}

#[derive(Debug)]
struct RiskState {
    daily_losses: f64,
    consecutive_losses: u32,
    last_reset_time: u64,
    transaction_history: HashMap<String, TransactionResult>,
}

#[derive(Debug, Clone)]
pub struct TransactionResult {
    pub signature: String,
    pub profit: f64,
    pub timestamp: u64,
    pub success: bool,
}

impl RiskManager {
    pub fn new() -> Self {
        // Load from environment variables or use defaults
        let max_loss_per_bundle = std::env::var("MAX_LOSS_PER_BUNDLE")
            .unwrap_or_else(|_| "0.1".to_string())
            .parse::<f64>()
            .unwrap_or(0.1);
            
        let max_daily_loss = std::env::var("MAX_DAILY_LOSS")
            .unwrap_or_else(|_| "1.0".to_string())
            .parse::<f64>()
            .unwrap_or(1.0);
            
        let max_consecutive_losses = std::env::var("MAX_CONSECUTIVE_LOSSES")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<u32>()
            .unwrap_or(5);
            
        let volatility_threshold = std::env::var("VOLATILITY_THRESHOLD")
            .unwrap_or_else(|_| "0.05".to_string()) // 5% volatility threshold
            .parse::<f64>()
            .unwrap_or(0.05);
            
        let min_profitability_ratio = std::env::var("MIN_PROFITABILITY_RATIO")
            .unwrap_or_else(|_| "1.2".to_string()) // Require 20% more profit than costs
            .parse::<f64>()
            .unwrap_or(1.2);
            
        let position_size_limit = std::env::var("POSITION_SIZE_LIMIT")
            .unwrap_or_else(|_| "5.0".to_string()) // Max 5 SOL per position
            .parse::<f64>()
            .unwrap_or(5.0);

        let state = Arc::new(Mutex::new(RiskState {
            daily_losses: 0.0,
            consecutive_losses: 0,
            last_reset_time: Self::current_timestamp(),
            transaction_history: HashMap::new(),
        }));

        Self {
            max_loss_per_bundle,
            max_daily_loss,
            max_consecutive_losses,
            volatility_threshold,
            min_profitability_ratio,
            position_size_limit,
            state,
        }
    }

    pub fn should_allow_transaction(&self, estimated_profit: f64, expected_cost: f64) -> bool {
        let mut state = self.state.lock().unwrap();
        
        // Check if we should reset daily counters (new day)
        self.reset_daily_counters_if_needed(&mut state);
        
        // Check max loss per bundle
        let net_result = estimated_profit - expected_cost;
        if net_result < -self.max_loss_per_bundle {
            Logger::status_update(&format!(
                "Rejecting transaction: expected loss {:.6} SOL exceeds max loss {:.6} SOL",
                -net_result, self.max_loss_per_bundle
            ));
            return false;
        }
        
        // Check profitability ratio
        if estimated_profit < expected_cost * self.min_profitability_ratio {
            Logger::status_update(&format!(
                "Rejecting transaction: profit/cost ratio {:.2} below minimum {:.2}",
                if expected_cost > 0.0 { estimated_profit / expected_cost } else { 0.0 },
                self.min_profitability_ratio
            ));
            return false;
        }
        
        // Check consecutive losses
        if state.consecutive_losses >= self.max_consecutive_losses {
            Logger::status_update(&format!(
                "Rejecting transaction: too many consecutive losses ({})",
                state.consecutive_losses
            ));
            return false;
        }
        
        // Check if position size is too large
        if expected_cost > self.position_size_limit {
            Logger::status_update(&format!(
                "Rejecting transaction: position size {:.6} SOL exceeds limit {:.6} SOL",
                expected_cost, self.position_size_limit
            ));
            return false;
        }
        
        true
    }

    pub fn record_transaction_result(&self, result: TransactionResult) {
        let mut state = self.state.lock().unwrap();
        
        if !result.success || result.profit < 0.0 {
            state.consecutive_losses += 1;
            state.daily_losses += result.profit.abs();
        } else {
            state.consecutive_losses = 0; // Reset on success
        }
        
        state.transaction_history.insert(result.signature.clone(), result.clone());
        
        // Keep history size manageable
        if state.transaction_history.len() > 1000 {
            // Remove oldest entries - keep only recent 500
            let mut entries: Vec<_> = state.transaction_history.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            entries.sort_by(|a, b| a.1.timestamp.cmp(&b.1.timestamp)); // Sort by timestamp
            
            // Keep only the most recent 500 entries
            if entries.len() > 500 {
                let to_remove: Vec<_> = entries.iter().take(entries.len() - 500).map(|(k, _)| k.clone()).collect();
                
                for sig in to_remove {
                    state.transaction_history.remove(&sig);
                }
            }
        }
    }

    pub fn check_market_volatility(&self, current_price: f64, previous_price: f64) -> bool {
        if previous_price == 0.0 {
            return true; // Can't calculate volatility if previous price is 0
        }
        
        let change_ratio = ((current_price - previous_price) / previous_price).abs();
        
        if change_ratio > self.volatility_threshold {
            Logger::status_update(&format!(
                "High market volatility detected: {:.2}% change exceeds threshold {:.2}%",
                change_ratio * 100.0,
                self.volatility_threshold * 100.0
            ));
            return false; // Don't trade in high volatility
        }
        
        true // Low volatility, safe to trade
    }

    fn reset_daily_counters_if_needed(&self, state: &mut RiskState) {
        let now = Self::current_timestamp();
        let seconds_in_day = 24 * 3600;
        
        if now - state.last_reset_time >= seconds_in_day {
            state.daily_losses = 0.0;
            state.consecutive_losses = 0;
            state.last_reset_time = now;
            Logger::status_update("Daily risk counters reset");
        }
    }

    pub fn get_risk_metrics(&self) -> RiskMetrics {
        let state = self.state.lock().unwrap();
        RiskMetrics {
            daily_losses: state.daily_losses,
            consecutive_losses: state.consecutive_losses,
            total_transactions: state.transaction_history.len(),
            success_rate: self.calculate_success_rate(&state),
        }
    }

    fn calculate_success_rate(&self, state: &RiskState) -> f64 {
        if state.transaction_history.is_empty() {
            return 0.0;
        }
        
        let successful_transactions = state.transaction_history
            .values()
            .filter(|result| result.success && result.profit >= 0.0)
            .count();
            
        successful_transactions as f64 / state.transaction_history.len() as f64
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[derive(Debug, Clone)]
pub struct RiskMetrics {
    pub daily_losses: f64,
    pub consecutive_losses: u32,
    pub total_transactions: usize,
    pub success_rate: f64,
}