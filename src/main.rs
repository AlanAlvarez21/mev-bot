use dotenv::dotenv;
use std::env;
use anyhow::Result;
use tokio;
use colored::Colorize;

use rust_mev_hybrid_bot::config::Network;
use rust_mev_hybrid_bot::logging::Logger;
use rust_mev_hybrid_bot::mempool::solana::SolanaMempool;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    let network_env = env::var("NETWORK").unwrap_or_else(|_| "devnet".to_string()).to_lowercase();
    let network = match network_env.as_str() {
        "mainnet" => Network::Mainnet,
        "testnet" => Network::Testnet,
        "devnet" => Network::Devnet,
        _ => Network::Devnet, // Default to devnet
    };
    
    let network_str = match network {
        Network::Mainnet => "MAINNET",
        Network::Testnet => "TESTNET", 
        Network::Devnet => "DEVNET",
    };
    
    let strategy = env::var("STRATEGY").unwrap_or_else(|_| "frontrun".to_string());
    println!("Debug: Strategy value read from env: {}", strategy);  // Debug line

    Logger::startup(network_str, &strategy);

    // Solana thread - now the only network we support
    if strategy.contains("snipe") || strategy.contains("frontrun") || strategy.contains("sandwich") || strategy.contains("arbitrage") {
        println!("Debug: Starting Solana mempool...");
        let sol_mempool = SolanaMempool::new(&network);
        Logger::solana_monitor_start();
        tokio::spawn(async move { sol_mempool.start().await });
    } else {
        println!("Debug: No Solana strategies enabled");
    }

    // Espera indefinida (bot corre forever)
    println!("{} Press Ctrl+C to stop", "🎬".cyan());
    tokio::signal::ctrl_c().await?;
    Logger::shutdown();
    Ok(())
}