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
    
    // NEW ARCHITECTURE: Validate required environment variables
    validate_environment_variables()?;
    
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
    
    let strategy = env::var("STRATEGY").unwrap_or_else(|_| "arbitrage".to_string());
    println!("Debug: Strategy value read from env: {}", strategy);  // Debug line

    Logger::startup(network_str, &strategy);

    // Solana thread - now the only network we support
    if strategy.contains("snipe") || strategy.contains("frontrun") || strategy.contains("sandwich") || strategy.contains("arbitrage") {
        println!("Debug: Starting Solana mempool...");
        let sol_mempool = SolanaMempool::new(&network);
        Logger::solana_monitor_start();
        tokio::spawn(async move {
            sol_mempool.start().await
        });
    } else {
        println!("Debug: No Solana strategies enabled");
    }

    // Espera indefinida (bot corre forever)
    println!("{} Press Ctrl+C to stop", "".cyan());
    tokio::signal::ctrl_c().await?;
    Logger::shutdown();
    Ok(())
}

fn validate_environment_variables() -> Result<()> {
    // NEW ARCHITECTURE: Check that all required environment variables are set
    let required_vars = vec![
        "HELIUS",      // For read/simulation calls
        "JITO_RPC_URL", // For execution
        "JITO_TIP_ACCOUNT", // For Jito tips
        "DRPC",        // Fallback RPC
    ];
    
    for var in required_vars {
        if std::env::var(var).is_err() {
            eprintln!("ERROR: Environment variable {} is not set", var);
            eprintln!("Please check your .env file and ensure all required variables are present");
            std::process::exit(1);
        }
    }
    
    println!("All required environment variables are present");
    Ok(())
}