use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::logging::Logger;
use crate::utils::mev_strategies::MevStrategyType;

#[derive(Debug, Clone)]
pub struct RiskLimits {
    pub global_loss_per_bundle: f64,      // Max loss allowed per bundle (e.g., 0.01 SOL)
    pub global_daily_spending_limit: f64, // Max spending per day (e.g., 100 SOL)
    pub max_consecutive_failures: u32,    // Max consecutive failures before pause
    pub min_balance_threshold: f64,       // Min balance to continue operations
    pub max_strategy_failures: u32,       // Max failures per strategy before disabling
    pub session_timeout_minutes: u64,     // Session timeout (0 = no timeout)
}

#[derive(Debug, Clone)]
pub struct BalanceTracker {
    pub initial_balance: f64,
    pub current_balance: f64,
    pub balance_history: VecDeque<(std::time::SystemTime, f64)>,
    pub total_spent: f64,
    pub total_earned: f64,
}

#[derive(Debug, Clone)]
pub struct StrategyFailureTracker {
    pub strategy_type: MevStrategyType,
    pub failure_count: u32,
    pub last_failure_time: Option<std::time::SystemTime>,
    pub is_disabled: bool,
    pub disabled_until: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone)]
pub struct RiskEvent {
    pub timestamp: std::time::SystemTime,
    pub event_type: RiskEventType,
    pub details: String,
    pub value: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum RiskEventType {
    BalanceThresholdBreached,
    DailyLimitExceeded,
    ConsecutiveFailures,
    StrategyDisabled,
    LossLimitExceeded,
    SessionTimeout,
}

pub struct RiskManager {
    limits: RiskLimits,
    balance_tracker: Arc<RwLock<BalanceTracker>>,
    strategy_failures: Arc<RwLock<HashMap<String, StrategyFailureTracker>>>,
    risk_events: Arc<RwLock<Vec<RiskEvent>>>,
    session_start_time: std::time::SystemTime,
    global_daily_spent: Arc<RwLock<f64>>,
    consecutive_failure_count: Arc<RwLock<u32>>,
    last_operation_time: Arc<RwLock<std::time::SystemTime>>,
}

impl RiskManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let limits = RiskLimits {
            global_loss_per_bundle: std::env::var("GLOBAL_LOSS_PER_BUNDLE")
                .unwrap_or_else(|_| "0.01".to_string())
                .parse::<f64>()
                .map_err(|e| format!("Invalid GLOBAL_LOSS_PER_BUNDLE: {}", e))?,
            
            global_daily_spending_limit: std::env::var("GLOBAL_DAILY_SPENDING_LIMIT")
                .unwrap_or_else(|_| "10.0".to_string())
                .parse::<f64>()
                .map_err(|e| format!("Invalid GLOBAL_DAILY_SPENDING_LIMIT: {}", e))?,
            
            max_consecutive_failures: std::env::var("MAX_CONSECUTIVE_FAILURES")
                .unwrap_or_else(|_| "5".to_string())
                .parse::<u32>()
                .map_err(|e| format!("Invalid MAX_CONSECUTIVE_FAILURES: {}", e))?,
                
            min_balance_threshold: std::env::var("MIN_BALANCE_THRESHOLD")
                .unwrap_or_else(|_| "0.5".to_string())
                .parse::<f64>()
                .map_err(|e| format!("Invalid MIN_BALANCE_THRESHOLD: {}", e))?,
                
            max_strategy_failures: std::env::var("MAX_STRATEGY_FAILURES")
                .unwrap_or_else(|_| "3".to_string())
                .parse::<u32>()
                .map_err(|e| format!("Invalid MAX_STRATEGY_FAILURES: {}", e))?,
                
            session_timeout_minutes: std::env::var("SESSION_TIMEOUT_MINUTES")
                .unwrap_or_else(|_| "0".to_string()) // 0 means no timeout
                .parse::<u64>()
                .map_err(|e| format!("Invalid SESSION_TIMEOUT_MINUTES: {}", e))?,
        };
        
        Ok(Self {
            limits,
            balance_tracker: Arc::new(RwLock::new(BalanceTracker {
                initial_balance: 0.0,
                current_balance: 0.0,
                balance_history: VecDeque::new(),
                total_spent: 0.0,
                total_earned: 0.0,
            })),
            strategy_failures: Arc::new(RwLock::new(HashMap::new())),
            risk_events: Arc::new(RwLock::new(Vec::new())),
            session_start_time: std::time::SystemTime::now(),
            global_daily_spent: Arc::new(RwLock::new(0.0)),
            consecutive_failure_count: Arc::new(RwLock::new(0)),
            last_operation_time: Arc::new(RwLock::new(std::time::SystemTime::now())),
        })
    }
    
    pub async fn initialize_balance(&self, balance: f64) {
        let mut tracker = self.balance_tracker.write().await;
        tracker.initial_balance = balance;
        tracker.current_balance = balance;
        
        // Add to balance history
        tracker.balance_history.push_back((std::time::SystemTime::now(), balance));
        
        // Keep only recent history (last 1000 entries)
        if tracker.balance_history.len() > 1000 {
            let to_remove = tracker.balance_history.len() - 1000;
            tracker.balance_history.drain(0..to_remove);
        }
    }
    
    pub async fn update_balance(&self, new_balance: f64) -> Result<bool, RiskError> {
        let mut tracker = self.balance_tracker.write().await;
        let old_balance = tracker.current_balance;
        tracker.current_balance = new_balance;
        
        // Add to history
        tracker.balance_history.push_back((std::time::SystemTime::now(), new_balance));
        
        // Keep only recent history
        if tracker.balance_history.len() > 1000 {
            let to_remove = tracker.balance_history.len() - 1000;
            tracker.balance_history.drain(0..to_remove);
        }
        
        // Check if balance dropped below minimum threshold
        if new_balance < self.limits.min_balance_threshold {
            let drop_percentage = (old_balance - new_balance) / old_balance;
            self.record_risk_event(RiskEventType::BalanceThresholdBreached, 
                                 format!("Balance dropped below minimum threshold: {:.4} SOL", new_balance),
                                 Some(drop_percentage)).await;
            return Err(RiskError::BalanceTooLow(new_balance));
        }
        
        // Update total spent/earned based on the change
        if new_balance < old_balance {
            tracker.total_spent += (old_balance - new_balance);
        } else {
            tracker.total_earned += (new_balance - old_balance);
        }
        
        Ok(true)
    }
    
    pub async fn should_allow_operation(
        &self,
        expected_profit: f64,
        costs: f64
    ) -> Result<(), RiskError> {
        // Check all risk conditions before allowing operation
        
        // 1. Check session timeout
        if self.limits.session_timeout_minutes > 0 {
            let elapsed = self.session_start_time.elapsed()
                .map_err(|e| RiskError::InternalError(e.to_string()))?;
                
            if elapsed.as_secs() > self.limits.session_timeout_minutes * 60 {
                self.record_risk_event(RiskEventType::SessionTimeout,
                                     "Session timeout limit exceeded".to_string(),
                                     Some(self.limits.session_timeout_minutes as f64)).await;
                return Err(RiskError::SessionTimeout);
            }
        }
        
        // 2. Check daily spending limit
        let daily_spent = { *self.global_daily_spent.read().await };
        let potential_total = daily_spent + costs;
        
        if potential_total > self.limits.global_daily_spending_limit {
            self.record_risk_event(RiskEventType::DailyLimitExceeded,
                                 format!("Daily spending limit would be exceeded: {:.4} SOL > {:.4} SOL", 
                                        potential_total, self.limits.global_daily_spending_limit),
                                 Some(potential_total)).await;
            return Err(RiskError::DailySpendingLimitExceeded);
        }
        
        // 3. Check balance is sufficient for operation
        let current_balance = { self.balance_tracker.read().await.current_balance };
        if current_balance < costs {
            return Err(RiskError::InsufficientBalance);
        }
        
        // 4. Check consecutive failures
        let consecutive_failures = { *self.consecutive_failure_count.read().await };
        if consecutive_failures >= self.limits.max_consecutive_failures {
            self.record_risk_event(RiskEventType::ConsecutiveFailures,
                                 format!("Maximum consecutive failures reached: {}", consecutive_failures),
                                 Some(consecutive_failures as f64)).await;
            return Err(RiskError::MaxConsecutiveFailures);
        }
        
        Ok(())
    }
    
    pub async fn should_allow_strategy(
        &self,
        strategy_type: &MevStrategyType,
        expected_profit: f64,
        costs: f64
    ) -> Result<(), RiskError> {
        // First check general operation allowance
        self.should_allow_operation(expected_profit, costs).await?;
        
        // Check if this specific strategy is disabled due to failures
        let strategy_key = format!("{:?}", strategy_type);
        let failures = self.strategy_failures.read().await;
        
        if let Some(tracker) = failures.get(&strategy_key) {
            if tracker.is_disabled {
                if let Some(disabled_until) = tracker.disabled_until {
                    if std::time::SystemTime::now() < disabled_until {
                        return Err(RiskError::StrategyDisabled(strategy_key));
                    } else {
                        // Re-enable the strategy after timeout
                        drop(failures);
                        let mut failures = self.strategy_failures.write().await;
                        if let Some(mut tracker) = failures.get_mut(&strategy_key) {
                            tracker.is_disabled = false;
                            tracker.disabled_until = None;
                            Logger::status_update(&format!("Re-enabling strategy: {}", strategy_key));
                        }
                    }
                } else {
                    return Err(RiskError::StrategyDisabled(strategy_key));
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn record_successful_operation(&self, profit: f64) {
        // Reset consecutive failure counter
        *self.consecutive_failure_count.write().await = 0;
        
        // Add to daily spent if this was a cost (negative profit)
        if profit < 0.0 {
            let mut daily_spent = self.global_daily_spent.write().await;
            *daily_spent += profit.abs();
        }
        
        // Update last operation time
        *self.last_operation_time.write().await = std::time::SystemTime::now();
    }
    
    pub async fn record_failed_operation(&self) -> Result<(), RiskError> {
        // Increment consecutive failure counter
        let mut failure_count = self.consecutive_failure_count.write().await;
        *failure_count += 1;
        
        if *failure_count >= self.limits.max_consecutive_failures {
            self.record_risk_event(RiskEventType::ConsecutiveFailures,
                                 format!("Reached maximum consecutive failures: {}", *failure_count),
                                 Some(*failure_count as f64)).await;
            return Err(RiskError::MaxConsecutiveFailures);
        }
        
        Ok(())
    }
    
    pub async fn record_strategy_failure(&self, strategy_type: &MevStrategyType) {
        let strategy_key = format!("{:?}", strategy_type);
        let mut failures = self.strategy_failures.write().await;
        
        let tracker = failures.entry(strategy_key.clone()).or_insert_with(|| StrategyFailureTracker {
            strategy_type: strategy_type.clone(),
            failure_count: 0,
            last_failure_time: None,
            is_disabled: false,
            disabled_until: None,
        });
        
        tracker.failure_count += 1;
        tracker.last_failure_time = Some(std::time::SystemTime::now());
        
        // Check if we should disable this strategy
        if tracker.failure_count >= self.limits.max_strategy_failures && !tracker.is_disabled {
            tracker.is_disabled = true;
            // Disable for 1 hour (can be configured)
            let disable_until = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
            tracker.disabled_until = Some(disable_until);
            
            self.record_risk_event(RiskEventType::StrategyDisabled,
                                 format!("Strategy disabled due to too many failures: {}", strategy_key),
                                 Some(tracker.failure_count as f64)).await;
            
            Logger::error_occurred(&format!("Strategy {} has been disabled due to {} consecutive failures", 
                                          strategy_key, tracker.failure_count));
        }
    }
    
    pub async fn check_bundle_risk(
        &self,
        expected_loss: f64,
        costs: f64
    ) -> Result<(), RiskError> {
        // Check if the expected loss exceeds the global loss limit per bundle
        if expected_loss.abs() > self.limits.global_loss_per_bundle {
            self.record_risk_event(RiskEventType::LossLimitExceeded,
                                 format!("Expected loss exceeds bundle limit: {:.4} SOL > {:.4} SOL", 
                                        expected_loss.abs(), self.limits.global_loss_per_bundle),
                                 Some(expected_loss.abs())).await;
            return Err(RiskError::LossLimitExceeded);
        }
        
        Ok(())
    }
    
    async fn record_risk_event(&self, event_type: RiskEventType, details: String, value: Option<f64>) {
        let event = RiskEvent {
            timestamp: std::time::SystemTime::now(),
            event_type,
            details,
            value,
        };
        
        let mut events = self.risk_events.write().await;
        events.push(event);
        
        // Keep only recent events
        if events.len() > 1000 {
            let to_remove = events.len() - 1000;
            events.drain(0..to_remove);
        }
    }
    
    // Check if the bot should pause operations
    pub async fn should_pause_operations(&self) -> bool {
        let current_balance = { self.balance_tracker.read().await.current_balance };
        let consecutive_failures = { *self.consecutive_failure_count.read().await };
        
        // Pause if balance is too low or too many consecutive failures
        current_balance < self.limits.min_balance_threshold || 
        consecutive_failures >= self.limits.max_consecutive_failures
    }
    
    // Get current risk metrics
    pub async fn get_risk_metrics(&self) -> RiskMetrics {
        let tracker = self.balance_tracker.read().await;
        let daily_spent = *self.global_daily_spent.read().await;
        let consecutive_failures = *self.consecutive_failure_count.read().await;
        
        RiskMetrics {
            current_balance: tracker.current_balance,
            initial_balance: tracker.initial_balance,
            balance_change: tracker.current_balance - tracker.initial_balance,
            total_spent: tracker.total_spent,
            total_earned: tracker.total_earned,
            daily_spending: daily_spent,
            daily_spending_limit: self.limits.global_daily_spending_limit,
            consecutive_failures,
            max_consecutive_failures: self.limits.max_consecutive_failures,
            active_strategy_failures: self.count_active_strategy_failures().await,
        }
    }
    
    async fn count_active_strategy_failures(&self) -> usize {
        let failures = self.strategy_failures.read().await;
        failures.values().filter(|tracker| tracker.is_disabled).count()
    }
    
    // Reset risk state (for testing or manual override)
    pub async fn reset_risk_state(&self) {
        *self.consecutive_failure_count.write().await = 0;
        *self.last_operation_time.write().await = std::time::SystemTime::now();
        
        // Reset strategy failures
        let mut failures = self.strategy_failures.write().await;
        for tracker in failures.values_mut() {
            tracker.failure_count = 0;
            tracker.is_disabled = false;
            tracker.disabled_until = None;
        }
    }
    
    // Manual override to enable a disabled strategy
    pub async fn enable_strategy(&self, strategy_type: &MevStrategyType) {
        let strategy_key = format!("{:?}", strategy_type);
        let mut failures = self.strategy_failures.write().await;
        
        if let Some(mut tracker) = failures.get_mut(&strategy_key) {
            tracker.is_disabled = false;
            tracker.disabled_until = None;
            tracker.failure_count = 0; // Reset failure count when manually enabled
            
            Logger::status_update(&format!("Manually re-enabled strategy: {}", strategy_key));
        }
    }
    
    // Get risk events in the last N minutes
    pub async fn get_recent_risk_events(&self, minutes: u64) -> Vec<RiskEvent> {
        let events = self.risk_events.read().await;
        let time_threshold = std::time::SystemTime::now() 
            - std::time::Duration::from_secs(minutes * 60);
            
        events.iter()
            .filter(|event| event.timestamp > time_threshold)
            .cloned()
            .collect()
    }
    
    // Check if we're within daily limits
    pub async fn check_daily_limits(&self, amount: f64) -> Result<(), RiskError> {
        let daily_spent = *self.global_daily_spent.read().await;
        let total_with_new_amount = daily_spent + amount;
        
        if total_with_new_amount > self.limits.global_daily_spending_limit {
            return Err(RiskError::DailySpendingLimitExceeded);
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RiskMetrics {
    pub current_balance: f64,
    pub initial_balance: f64,
    pub balance_change: f64,
    pub total_spent: f64,
    pub total_earned: f64,
    pub daily_spending: f64,
    pub daily_spending_limit: f64,
    pub consecutive_failures: u32,
    pub max_consecutive_failures: u32,
    pub active_strategy_failures: usize,
}

#[derive(Debug)]
pub enum RiskError {
    BalanceTooLow(f64),
    DailySpendingLimitExceeded,
    MaxConsecutiveFailures,
    LossLimitExceeded,
    StrategyDisabled(String),
    SessionTimeout,
    InsufficientBalance,
    InternalError(String),
}

impl std::fmt::Display for RiskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskError::BalanceTooLow(balance) => write!(f, "Balance too low: {:.4} SOL", balance),
            RiskError::DailySpendingLimitExceeded => write!(f, "Daily spending limit exceeded"),
            RiskError::MaxConsecutiveFailures => write!(f, "Maximum consecutive failures reached"),
            RiskError::LossLimitExceeded => write!(f, "Loss limit exceeded"),
            RiskError::StrategyDisabled(strategy) => write!(f, "Strategy disabled: {}", strategy),
            RiskError::SessionTimeout => write!(f, "Session timeout"),
            RiskError::InsufficientBalance => write!(f, "Insufficient balance"),
            RiskError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for RiskError {}

// Additional utilities for risk management
pub mod risk_utils {
    use super::*;
    
    #[derive(Debug, Clone)]
    pub struct PositionSizer {
        pub max_position_size: f64, // Max % of balance to risk per trade
        pub max_loss_per_trade: f64, // Max absolute loss per trade
        pub risk_reward_ratio: f64,  // Min risk/reward ratio
    }
    
    impl PositionSizer {
        pub fn new() -> Self {
            Self {
                max_position_size: 0.05, // Max 5% of balance
                max_loss_per_trade: 0.01, // Max 0.01 SOL loss
                risk_reward_ratio: 1.0 / 3.0, // 1:3 risk/reward (expect 3x reward for 1x risk)
            }
        }
        
        pub async fn calculate_position_size(
            &self,
            current_balance: f64,
            expected_profit: f64,
            estimated_loss: f64
        ) -> f64 {
            // Calculate max position based on balance
            let max_by_balance = current_balance * self.max_position_size;
            
            // Calculate position based on expected risk/reward
            let min_profit_for_risk = estimated_loss * self.risk_reward_ratio;
            let max_by_risk_reward = if expected_profit >= min_profit_for_risk {
                max_by_balance // Allow full position if risk/reward is acceptable
            } else {
                0.0 // Don't trade if risk/reward is poor
            };
            
            // Return minimum of all constraints
            max_by_balance.min(max_by_risk_reward).min(self.max_loss_per_trade)
        }
    }
    
    // Circuit breaker to pause operations if conditions are unfavorable
    pub struct CircuitBreaker {
        pub enabled: bool,
        pub consecutive_failure_threshold: u32,
        pub cooldown_period_minutes: u64,
    }
    
    impl CircuitBreaker {
        pub fn new() -> Self {
            Self {
                enabled: true,
                consecutive_failure_threshold: 5,
                cooldown_period_minutes: 10,
            }
        }
        
        pub async fn should_break_circuit(&self, consecutive_failures: u32) -> bool {
            if !self.enabled {
                return false;
            }
            
            consecutive_failures >= self.consecutive_failure_threshold
        }
        
        pub async fn get_cooldown_remaining(&self, last_failure_time: Option<std::time::SystemTime>) -> Option<u64> {
            if let Some(failure_time) = last_failure_time {
                let elapsed = failure_time.elapsed().ok()?;
                let cooldown_seconds = self.cooldown_period_minutes * 60;
                
                if elapsed.as_secs() < cooldown_seconds {
                    Some(cooldown_seconds - elapsed.as_secs())
                } else {
                    None // Cooldown period has passed
                }
            } else {
                None
            }
        }
    }
}