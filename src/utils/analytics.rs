use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::Value;
use serde::{Serialize, Deserialize};
use crate::logging::Logger;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analytics {
    pub total_profit: f64,
    pub total_transactions: u64,
    pub successful_transactions: u64,
    pub failed_transactions: u64,
    pub avg_profit_per_successful: f64,
    pub total_fees_paid: f64,
    pub start_time: u64,
    pub strategy_performance: HashMap<String, StrategyStats>,
    pub opportunity_analysis: HashMap<String, OpportunityStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyStats {
    pub executions: u64,
    pub successful_executions: u64,
    pub total_profit: f64,
    pub avg_profit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityStats {
    pub detected: u64,
    pub executed: u64,
    pub profitable_executions: u64,
    pub total_profit: f64,
    pub avg_execution_time_ms: f64,
}

impl Analytics {
    pub fn new() -> Self {
        Self {
            total_profit: 0.0,
            total_transactions: 0,
            successful_transactions: 0,
            failed_transactions: 0,
            avg_profit_per_successful: 0.0,
            total_fees_paid: 0.0,
            start_time: Self::current_timestamp(),
            strategy_performance: HashMap::new(),
            opportunity_analysis: HashMap::new(),
        }
    }

    pub fn record_transaction(&mut self, strategy: &str, success: bool, profit: f64, fees: f64) {
        self.total_transactions += 1;
        
        if success {
            self.successful_transactions += 1;
            self.total_profit += profit;
        } else {
            self.failed_transactions += 1;
            self.total_profit -= fees; // Record fees as loss on failure
        }
        
        self.total_fees_paid += fees;
        
        // Update strategy performance
        let strategy_stats = self.strategy_performance.entry(strategy.to_string()).or_insert_with(|| {
            StrategyStats {
                executions: 0,
                successful_executions: 0,
                total_profit: 0.0,
                avg_profit: 0.0,
            }
        });
        
        strategy_stats.executions += 1;
        if success {
            strategy_stats.successful_executions += 1;
            strategy_stats.total_profit += profit;
        }
        
        if strategy_stats.executions > 0 {
            strategy_stats.avg_profit = strategy_stats.total_profit / strategy_stats.executions as f64;
        }
        
        if self.successful_transactions > 0 {
            self.avg_profit_per_successful = self.total_profit / self.successful_transactions as f64;
        }
    }

    pub fn record_opportunity(&mut self, opportunity_type: &str, executed: bool, profitable: bool, profit: f64, execution_time_ms: f64) {
        let opp_stats = self.opportunity_analysis.entry(opportunity_type.to_string()).or_insert_with(|| {
            OpportunityStats {
                detected: 0,
                executed: 0,
                profitable_executions: 0,
                total_profit: 0.0,
                avg_execution_time_ms: 0.0,
            }
        });
        
        opp_stats.detected += 1;
        
        if executed {
            opp_stats.executed += 1;
            opp_stats.total_profit += profit;
            
            if profitable {
                opp_stats.profitable_executions += 1;
            }
        }
        
        // Update average execution time
        let total_executions = opp_stats.executed.max(1) as f64;
        opp_stats.avg_execution_time_ms = ((opp_stats.avg_execution_time_ms * (total_executions - 1.0)) + execution_time_ms) / total_executions;
    }

    pub fn get_performance_metrics(&self) -> Value {
        let elapsed_time = Self::current_timestamp() - self.start_time;
        let hours_running = elapsed_time as f64 / 3600.0;
        
        serde_json::json!({
            "total_profit_sol": self.total_profit,
            "total_transactions": self.total_transactions,
            "successful_transactions": self.successful_transactions,
            "failed_transactions": self.failed_transactions,
            "success_rate": if self.total_transactions > 0 { 
                self.successful_transactions as f64 / self.total_transactions as f64 
            } else { 0.0 },
            "avg_profit_per_successful": self.avg_profit_per_successful,
            "total_fees_paid": self.total_fees_paid,
            "profit_per_hour": if hours_running > 0.0 { 
                self.total_profit / hours_running 
            } else { 0.0 },
            "hours_running": hours_running,
            "strategy_performance": self.strategy_performance,
            "opportunity_analysis": self.opportunity_analysis
        })
    }

    pub fn print_summary(&self) {
        let metrics = self.get_performance_metrics();
        Logger::status_update(&format!("Analytics Summary: {:?}", metrics));
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}