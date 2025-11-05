use crate::logging::Logger;

#[derive(Debug, Clone)]
pub struct OpportunityAnalysis {
    pub profit: f64,           // Beneficio estimado en SOL
    pub cost: f64,             // Costo estimado en SOL (tarifas + tips)
    pub revenue: f64,          // Ingreso estimado en SOL
    pub is_profitable: bool,   // Si la oportunidad es rentable
    pub min_profit_margin: f64, // Margen de beneficio mínimo requerido
}

impl OpportunityAnalysis {
    pub fn new(profit: f64, cost: f64, min_profit_margin: f64) -> Self {
        let revenue = profit + cost;
        let is_profitable = profit > cost * (1.0 + min_profit_margin);
        
        Self {
            profit,
            cost,
            revenue,
            is_profitable,
            min_profit_margin,
        }
    }
    
    pub fn calculate_from_amounts(initial_amount: f64, final_amount: f64, fees: f64) -> Self {
        let revenue = final_amount;
        let cost = initial_amount + fees;
        let profit = revenue - cost;
        let min_profit_margin = 0.1; // 10% de margen mínimo
        let is_profitable = profit > fees * (1.0 + min_profit_margin);
        
        Logger::status_update(&format!(
            "Analysis: Initial: {:.6} SOL, Final: {:.6} SOL, Fees: {:.6} SOL, Profit: {:.6} SOL, Profitable: {}",
            initial_amount, final_amount, fees, profit, is_profitable
        ));
        
        Self {
            profit,
            cost,
            revenue,
            is_profitable,
            min_profit_margin,
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
        let min_profit_margin = 0.2; // 20% más conservador para evitar pérdidas
        // Para considerar rentable, el beneficio debe ser significativamente mayor que los costos
        let is_profitable = profit > fees * (1.0 + min_profit_margin);
        
        Logger::status_update(&format!(
            "Frontrun Analysis: Target impact: {:.6} SOL, Our profit: {:.6} SOL, Fees: {:.6} SOL, Profitable: {}",
            target_amount, our_expected_profit, fees, is_profitable
        ));
        
        OpportunityAnalysis {
            profit,
            cost,
            revenue,
            is_profitable,
            min_profit_margin,
        }
    }
    
    pub fn should_execute(opportunity: &OpportunityAnalysis) -> bool {
        if opportunity.is_profitable {
            Logger::status_update(&format!(
                "✅ Opportunity is profitable: {:.6} SOL profit (min required: {:.6} SOL)",
                opportunity.profit,
                opportunity.cost * opportunity.min_profit_margin
            ));
            true
        } else {
            Logger::status_update(&format!(
                "❌ Opportunity not profitable: {:.6} SOL profit vs {:.6} SOL cost",
                opportunity.profit,
                opportunity.cost
            ));
            false
        }
    }
}