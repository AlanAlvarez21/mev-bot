use ethers::types::Transaction;

// Placeholder simple: Siempre true en testnet
pub async fn is_profitable_sandwich(_tx: &Transaction) -> bool {
    true  // En prod: Simula con revm::simulate_swap_profit
}

pub fn is_profitable_frontrun(_tx: &str) -> bool {
    true  // En prod: Check amount > threshold
}