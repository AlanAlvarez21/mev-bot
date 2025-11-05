use colored::*;

/// Professional CLI logging for the MEV bot
pub struct Logger;

impl Logger {
    pub fn startup(network: &str, strategies: &str) {
        println!("{}", "=".repeat(60).blue());
        println!("{} {}", "Solana MEV Bot".bold().green(), "v0.1.0".dimmed());
        println!("{}", "=".repeat(60).blue());
        println!("{} {}", "Network:".bold().yellow(), network);
        println!("{} {}", "Strategies:".bold().yellow(), strategies);
        println!("{} {}", "Status:".bold().yellow(), "Running".green());
        println!("{}", "=".repeat(60).blue());
    }

    pub fn eth_monitor_start() {
        println!("{} {}", "".cyan(), "Ethereum mempool monitor started".cyan());
    }

    pub fn solana_monitor_start() {
        println!("{} {}", "".cyan(), "Solana mempool monitor started".cyan());
    }

    pub fn opportunity_detected(chain: &str, tx_hash: &str) {
        println!("{} {} {} {}", "⚡".yellow(), "OPPORTUNITY".yellow().bold(), format!("on {}", chain).dimmed(), format!("Tx: {}", tx_hash).dimmed());
    }

    pub fn bundle_sent(chain: &str, success: bool) {
        if success {
            println!("{} {} {}", "".green(), "BUNDLE SENT".green().bold(), format!("on {}", chain).dimmed());
        } else {
            println!("{} {} {}", "❌".red(), "BUNDLE FAILED".red().bold(), format!("on {}", chain).dimmed());
        }
    }

    pub fn error_occurred(error: &str) {
        println!("{} {} {}", "⚠️".red(), "ERROR".red().bold(), error.dimmed());
    }

    pub fn status_update(status: &str) {
        println!("{} {}", "".blue(), status);
    }

    pub fn shutdown() {
        println!();
        println!("{}", "=".repeat(60).blue());
        println!("{} {}", "🛑 MEV Bot".red().bold(), "Shutdown initiated".dimmed());
        println!("{}", "=".repeat(60).blue());
    }
}