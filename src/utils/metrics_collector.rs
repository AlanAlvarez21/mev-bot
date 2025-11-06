use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use crate::logging::Logger;
use crate::utils::mev_strategies::{MevStrategyType, MevStrategyResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityMetrics {
    pub estimated_profit: f64,
    pub actual_profit: f64,
    pub fees_paid: f64,
    pub tip_paid: f64,
    pub confidence_score: f64,
    pub simulation_results: Vec<SimulationResultMetric>,
    pub execution_time_ms: u64,
    pub success: bool,
    pub opportunity_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResultMetric {
    pub is_valid: bool,
    pub net_profit: f64,
    pub estimated_fees: f64,
    pub jito_tip: f64,
    pub slippage: f64,
    pub confidence_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub total_opportunities_detected: u64,
    pub total_opportunities_evaluated: u64,
    pub total_opportunities_executed: u64,
    pub total_successful_executions: u64,
    pub total_profit: f64,
    pub total_fees_paid: f64,
    pub total_tips_paid: f64,
    pub false_positive_rate: f64,
    pub execution_success_rate: f64,
    pub avg_profit_per_success: f64,
    pub avg_execution_time_ms: f64,
    pub start_time: std::time::SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyMetrics {
    pub strategy_type: MevStrategyType,
    pub executions: u64,
    pub successes: u64,
    pub total_profit: f64,
    pub total_fees: f64,
    pub total_tips: f64,
    pub avg_profit_per_execution: f64,
    pub avg_execution_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcMetrics {
    pub endpoint_type: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub avg_response_time_ms: f64,
    pub error_rate: f64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub timestamp: std::time::SystemTime,
    pub alert_type: AlertType,
    pub message: String,
    pub severity: AlertSeverity,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    BalanceDrop,
    ConsecutiveFailures,
    HighLatency,
    LowSuccessRate,
    UnexpectedError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

pub struct MetricsCollector {
    system_metrics: Arc<RwLock<SystemMetrics>>,
    strategy_metrics: Arc<RwLock<HashMap<String, StrategyMetrics>>>,
    rpc_metrics: Arc<RwLock<HashMap<String, RpcMetrics>>>,
    opportunity_history: Arc<RwLock<Vec<OpportunityMetrics>>>,
    alert_history: Arc<RwLock<Vec<AlertEvent>>>,
    
    // Monitoring thresholds
    pub balance_drop_threshold: f64,    // Percentage drop to trigger alert
    pub consecutive_failures_threshold: u32, // Number of failures to trigger alert
    pub success_rate_threshold: f64,    // Minimum success rate threshold
    pub max_opportunity_age_ms: u64,    // Maximum age of opportunity metrics to keep
}

impl MetricsCollector {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            system_metrics: Arc::new(RwLock::new(SystemMetrics {
                total_opportunities_detected: 0,
                total_opportunities_evaluated: 0,
                total_opportunities_executed: 0,
                total_successful_executions: 0,
                total_profit: 0.0,
                total_fees_paid: 0.0,
                total_tips_paid: 0.0,
                false_positive_rate: 0.0,
                execution_success_rate: 0.0,
                avg_profit_per_success: 0.0,
                avg_execution_time_ms: 0.0,
                start_time: std::time::SystemTime::now(),
            })),
            strategy_metrics: Arc::new(RwLock::new(HashMap::new())),
            rpc_metrics: Arc::new(RwLock::new(HashMap::new())),
            opportunity_history: Arc::new(RwLock::new(Vec::new())),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            balance_drop_threshold: 0.1,      // 10% drop
            consecutive_failures_threshold: 5, // 5 consecutive failures
            success_rate_threshold: 0.7,      // 70% success rate
            max_opportunity_age_ms: 3_600_000, // Keep metrics for 1 hour (in milliseconds)
        })
    }
    
    pub async fn record_opportunity_detected(&self) {
        let mut metrics = self.system_metrics.write().await;
        metrics.total_opportunities_detected += 1;
    }
    
    pub async fn record_opportunity_evaluated(&self) {
        let mut metrics = self.system_metrics.write().await;
        metrics.total_opportunities_evaluated += 1;
    }
    
    pub async fn record_strategy_execution(&self, result: &MevStrategyResult) {
        let mut sys_metrics = self.system_metrics.write().await;
        sys_metrics.total_opportunities_executed += 1;
        
        if result.success {
            sys_metrics.total_successful_executions += 1;
            sys_metrics.total_profit += result.profit;
        }
        
        sys_metrics.total_fees_paid += result.fees_paid;
        sys_metrics.total_tips_paid += result.tip_paid;
        
        // Update success rate
        if sys_metrics.total_opportunities_executed > 0 {
            sys_metrics.execution_success_rate = 
                sys_metrics.total_successful_executions as f64 / 
                sys_metrics.total_opportunities_executed as f64;
        }
        
        // Update average profit per success
        if sys_metrics.total_successful_executions > 0 {
            sys_metrics.avg_profit_per_success = 
                sys_metrics.total_profit / 
                sys_metrics.total_successful_executions as f64;
        }
        
        // Update to be implemented in record_opportunity_result
        
        // Record strategy-specific metrics
        self.record_strategy_specific_metrics(result).await;
    }
    
    async fn record_strategy_specific_metrics(&self, result: &MevStrategyResult) {
        let strategy_key = format!("{:?}", result.strategy_type);
        let mut strategy_map = self.strategy_metrics.write().await;
        
        let strategy_metrics = strategy_map.entry(strategy_key).or_insert_with(|| StrategyMetrics {
            strategy_type: result.strategy_type.clone(),
            executions: 0,
            successes: 0,
            total_profit: 0.0,
            total_fees: 0.0,
            total_tips: 0.0,
            avg_profit_per_execution: 0.0,
            avg_execution_time_ms: 0.0,
        });
        
        strategy_metrics.executions += 1;
        if result.success {
            strategy_metrics.successes += 1;
            strategy_metrics.total_profit += result.profit;
        }
        
        strategy_metrics.total_fees += result.fees_paid;
        strategy_metrics.total_tips += result.tip_paid;
        
        // Update averages
        strategy_metrics.avg_profit_per_execution = 
            strategy_metrics.total_profit / strategy_metrics.executions as f64;
        
        strategy_metrics.avg_execution_time_ms = 
            (strategy_metrics.avg_execution_time_ms * (strategy_metrics.executions as f64 - 1.0) + 
             result.execution_time_ms as f64) / strategy_metrics.executions as f64;
    }
    
    pub async fn record_opportunity_result(
        &self,
        estimated_profit: f64,
        actual_profit: f64,
        fees_paid: f64,
        tip_paid: f64,
        confidence_score: f64,
        simulation_results: Vec<SimulationResultMetric>,
        execution_time_ms: u64,
        success: bool,
        opportunity_type: String,
    ) {
        let opportunity_metric = OpportunityMetrics {
            estimated_profit,
            actual_profit,
            fees_paid,
            tip_paid,
            confidence_score,
            simulation_results,
            execution_time_ms,
            success,
            opportunity_type,
        };
        
        // Add to history
        let mut history = self.opportunity_history.write().await;
        history.push(opportunity_metric);
        
        // Keep only recent records to manage memory
        if history.len() > 10000 { // Keep last 10,000 records
            let to_remove = history.len() - 10000;
            history.drain(0..to_remove);
        }
        
        // Update system metrics
        let mut sys_metrics = self.system_metrics.write().await;
        sys_metrics.avg_execution_time_ms = 
            (sys_metrics.avg_execution_time_ms * (sys_metrics.total_opportunities_executed as f64 - 1.0) + 
             execution_time_ms as f64) / sys_metrics.total_opportunities_executed as f64;
    }
    
    pub async fn record_rpc_call(
        &self,
        endpoint_type: &str,
        success: bool,
        response_time_ms: f64,
        bytes_sent: u64,
        bytes_received: u64,
    ) {
        let mut rpc_map = self.rpc_metrics.write().await;
        
        let key = endpoint_type.to_string();
        let rpc_metrics = rpc_map.entry(key).or_insert_with(|| RpcMetrics {
            endpoint_type: endpoint_type.to_string(),
            total_requests: 0,
            successful_requests: 0,
            avg_response_time_ms: 0.0,
            error_rate: 0.0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
        });
        
        rpc_metrics.total_requests += 1;
        if success {
            rpc_metrics.successful_requests += 1;
        }
        
        rpc_metrics.total_bytes_sent += bytes_sent;
        rpc_metrics.total_bytes_received += bytes_received;
        
        // Update response time average
        rpc_metrics.avg_response_time_ms = 
            (rpc_metrics.avg_response_time_ms * (rpc_metrics.total_requests as f64 - 1.0) + 
             response_time_ms) / rpc_metrics.total_requests as f64;
        
        // Update error rate
        rpc_metrics.error_rate = 
            (rpc_metrics.total_requests - rpc_metrics.successful_requests) as f64 / 
            rpc_metrics.total_requests as f64;
    }
    
    // Alert system
    pub async fn check_and_trigger_alerts(&self, current_balance: f64, previous_balance: f64) {
        // Check for balance drop
        if previous_balance > 0.0 {
            let balance_drop_percentage = (previous_balance - current_balance) / previous_balance;
            if balance_drop_percentage > self.balance_drop_threshold {
                self.trigger_alert(AlertType::BalanceDrop, 
                                 AlertSeverity::Warning,
                                 format!("Balance dropped by {:.2}%", balance_drop_percentage * 100.0),
                                 Some(balance_drop_percentage)).await;
            }
        }
        
        // Check success rate
        let sys_metrics = self.system_metrics.read().await;
        if sys_metrics.total_opportunities_executed >= 10 && 
           sys_metrics.execution_success_rate < self.success_rate_threshold {
            self.trigger_alert(AlertType::LowSuccessRate,
                             AlertSeverity::Warning,
                             format!("Success rate dropped to {:.2}%", sys_metrics.execution_success_rate * 100.0),
                             Some(sys_metrics.execution_success_rate)).await;
        }
    }
    
    async fn trigger_alert(&self, alert_type: AlertType, severity: AlertSeverity, message: String, value: Option<f64>) {
        let alert = AlertEvent {
            timestamp: std::time::SystemTime::now(),
            alert_type,
            message,
            severity: severity.clone(),
            value,
        };
        
        let mut alerts = self.alert_history.write().await;
        alerts.push(alert.clone());
        
        // Log the alert
        Logger::error_occurred(&format!("[ALERT - {:?}] {}", severity, alert.message));
        
        // Keep only recent alerts
        if alerts.len() > 1000 { // Keep last 1000 alerts
            let to_remove = alerts.len() - 1000;
            alerts.drain(0..to_remove);
        }
    }
    
    // Retrieve metrics
    pub async fn get_system_metrics(&self) -> SystemMetrics {
        self.system_metrics.read().await.clone()
    }
    
    pub async fn get_strategy_metrics(&self, strategy_type: &MevStrategyType) -> Option<StrategyMetrics> {
        let map = self.strategy_metrics.read().await;
        let key = format!("{:?}", strategy_type);
        map.get(&key).cloned()
    }
    
    pub async fn get_all_strategy_metrics(&self) -> Vec<StrategyMetrics> {
        let map = self.strategy_metrics.read().await;
        map.values().cloned().collect()
    }
    
    pub async fn get_rpc_metrics(&self, endpoint_type: &str) -> Option<RpcMetrics> {
        let map = self.rpc_metrics.read().await;
        map.get(endpoint_type).cloned()
    }
    
    pub async fn get_recent_alerts(&self, count: usize) -> Vec<AlertEvent> {
        let alerts = self.alert_history.read().await;
        let start = alerts.len().saturating_sub(count);
        alerts[start..].to_vec()
    }
    
    // Export metrics to JSON
    pub async fn export_metrics_json(&self) -> Result<String, Box<dyn std::error::Error>> {
        let export = MetricsExport {
            system: self.get_system_metrics().await,
            strategies: self.get_all_strategy_metrics().await,
            alerts: self.get_recent_alerts(50).await, // Last 50 alerts
            export_time: std::time::SystemTime::now(),
        };
        
        let json = serde_json::to_string_pretty(&export)
            .map_err(|e| format!("Failed to serialize metrics: {}", e))?;
        
        Ok(json)
    }
    
    // Export to a simple SQLite database (if available)
    pub async fn export_to_storage(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.export_metrics_json().await?;
        std::fs::write(file_path, json)
            .map_err(|e| format!("Failed to write metrics to file: {}", e).into())
    }
    
    // Calculate false positive rate
    pub async fn calculate_false_positive_rate(&self) -> f64 {
        let sys_metrics = self.system_metrics.read().await;
        
        if sys_metrics.total_opportunities_evaluated == 0 {
            0.0
        } else {
            // False positive rate is the rate of detected opportunities that were not profitable when executed
            let total_detected = sys_metrics.total_opportunities_detected;
            let total_evaluated = sys_metrics.total_opportunities_evaluated;
            
            // For now, we'll calculate this as 1 - evaluation_rate as a proxy
            // In a more complete implementation, we'd track which opportunities were false positives
            if total_detected > 0 {
                (total_detected - total_evaluated) as f64 / total_detected as f64
            } else {
                0.0
            }
        }
    }
    
    // Get performance metrics by time window
    pub async fn get_performance_in_window(&self, minutes: u64) -> SystemMetrics {
        let start_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
            
        let window_start = start_time - (minutes * 60 * 1000); // Convert minutes to milliseconds
        
        // In a real implementation, we'd filter metrics by time window
        // For now, return the full system metrics
        self.get_system_metrics().await
    }
    
    // Reset metrics (for testing or new sessions)
    pub async fn reset_metrics(&self) {
        let mut sys_metrics = self.system_metrics.write().await;
        *sys_metrics = SystemMetrics {
            total_opportunities_detected: 0,
            total_opportunities_evaluated: 0,
            total_opportunities_executed: 0,
            total_successful_executions: 0,
            total_profit: 0.0,
            total_fees_paid: 0.0,
            total_tips_paid: 0.0,
            false_positive_rate: 0.0,
            execution_success_rate: 0.0,
            avg_profit_per_success: 0.0,
            avg_execution_time_ms: 0.0,
            start_time: std::time::SystemTime::now(),
        };
        
        // Clear other metrics
        *self.strategy_metrics.write().await = HashMap::new();
        *self.rpc_metrics.write().await = HashMap::new();
        *self.opportunity_history.write().await = Vec::new();
        *self.alert_history.write().await = Vec::new();
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MetricsExport {
    system: SystemMetrics,
    strategies: Vec<StrategyMetrics>,
    alerts: Vec<AlertEvent>,
    export_time: std::time::SystemTime,
}

// Implement Prometheus-style metrics for monitoring
pub mod prometheus_exporter {
    use super::*;
    
    pub struct PrometheusMetrics {
        metrics_collector: Arc<MetricsCollector>,
    }
    
    impl PrometheusMetrics {
        pub fn new(collector: Arc<MetricsCollector>) -> Self {
            Self {
                metrics_collector: collector,
            }
        }
        
        pub async fn format_prometheus(&self) -> String {
            let sys_metrics = self.metrics_collector.get_system_metrics().await;
            let strategy_metrics = self.metrics_collector.get_all_strategy_metrics().await;
            
            let mut output = String::new();
            
            // System metrics
            output.push_str(&format!("# HELP mev_bot_total_opportunities_detected Total opportunities detected\n"));
            output.push_str(&format!("mev_bot_total_opportunities_detected {}\n", sys_metrics.total_opportunities_detected));
            
            output.push_str(&format!("# HELP mev_bot_total_opportunities_executed Total opportunities executed\n"));
            output.push_str(&format!("mev_bot_total_opportunities_executed {}\n", sys_metrics.total_opportunities_executed));
            
            output.push_str(&format!("# HELP mev_bot_total_successful_executions Total successful executions\n"));
            output.push_str(&format!("mev_bot_total_successful_executions {}\n", sys_metrics.total_successful_executions));
            
            output.push_str(&format!("# HELP mev_bot_total_profit Total profit in SOL\n"));
            output.push_str(&format!("mev_bot_total_profit {:.6}\n", sys_metrics.total_profit));
            
            output.push_str(&format!("# HELP mev_bot_execution_success_rate Success rate of executions\n"));
            output.push_str(&format!("mev_bot_execution_success_rate {:.4}\n", sys_metrics.execution_success_rate));
            
            output.push_str(&format!("# HELP mev_bot_avg_profit_per_success Average profit per successful execution\n"));
            output.push_str(&format!("mev_bot_avg_profit_per_success {:.6}\n", sys_metrics.avg_profit_per_success));
            
            // Strategy-specific metrics
            for strategy in strategy_metrics {
                let strategy_name = format!("{:?}", strategy.strategy_type).to_lowercase();
                
                output.push_str(&format!("# HELP mev_bot_strategy_executions_total Total executions for {}\n", strategy_name));
                output.push_str(&format!("mev_bot_strategy_{}_executions_total {}\n", strategy_name, strategy.executions));
                
                output.push_str(&format!("# HELP mev_bot_strategy_successes_total Total successes for {}\n", strategy_name));
                output.push_str(&format!("mev_bot_strategy_{}_successes_total {}\n", strategy_name, strategy.successes));
                
                output.push_str(&format!("# HELP mev_bot_strategy_total_profit Total profit for {}\n", strategy_name));
                output.push_str(&format!("mev_bot_strategy_{}_total_profit {:.6}\n", strategy_name, strategy.total_profit));
            }
            
            output
        }
    }
}

impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        MetricsCollector {
            system_metrics: Arc::clone(&self.system_metrics),
            strategy_metrics: Arc::clone(&self.strategy_metrics),
            rpc_metrics: Arc::clone(&self.rpc_metrics),
            opportunity_history: Arc::clone(&self.opportunity_history),
            alert_history: Arc::clone(&self.alert_history),
            balance_drop_threshold: self.balance_drop_threshold,
            consecutive_failures_threshold: self.consecutive_failures_threshold,
            success_rate_threshold: self.success_rate_threshold,
            max_opportunity_age_ms: self.max_opportunity_age_ms,
        }
    }
}