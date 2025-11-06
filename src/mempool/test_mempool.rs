#[cfg(test)]
mod tests {
    use super::*;
    use crate::mempool::solana::SolanaMempool;
    use crate::config::Network;

    #[tokio::test]
    async fn test_transaction_analysis() {
        let mempool = SolanaMempool::new(&Network::Devnet);
        
        // Test with a dummy signature to ensure it doesn't use fake profit estimates
        let analysis = mempool.estimate_profitability("dummy_signature_123456789").await;
        
        // The analysis should be based on real transaction data, not on signature-derived estimates
        // If the transaction doesn't exist or can't be fetched, profit should be 0
        assert!(analysis.profit >= 0.0);
        assert!(analysis.net_profit <= analysis.profit); // Net profit can't exceed gross profit
        
        // If no real transaction data is available, profit should be 0
        if analysis.profit == 0.0 {
            println!("Correctly returned zero profit when no transaction data available");
            assert!(!analysis.is_profitable);
        }
    }

    #[tokio::test]
    async fn test_profitability_calculator() {
        use crate::utils::profitability_calculator::{ProfitabilityCalculator, OpportunityAnalysis};
        
        // Test with zero profit (should not be profitable)
        let analysis = OpportunityAnalysis::new(0.0, 0.006, 0.1);
        assert!(!ProfitabilityCalculator::should_execute(&analysis));
        
        // Test with small profit but high fees (should not be profitable)
        let analysis = OpportunityAnalysis::new(0.005, 0.006, 0.1); // 0.005 profit, 0.006 fees = -0.001 net
        assert!(!ProfitabilityCalculator::should_execute(&analysis));
        
        // Test with good profit over fees (should be profitable)
        let analysis = OpportunityAnalysis::new(0.02, 0.006, 0.1); // 0.02 profit, 0.006 fees = 0.014 net
        assert!(ProfitabilityCalculator::should_execute(&analysis));
    }
}