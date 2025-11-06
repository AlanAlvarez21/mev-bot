use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::Value;
use crate::logging::Logger;
use crate::utils::enhanced_transaction_simulator::{OpportunityDetails, OpportunityType};

#[derive(Debug, Clone)]
pub struct ConfidenceFactors {
    pub pool_size_factor: f64,
    pub slippage_factor: f64,
    pub price_impact_factor: f64,
    pub simulation_success_factor: f64,
    pub recent_block_variance_factor: f64,
    pub sender_history_factor: f64,
    pub transaction_value_factor: f64,
}

#[derive(Debug, Clone)]
pub struct ConfidenceScore {
    pub score: f64,           // 0.0 to 1.0
    pub factors: ConfidenceFactors,
    pub reason: String,       // Reason for the score
    pub is_reliable: bool,    // Whether score is based on sufficient data
}

#[derive(Debug, Clone)]
pub struct OpportunityFilteringResult {
    pub should_execute: bool,
    pub confidence_score: ConfidenceScore,
    pub filtered_reason: Option<String>,
}

pub struct FalsePositiveReducer {
    min_confidence_threshold: f64,
    slippage_threshold: f64,  // 3% threshold
    pool_depth_multiplier: f64,
    spam_sender_cache: Arc<RwLock<HashMap<String, SenderHistory>>>,
    opportunity_history: Arc<RwLock<HashMap<String, Vec<HistoricalResult>>>>,
}

#[derive(Debug, Clone)]
struct SenderHistory {
    transaction_count: u32,
    total_value: f64,
    success_rate: f64,
    last_seen: std::time::SystemTime,
}

#[derive(Debug, Clone)]
struct HistoricalResult {
    timestamp: std::time::SystemTime,
    profit: f64,
    success: bool,
}

impl FalsePositiveReducer {
    pub fn new() -> Self {
        Self {
            min_confidence_threshold: 0.85, // 85% confidence required
            slippage_threshold: 0.03,       // 3% of potential profit
            pool_depth_multiplier: 10.0,    // Require 10x pool depth
            spam_sender_cache: Arc::new(RwLock::new(HashMap::new())),
            opportunity_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn evaluate_opportunity(
        &self, 
        opportunity: &OpportunityDetails,
        simulation_results: &[crate::utils::enhanced_transaction_simulator::SimulationResult]
    ) -> OpportunityFilteringResult {
        Logger::status_update("Evaluating opportunity to reduce false positives");
        
        // Calculate comprehensive confidence score
        let confidence_score = self.calculate_confidence_score(opportunity, simulation_results).await;
        
        // Apply various filters
        let slippage_check = self.check_slippage_threshold(opportunity, &confidence_score).await;
        let pool_depth_check = self.check_pool_depth_sufficiency(opportunity).await;
        let spam_check = self.detect_spam_transaction(opportunity).await;
        let value_threshold_check = self.check_value_threshold(opportunity).await;
        
        // Overall decision
        let mut should_execute = true;
        let mut filtered_reasons = Vec::new();
        
        if confidence_score.score < self.min_confidence_threshold {
            should_execute = false;
            filtered_reasons.push(format!(
                "Confidence score {:.2}% below threshold {:.2}%", 
                confidence_score.score * 100.0, 
                self.min_confidence_threshold * 104.0
            ));
        }
        
        if !slippage_check {
            should_execute = false;
            filtered_reasons.push("Slippage exceeds acceptable threshold".to_string());
        }
        
        if !pool_depth_check {
            should_execute = false;
            filtered_reasons.push("Insufficient pool depth for trade size".to_string());
        }
        
        if !spam_check {
            should_execute = false;
            filtered_reasons.push("Transaction detected as potential spam/test".to_string());
        }
        
        if !value_threshold_check {
            should_execute = false;
            filtered_reasons.push("Opportunity value below minimum threshold".to_string());
        }
        
        OpportunityFilteringResult {
            should_execute,
            confidence_score,
            filtered_reason: if !filtered_reasons.is_empty() { 
                Some(filtered_reasons.join(", ")) 
            } else { 
                None 
            },
        }
    }
    
    async fn calculate_confidence_score(
        &self,
        opportunity: &OpportunityDetails,
        simulation_results: &[crate::utils::enhanced_transaction_simulator::SimulationResult]
    ) -> ConfidenceScore {
        // Calculate all confidence factors
        let pool_size_factor = self.calculate_pool_size_factor(opportunity).await;
        let slippage_factor = self.calculate_slippage_factor(opportunity).await;
        let price_impact_factor = self.calculate_price_impact_factor(opportunity).await;
        let simulation_success_factor = self.calculate_simulation_success_factor(simulation_results).await;
        let recent_block_variance_factor = self.calculate_recent_block_variance_factor().await;
        let sender_history_factor = self.calculate_sender_history_factor(opportunity).await;
        let transaction_value_factor = self.calculate_transaction_value_factor(opportunity).await;
        
        // Combine factors with weights
        let weighted_score = 
            pool_size_factor * 0.15 +      // 15% weight
            slippage_factor * 0.20 +      // 20% weight - high importance
            price_impact_factor * 0.15 +  // 15% weight
            simulation_success_factor * 0.20 + // 20% weight - high importance
            recent_block_variance_factor * 0.10 + // 10% weight
            sender_history_factor * 0.10 + // 10% weight
            transaction_value_factor * 0.10; // 10% weight
        
        // Ensure score is between 0 and 1
        let final_score = weighted_score.min(1.0).max(0.0);
        
        let factors = ConfidenceFactors {
            pool_size_factor,
            slippage_factor,
            price_impact_factor,
            simulation_success_factor,
            recent_block_variance_factor,
            sender_history_factor,
            transaction_value_factor,
        };
        
        let reason = if final_score >= self.min_confidence_threshold {
            "Opportunity meets all confidence criteria".to_string()
        } else {
            "Opportunity does not meet minimum confidence threshold".to_string()
        };
        
        ConfidenceScore {
            score: final_score,
            factors,
            reason,
            is_reliable: simulation_results.len() > 0, // Has simulation data
        }
    }
    
    async fn calculate_pool_size_factor(&self, opportunity: &OpportunityDetails) -> f64 {
        // Calculate factor based on pool size relative to trade size
        // Larger pools relative to trade size = higher confidence
        
        let pool_size = self.estimate_pool_size(&opportunity.token_a, &opportunity.token_b).await;
        let trade_size = opportunity.trade_size as f64;
        
        if pool_size == 0.0 {
            return 0.1; // Very low confidence if no pool data
        }
        
        let pool_to_trade_ratio = pool_size / trade_size;
        
        // Return higher score for larger pool relative to trade size
        // Cap at 1.0: if pool is at least 50x trade size, max score
        (pool_to_trade_ratio / 50.0).min(1.0)
    }
    
    async fn calculate_slippage_factor(&self, opportunity: &OpportunityDetails) -> f64 {
        // Calculate factor based on expected slippage
        // Lower slippage = higher confidence
        
        let expected_slippage = self.estimate_slippage(opportunity).await;
        let max_acceptable_slippage = opportunity.estimated_profit * self.slippage_threshold;
        
        if expected_slippage == 0.0 {
            return 1.0; // No slippage = perfect
        }
        
        if expected_slippage > max_acceptable_slippage {
            return 0.1; // Very low confidence if slippage is too high
        }
        
        // Calculate factor: higher confidence when slippage is much less than threshold
        1.0 - (expected_slippage / max_acceptable_slippage)
    }
    
    async fn calculate_price_impact_factor(&self, opportunity: &OpportunityDetails) -> f64 {
        // Calculate factor based on expected price impact
        // Lower price impact = higher confidence
        
        let expected_price_impact = self.estimate_price_impact(opportunity).await;
        let max_acceptable_impact = opportunity.estimated_profit * 0.05; // 5% of profit
        
        if expected_price_impact == 0.0 {
            return 1.0;
        }
        
        if expected_price_impact > max_acceptable_impact {
            return 0.2; // Low confidence if price impact is too high
        }
        
        1.0 - (expected_price_impact / max_acceptable_impact).min(1.0)
    }
    
    async fn calculate_simulation_success_factor(&self, simulation_results: &[crate::utils::enhanced_transaction_simulator::SimulationResult]) -> f64 {
        // Calculate factor based on simulation results
        // Higher success rate in simulations = higher confidence
        
        if simulation_results.is_empty() {
            return 0.1; // Low confidence without simulation data
        }
        
        let valid_results: Vec<&crate::utils::enhanced_transaction_simulator::SimulationResult> = 
            simulation_results.iter().filter(|r| r.is_valid).collect();
        
        if valid_results.is_empty() {
            return 0.1; // Very low confidence if no valid simulations
        }
        
        // Calculate average net profit from valid simulations
        let avg_net_profit: f64 = valid_results.iter()
            .map(|r| r.net_profit)
            .sum::<f64>() / valid_results.len() as f64;
        
        // Calculate consistency (variance across results)
        let avg_net_profit_f64 = avg_net_profit;
        let variance: f64 = valid_results.iter()
            .map(|r| (r.net_profit - avg_net_profit_f64).powi(2))
            .sum::<f64>() / valid_results.len() as f64;
        
        // High average profit and low variance = high confidence
        let profit_factor = (avg_net_profit_f64 / 0.01).min(0.5); // Cap profit factor at 0.5
        let consistency_factor = (1.0 - variance.min(1.0)).max(0.0); // Inverse of variance
        
        (profit_factor + consistency_factor).min(1.0)
    }
    
    async fn calculate_recent_block_variance_factor(&self) -> f64 {
        // Calculate factor based on recent block variance
        // Lower variance in similar opportunities = higher confidence
        
        // In a real implementation, this would analyze recent block data
        // For now, return a conservative estimate
        0.8 // Assume moderate confidence from block analysis
    }
    
    async fn calculate_sender_history_factor(&self, opportunity: &OpportunityDetails) -> f64 {
        // Calculate factor based on sender's transaction history
        // In real implementation, this would check if sender is legitimate
        
        // For now, return a default value
        // In practice, you'd check a sender's history of successful transactions
        0.9 // Assume sender is typically legitimate
    }
    
    async fn calculate_transaction_value_factor(&self, opportunity: &OpportunityDetails) -> f64 {
        // Calculate factor based on transaction value
        // Higher value transactions may have different risk profiles
        
        let value_threshold_for_high_confidence = 0.01; // 0.01 SOL threshold
        
        if opportunity.estimated_profit >= value_threshold_for_high_confidence {
            1.0 // High value = high confidence
        } else if opportunity.estimated_profit >= value_threshold_for_high_confidence / 2.0 {
            0.7 // Medium value = medium confidence
        } else {
            0.3 // Low value = low confidence (might be spam)
        }
    }
    
    async fn check_slippage_threshold(
        &self, 
        opportunity: &OpportunityDetails, 
        confidence_score: &ConfidenceScore
    ) -> bool {
        // Check if slippage factor is acceptable
        confidence_score.factors.slippage_factor > 0.5
    }
    
    async fn check_pool_depth_sufficiency(&self, opportunity: &OpportunityDetails) -> bool {
        // Check if pool depth is sufficient for the trade size
        let pool_size = self.estimate_pool_size(&opportunity.token_a, &opportunity.token_b).await;
        let trade_size = opportunity.trade_size as f64;
        
        pool_size >= trade_size * self.pool_depth_multiplier
    }
    
    async fn detect_spam_transaction(&self, opportunity: &OpportunityDetails) -> bool {
        // Check if this looks like a spam/test transaction
        
        // In a real implementation, this would check:
        // - Sender's transaction history
        // - Typical value thresholds
        // - Transaction patterns
        
        // For now, just return false (assume legitimate)
        // In practice, you'd implement more sophisticated spam detection
        false
    }
    
    async fn check_value_threshold(&self, opportunity: &OpportunityDetails) -> bool {
        // Check if opportunity value meets minimum threshold to be worth pursuing
        opportunity.estimated_profit >= 0.001 // Minimum 0.001 SOL
    }
    
    async fn estimate_pool_size(&self, token_a: &str, token_b: &str) -> f64 {
        // Estimate pool size for the token pair
        // In a real implementation, this would query DEX APIs
        
        // Placeholder implementation
        match (token_a, token_b) {
            ("SOL", "USDC") | ("USDC", "SOL") => 10000.0, // Large SOL/USDC pool
            _ => 1000.0, // Smaller pool for other pairs
        }
    }
    
    async fn estimate_slippage(&self, opportunity: &OpportunityDetails) -> f64 {
        // Estimate expected slippage for the trade
        let pool_size = self.estimate_pool_size(&opportunity.token_a, &opportunity.token_b).await;
        let trade_size = opportunity.trade_size as f64;
        
        if pool_size > 0.0 {
            (trade_size / pool_size) * 0.05 // 5% of trade-to-pool ratio as slippage
        } else {
            0.01 // Default if no pool data
        }
    }
    
    async fn estimate_price_impact(&self, opportunity: &OpportunityDetails) -> f64 {
        // Estimate expected price impact
        self.estimate_slippage(opportunity).await * 0.8 // Price impact is typically less than slippage
    }
    
    // Method to record opportunity results for historical analysis
    pub async fn record_opportunity_result(
        &self,
        opportunity_id: &str,
        profit: f64,
        success: bool
    ) {
        let mut history = self.opportunity_history.write().await;
        
        let entry = history.entry(opportunity_id.to_string()).or_insert_with(Vec::new);
        entry.push(HistoricalResult {
            timestamp: std::time::SystemTime::now(),
            profit,
            success,
        });
        
        // Keep only recent results (last 100)
        if entry.len() > 100 {
            entry.drain(0..entry.len() - 100);
        }
    }
    
    // Method to check historical success rate for similar opportunities
    pub async fn get_historical_success_rate(&self, opportunity_type: &OpportunityType) -> f64 {
        let history = self.opportunity_history.read().await;
        
        let relevant_results: Vec<&HistoricalResult> = history
            .values()
            .flatten()
            .filter(|result| {
                // In a real implementation, this would match the opportunity type
                // For now, return all results
                true
            })
            .collect();
        
        if relevant_results.is_empty() {
            return 0.5; // Default 50% if no data
        }
        
        let successful = relevant_results.iter()
            .filter(|result| result.success)
            .count();
        
        successful as f64 / relevant_results.len() as f64
    }
    
    // Method to update the confidence threshold based on recent performance
    pub async fn adjust_confidence_threshold(&mut self, recent_performance: &[bool]) {
        if recent_performance.is_empty() {
            return;
        }
        
        let success_rate: f64 = recent_performance.iter()
            .filter(|&&success| success)
            .count() as f64 / recent_performance.len() as f64;
        
        // Adjust threshold based on success rate
        if success_rate > 0.85 {
            // If success rate is high, we can afford to be more selective
            self.min_confidence_threshold = (self.min_confidence_threshold * 1.05).min(0.95);
        } else if success_rate < 0.7 {
            // If success rate is low, be less selective to catch more opportunities
            self.min_confidence_threshold = (self.min_confidence_threshold * 0.95).max(0.7);
        }
    }
    
    // Method to detect consecutive failures for specific strategy types
    pub async fn check_consecutive_failures(&self, strategy_type: &str) -> u32 {
        // In a real implementation, this would track consecutive failures by strategy type
        // For now, return a placeholder
        0
    }
}

// Additional utilities for false positive detection
pub mod fp_detection_utils {
    use super::*;
    
    #[derive(Debug, Clone)]
    pub struct TransactionPattern {
        pub sender_address: String,
        pub frequency: u32,
        pub total_value: f64,
        pub average_profit: f64,
        pub success_rate: f64,
    }
    
    pub struct PatternAnalyzer {
        patterns: Arc<RwLock<HashMap<String, TransactionPattern>>>,
    }
    
    impl PatternAnalyzer {
        pub fn new() -> Self {
            Self {
                patterns: Arc::new(RwLock::new(HashMap::new())),
            }
        }
        
        pub async fn analyze_pattern(&self, sender: &str, value: f64, profit: f64, success: bool) -> f64 {
            // Analyze if this sender's pattern indicates legitimate MEV activity or spam
            // Return confidence score based on pattern analysis
            
            // For now, return a simple score based on value and success history
            if value < 0.001 {
                // Very low value might indicate test/spam
                if success {
                    0.7 // Still some confidence if it succeeded
                } else {
                    0.2 // Low confidence if low value and failed
                }
            } else {
                // Higher value transactions get more confidence
                0.9
            }
        }
    }
}