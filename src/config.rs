#[derive(Clone, Debug)]
pub enum Network {
    Mainnet,
    Testnet,
    Devnet,
}

impl Network {
    pub fn rpc_url_sol(&self) -> String {
        match self {
            Network::Testnet => std::env::var("SOL_RPC_URL").unwrap_or_else(|_| "https://api.devnet.solana.com".to_string()),
            Network::Devnet => std::env::var("SOL_RPC_URL").unwrap_or_else(|_| "https://api.devnet.solana.com".to_string()),
            Network::Mainnet => {
                let api_key = std::env::var("HELIUS_API_KEY").unwrap_or_else(|_| "TU_KEY".to_string());
                if api_key == "TU_KEY" {
                    std::env::var("SOL_RPC_URL").unwrap_or_else(|_| format!("https://mainnet.helius-rpc.com/?api-key={}", api_key))
                } else {
                    format!("https://mainnet.helius-rpc.com/?api-key={}", api_key)
                }
            }
        }
    }
}