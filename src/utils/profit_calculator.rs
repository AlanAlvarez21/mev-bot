use std::collections::HashMap;

pub struct ProfitCalculator {
    pub base_fee: f64,           // Costo base de la transacción
    pub gas_price: f64,          // Precio actual del gas
    pub gas_limit: u64,          // Límite de gas
    pub exchange_rates: HashMap<String, f64>, // Tasas de cambio para diferentes tokens
    pub min_profit_margin: f64,  // Margen mínimo de beneficio
}

impl ProfitCalculator {
    pub fn new() -> Self {
        let mut exchange_rates = HashMap::new();
        // Añadir tasas de cambio básicas
        exchange_rates.insert("SOL".to_string(), 1.0); // 1 SOL
        exchange_rates.insert("USDC".to_string(), 0.999); // Aproximadamente 1 USD
        exchange_rates.insert("USDT".to_string(), 0.999); // Aproximadamente 1 USD
        
        Self {
            base_fee: 0.005, // 0.005 SOL por transacción base
            gas_price: 0.000001, // Precio base del gas
            gas_limit: 200000, // Límite de gas estándar
            exchange_rates,
            min_profit_margin: 0.1, // 10% de margen mínimo
        }
    }

    pub fn calculate_profitability(
        &self,
        estimated_profit: f64,  // Beneficio estimado en SOL
        fees: f64,              // Tarifas totales en SOL
        tip_amount: f64,        // Propina a Jito
    ) -> OpportunityAnalysis {
        let total_costs = fees + tip_amount;
        let net_profit = estimated_profit - total_costs;
        let profit_margin = if estimated_profit > 0.0 {
            net_profit / estimated_profit
        } else {
            0.0
        };

        let is_profitable = net_profit > (estimated_profit * self.min_profit_margin);

        OpportunityAnalysis {
            estimated_profit,
            fees,
            tip_amount,
            total_costs,
            net_profit,
            profit_margin,
            is_profitable,
            min_profit_margin: self.min_profit_margin,
        }
    }

    pub fn calculate_minimal_rentability_for_bundle(&self, bundle_size: usize) -> f64 {
        // Calcular la tarifa mínima necesaria para un bundle
        let base_tx_cost = self.base_fee;
        let bundle_cost = base_tx_cost * bundle_size as f64;
        // Agregar tarifa adicional para compensar la incertidumbre de bundles
        bundle_cost * 1.5 // 50% extra para cubrir la complejidad del bundle
    }

    pub fn estimate_opportunity_profit(&self, transaction_data: &str) -> f64 {
        // En una implementación completa, analizaríamos la transacción para estimar beneficios
        // Por ahora, usamos una estimación básica basada en patrones comunes
        if transaction_data.contains("swap") || transaction_data.contains("Swap") {
            // Si parece una transacción de swap, estimamos un beneficio promedio
            (transaction_data.len() % 1000) as f64 / 10000.0 + 0.01
        } else if transaction_data.contains("arbitrage") {
            // Si parece una operación de arbitraje
            (transaction_data.len() % 1000) as f64 / 5000.0 + 0.02
        } else {
            // Valor por defecto para otras transacciones
            (transaction_data.len() % 1000) as f64 / 20000.0 + 0.001
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpportunityAnalysis {
    pub estimated_profit: f64,
    pub fees: f64,
    pub tip_amount: f64,
    pub total_costs: f64,
    pub net_profit: f64,
    pub profit_margin: f64,
    pub is_profitable: bool,
    pub min_profit_margin: f64,
}