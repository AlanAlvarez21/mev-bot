use crate::logging::Logger;

#[derive(Debug, Clone)]
pub struct OpportunityAnalysis {
    pub profit: f64,           // Beneficio estimado en SOL
    pub cost: f64,             // Costo estimado en SOL (tarifas + tips)
    pub revenue: f64,          // Ingreso estimado en SOL
    pub is_profitable: bool,   // Si la oportunidad es rentable
    pub min_profit_margin: f64, // Margen de beneficio mínimo requerido
    pub net_profit: f64,       // Profit neto (profit - cost)
}

impl OpportunityAnalysis {
    pub fn new(profit: f64, cost: f64, min_profit_margin: f64) -> Self {
        let revenue = profit + cost;
        let net_profit = profit - cost;
        let is_profitable = net_profit > cost * min_profit_margin; // Changed condition to be more conservative
        
        Self {
            profit,
            cost,
            revenue,
            is_profitable,
            min_profit_margin,
            net_profit,
        }
    }
    
    pub fn calculate_from_amounts(initial_amount: f64, final_amount: f64, fees: f64) -> Self {
        let revenue = final_amount;
        let cost = fees;  // Fixed: cost should just be fees for MEV transactions
        let profit = final_amount - initial_amount; // Actual profit calculation
        let net_profit = profit - cost;
        let min_profit_margin = 0.1; // 10% de margen mínimo
        
        Logger::status_update(&format!(
            "Analysis: Initial: {:.6} SOL, Final: {:.6} SOL, Fees: {:.6} SOL, Raw Profit: {:.6} SOL, Net Profit: {:.6} SOL, Profitable: {}",
            initial_amount, final_amount, fees, profit, net_profit, net_profit > 0.001  // Require minimum profit threshold
        ));
        
        // More conservative profitability check
        let is_profitable = net_profit > 0.001; // Require at least 0.001 SOL net profit to be profitable
        
        Self {
            profit,
            cost,
            revenue,
            is_profitable,
            min_profit_margin,
            net_profit,
        }
    }
}

pub struct ProfitabilityCalculator;

impl ProfitabilityCalculator {
    pub fn analyze_arbitrage(initial_amount: f64, final_amount: f64, fees: f64) -> OpportunityAnalysis {
        OpportunityAnalysis::calculate_from_amounts(initial_amount, final_amount, fees)
    }
    
    pub fn analyze_swap(initial_amount: f64, expected_return: f64, fees: f64) -> OpportunityAnalysis {
        OpportunityAnalysis::calculate_from_amounts(initial_amount, expected_return, fees)
    }
    
    pub fn analyze_frontrun(
        target_amount: f64,      // Cantidad que el target va a ganar/perder
        our_expected_profit: f64, // Nuestro beneficio esperado
        fees: f64                 // Tarifas totales (mias + tips)
    ) -> OpportunityAnalysis {
        // En frontrun, nuestro beneficio viene de aprovechar el efecto de la transacción objetivo
        let profit = our_expected_profit;
        let cost = fees;
        // Revenue should be the total amount received, which is profit + initial capital invested
        // But in MEV, the revenue is simply the profit if any (this is conceptually complex)
        let revenue = profit.max(0.0); // We don't consider negative profits as negative revenue
        let net_profit = profit - cost;
        let min_profit_margin = 0.10; // Set to 10% to be more conservative
        // For frontrun, we need positive net profit to be considered profitable
        let is_profitable = net_profit > 0.001 && profit > 0.0; // Require minimum profit after fees AND positive profit estimate from real analysis
        
        Logger::status_update(&format!(
            "Frontrun Analysis: Target impact: {:.6} SOL, Our profit: {:.6} SOL, Fees: {:.6} SOL, Net profit: {:.6} SOL, Profitable: {}",
            target_amount, our_expected_profit, fees, net_profit, is_profitable
        ));
        
        OpportunityAnalysis {
            profit,
            cost,
            revenue,
            is_profitable,
            min_profit_margin: min_profit_margin,
            net_profit,
        }
    }
    
    pub fn should_execute(opportunity: &OpportunityAnalysis) -> bool {
        // More conservative check: ensure we have positive net profit and positive expected profit
        let is_really_profitable = opportunity.is_profitable && opportunity.net_profit > 0.001 && opportunity.profit > 0.0;
        
        if is_really_profitable {
            Logger::status_update(&format!(
                "✅ Opportunity is profitable: {:.6} SOL net profit (min required: {:.6} SOL), expected profit: {:.6} SOL",
                opportunity.net_profit,
                0.001,  // Show minimum threshold
                opportunity.profit
            ));
            true
        } else {
            Logger::status_update(&format!(
                "❌ Opportunity not profitable: {:.6} SOL net profit vs {:.6} SOL minimum, expected profit: {:.6} SOL",
                opportunity.net_profit,
                0.001,  // Show minimum threshold
                opportunity.profit
            ));
            false
        }
    }
}