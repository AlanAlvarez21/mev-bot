use colored::*;

fn main() {
    println!("{}", "=".repeat(60).blue());
    println!("{} {}", "🤖 MEV Bot".bold().green(), "v0.1.0".dimmed());
    println!("{}", "=".repeat(60).blue());
    println!("{} {}", "Network:".bold().yellow(), "TESTNET");
    println!("{} {}", "Strategies:".bold().yellow(), "sandwich,arbitrage");
    println!("{} {}", "Status:".bold().yellow(), "Running".green());
    println!("{}", "=".repeat(60).blue());
    
    println!("{} {}", "🔗".cyan(), "Ethereum mempool monitor started".cyan());
    
    println!("{} {} {}", "⚡".yellow(), "OPPORTUNITY".yellow().bold(), "on Ethereum".dimmed());
    println!("{} {}", "📊".blue(), "Ethereum mempool monitoring active");
    
    println!("{} {} {}", "📦".green(), "BUNDLE SENT".green().bold(), "on Ethereum".dimmed());
    
    println!();
    println!("{}", "=".repeat(60).blue());
    println!("{} {}", "🛑 MEV Bot".red().bold(), "Shutdown initiated".dimmed());
    println!("{}", "=".repeat(60).blue());
}