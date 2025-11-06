use std::sync::Arc;
use serde_json::Value;
use crate::logging::Logger;
use crate::rpc::rpc_manager::RpcManager;

#[derive(Debug, Clone)]
pub struct FeeCalculator {
    rpc_manager: Arc<RpcManager>,
    base_fee: f64,
    jito_tip: f64,
    dynamic_fee_multiplier: f64,
}

#[derive(Debug, Clone)]
pub struct FeeEstimation {
    pub transaction_fee: f64,
    pub jito_tip: f64,
    pub priority_fee: f64,
    pub total_execution_cost: f64,
    pub compute_unit_price: u64,
    pub compute_units_consumed: u64,
}

impl FeeCalculator {
    pub async fn new(rpc_manager: Arc<RpcManager>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            rpc_manager,
            base_fee: 0.001, // Base transaction fee
            jito_tip: 0.001, // Default Jito tip
            dynamic_fee_multiplier: 1.0, // Multiplier that can be adjusted based on network conditions
        })
    }
    
    pub async fn calculate_dynamic_fees(&self, opportunity_value: f64) -> Result<FeeEstimation, Box<dyn std::error::Error + Send + Sync>> {
        Logger::status_update("Calculating dynamic fees based on recent block analysis");
        
        // Get recent prioritization fees from the network
        let recent_fees_data = self.get_recent_prioritization_fees().await?;
        
        // Calculate priority fee based on recent network activity
        let priority_fee = self.calculate_priority_fee(&recent_fees_data, opportunity_value).await?;
        
        // Calculate Jito tip based on current competition level
        let jito_tip = self.calculate_dynamic_jito_tip(&recent_fees_data, opportunity_value).await?;
        
        // Calculate base transaction fee with adjustments
        let transaction_fee = self.calculate_base_transaction_fee(&recent_fees_data).await?;
        
        // Calculate compute units and prices
        let compute_unit_price = self.estimate_compute_unit_price(&recent_fees_data).await?;
        let compute_units_consumed = self.estimate_compute_units_consumed().await?;
        
        let total_execution_cost = transaction_fee + priority_fee + jito_tip;
        
        Ok(FeeEstimation {
            transaction_fee,
            jito_tip,
            priority_fee,
            total_execution_cost,
            compute_unit_price,
            compute_units_consumed,
        })
    }
    
    async fn get_recent_prioritization_fees(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        // Use the RPC manager to get recent prioritization fees
        self.rpc_manager.get_recent_prioritization_fees().await
    }
    
    async fn calculate_priority_fee(&self, fees_data: &Value, opportunity_value: f64) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // Analyze recent fees to determine appropriate priority fee
        let mut fees_list = Vec::new();
        
        if let Some(fees_array) = fees_data["result"].as_array() {
            for fee_entry in fees_array {
                if let Some(prioritization_fee) = fee_entry["prioritizationFee"].as_u64() {
                    fees_list.push(prioritization_fee as f64);
                }
            }
        }
        
        if fees_list.is_empty() {
            // Fallback if no recent fee data available
            return Ok(0.001); // Conservative estimate
        }
        
        // Calculate average fee and adjust based on opportunity value
        let avg_fee: f64 = fees_list.iter().sum::<f64>() / fees_list.len() as f64;
        
        // For higher value opportunities, we may want to pay higher priority fees to ensure inclusion
        let multiplier = if opportunity_value > 1.0 { 1.5 } else if opportunity_value > 0.1 { 1.2 } else { 1.0 };
        
        // Convert from lamports to SOL and apply multiplier
        let priority_fee_sol = (avg_fee / 1_000_000_000.0) * multiplier;
        
        Ok(priority_fee_sol.min(0.01)) // Cap priority fee at 0.01 SOL
    }
    
    async fn calculate_dynamic_jito_tip(&self, fees_data: &Value, opportunity_value: f64) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // Analyze block space utilization and bundle competition to determine optimal tip
        let competition_level = self.assess_bundle_competition(fees_data).await?;
        
        let base_tip = match competition_level {
            CompetitionLevel::Low => 0.0005,
            CompetitionLevel::Medium => 0.001,
            CompetitionLevel::High => 0.002,
            CompetitionLevel::VeryHigh => 0.003,
        };
        
        // Increase tip for higher-value opportunities
        let value_multiplier = if opportunity_value > 1.0 { 1.5 } else if opportunity_value > 0.5 { 1.2 } else { 1.0 };
        
        Ok(base_tip * value_multiplier)
    }
    
    async fn calculate_base_transaction_fee(&self, fees_data: &Value) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // Calculate base transaction fee based on recent network conditions
        // The base fee is typically fixed but can vary based on network congestion
        
        // For now, return a base fee with potential adjustment
        Ok(0.000005) // Base transaction fee in SOL
    }
    
    async fn estimate_compute_unit_price(&self, fees_data: &Value) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        // Estimate the optimal compute unit price based on recent fees
        let mut prices = Vec::new();
        
        if let Some(fees_array) = fees_data["result"].as_array() {
            for fee_entry in fees_array {
                if let Some(prioritization_fee) = fee_entry["prioritizationFee"].as_u64() {
                    prices.push(prioritization_fee);
                }
            }
        }
        
        if prices.is_empty() {
            return Ok(1_000_000); // Conservative default in micro-lamports
        }
        
        // Calculate average and convert to micro-lamports
        let avg_price = prices.iter().sum::<u64>() as f64 / prices.len() as f64;
        
        // Convert to appropriate units for compute budget
        Ok((avg_price.max(100_000.0).min(100_000_000.0)) as u64) // Between 0.1 and 100 micro-lamports
    }
    
    async fn estimate_compute_units_consumed(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        // Estimate compute units consumed based on transaction complexity
        // For MEV transactions, this would depend on the complexity of the DEX operations
        
        // Conservative estimate for complex MEV transactions
        Ok(200_000) // 200k compute units for complex DEX operations
    }
    
    async fn assess_bundle_competition(&self, fees_data: &Value) -> Result<CompetitionLevel, Box<dyn std::error::Error + Send + Sync>> {
        // Assess competition level based on recent prioritization fees
        // Higher fees indicate more competition
        
        let mut fees_list = Vec::new();
        
        if let Some(fees_array) = fees_data["result"].as_array() {
            for fee_entry in fees_array {
                if let Some(prioritization_fee) = fee_entry["prioritizationFee"].as_u64() {
                    fees_list.push(prioritization_fee as f64);
                }
            }
        }
        
        if fees_list.is_empty() {
            return Ok(CompetitionLevel::Low);
        }
        
        let avg_fee = fees_list.iter().sum::<f64>() / fees_list.len() as f64;
        
        // Define competition thresholds in lamports
        if avg_fee > 100_000_000.0 { // > 0.1 SOL equivalent in lamports
            Ok(CompetitionLevel::VeryHigh)
        } else if avg_fee > 50_000_000.0 { // > 0.05 SOL equivalent
            Ok(CompetitionLevel::High)
        } else if avg_fee > 10_000_000.0 { // > 0.01 SOL equivalent
            Ok(CompetitionLevel::Medium)
        } else {
            Ok(CompetitionLevel::Low)
        }
    }
    
    // Method to adjust fee calculations based on success/failure history
    pub async fn adjust_fee_strategy(&mut self, successful_execution: bool, execution_time_ms: u64) {
        if successful_execution {
            // If execution was fast, we might be overpaying - reduce fees slightly
            if execution_time_ms < 500 { // Under 500ms
                self.dynamic_fee_multiplier = (self.dynamic_fee_multiplier * 0.95).max(0.5);
            } else {
                // Execution was normal timing, maintain current multiplier
                self.dynamic_fee_multiplier = self.dynamic_fee_multiplier * 0.99; // Small decrease over time
            }
        } else {
            // If execution failed, we likely need to increase fees
            self.dynamic_fee_multiplier = (self.dynamic_fee_multiplier * 1.1).min(3.0); // Cap at 3x
        }
        
        // Ensure multiplier stays within reasonable bounds
        self.dynamic_fee_multiplier = self.dynamic_fee_multiplier.clamp(0.1, 5.0);
    }
    
    // Method to calculate if expected profit exceeds total costs with safety margin
    pub async fn calculate_profitability_with_fees(
        &self,
        expected_profit: f64,
        opportunity_value: f64
    ) -> Result<ProfitabilityAnalysis, Box<dyn std::error::Error + Send + Sync>> {
        let fee_estimation = self.calculate_dynamic_fees(opportunity_value).await?;
        
        let total_costs = fee_estimation.total_execution_cost;
        let net_profit = expected_profit - total_costs;
        
        // Calculate if opportunity is profitable with safety margin
        let safety_margin = 0.005; // 0.005 SOL safety margin
        let minimum_profitable = total_costs + safety_margin;
        let is_profitable = net_profit > safety_margin;
        
        Ok(ProfitabilityAnalysis {
            expected_profit,
            total_costs,
            net_profit,
            fee_estimation,
            is_profitable,
            minimum_profitable,
            safety_margin,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProfitabilityAnalysis {
    pub expected_profit: f64,
    pub total_costs: f64,
    pub net_profit: f64,
    pub fee_estimation: FeeEstimation,
    pub is_profitable: bool,
    pub minimum_profitable: f64,
    pub safety_margin: f64,
}

enum CompetitionLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}