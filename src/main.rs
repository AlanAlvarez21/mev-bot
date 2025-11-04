use dotenv::dotenv;
use std::env;
use anyhow::Result;
use tokio;
use colored::Colorize;

use rust_mev_hybrid_bot::config::Network;
use rust_mev_hybrid_bot::logging::Logger;
use rust_mev_hybrid_bot::mempool::eth::EthMempool;
use rust_mev_hybrid_bot::mempool::solana::SolanaMempool;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    let testnet = env::var("TESTNET").unwrap_or_else(|_| "true".to_string()) == "true";
    let network = if testnet { Network::Testnet } else { Network::Mainnet };
    let network_str = if testnet { "TESTNET" } else { "MAINNET" };
    
    let strategy = env::var("STRATEGY").unwrap_or_else(|_| "sandwich".to_string());

    Logger::startup(network_str, &strategy);

    // Ethereum thread
    if strategy.contains("sandwich") || strategy.contains("arbitrage") {
        let eth_mempool = EthMempool::new(&network).await;
        Logger::eth_monitor_start();
        tokio::spawn(async move { eth_mempool.start().await });
    }

    // Solana thread
    if strategy.contains("snipe") || strategy.contains("frontrun") {
        let sol_mempool = SolanaMempool::new(&network);
        Logger::solana_monitor_start();
        tokio::spawn(async move { sol_mempool.start().await });
    }

    // Espera indefinida (bot corre forever)
    println!("{} Press Ctrl+C to stop", "🎬".cyan());
    tokio::signal::ctrl_c().await?;
    Logger::shutdown();
    Ok(())
}