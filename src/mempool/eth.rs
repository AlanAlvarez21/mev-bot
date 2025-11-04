use ethers::providers::{Middleware, Provider, Ws, StreamExt};
use std::sync::Arc;
use crate::config::Network;
use crate::executor::send_sandwich_bundle;
use crate::logging::Logger;

pub struct EthMempool {
    provider: Arc<Provider<Ws>>,
}

impl EthMempool {
    pub async fn new(network: &Network) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let http_url = network.rpc_url_eth();
        // For WebSocket, we try to construct it from HTTP URL, but use environment var if available
        let ws_url = std::env::var("ETH_WS_URL")
            .unwrap_or_else(|_| http_url.replace("https://", "wss://"));
        let ws = Ws::connect(ws_url).await?;
        let provider = Arc::new(Provider::new(ws));

        Ok(Self { provider })
    }

    pub async fn start(&self) {
        let mut stream = self.provider.subscribe_pending_txs().await.expect("Stream fallido");
        Logger::status_update("Ethereum mempool monitoring active");
        while let Some(tx_hash) = stream.next().await {
            if let Ok(Some(tx)) = self.provider.get_transaction(tx_hash).await {
                if crate::strategy::strategy::is_profitable_sandwich(&tx).await {
                    Logger::opportunity_detected("Ethereum", &format!("{:?}", tx_hash));
                    match send_sandwich_bundle(&self.provider, &tx).await {
                        Ok(_) => Logger::bundle_sent("Ethereum", true),
                        Err(e) => {
                            Logger::error_occurred(&format!("Bundle failed: {}", e));
                        }
                    }
                }
            }
        }
    }
}