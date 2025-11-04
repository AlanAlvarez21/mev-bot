# Rust MEV Hybrid Bot

A multi-chain MEV (Maximum Extractable Value) bot written in Rust that operates on both Ethereum and Solana networks. This bot monitors mempools in real-time to identify and execute profitable arbitrage, sandwich, and frontrun trading strategies.

## Features

- **Multi-chain Support**: Operates on both Ethereum and Solana simultaneously
- **Real-time Mempool Monitoring**: Tracks pending transactions via WebSocket connections
- **Multiple MEV Strategies**: Implements sandwich attacks, arbitrage, and frontrunning
- **Privacy-First Execution**: Uses Flashbots (Ethereum) and Jito (Solana) for private transaction bundles
- **Configurable Strategies**: Easily switch between different MEV strategies
- **Testnet Compatible**: Designed for safe testing on testnet environments

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)
- A computer capable of running Rust applications continuously
- (Optional) Access to premium RPC providers for better performance

## Installation

1. Clone the repository:

```bash
git clone https://github.com/yourusername/rust-mev-hybrid-bot.git
cd rust-mev-hybrid-bot
```

2. Install Rust dependencies:

```bash
cargo build
```

## Configuration

1. Create a `.env` file in the project root with the following environment variables:

```env
# Ethereum configuration
ETH_PRIVATE_KEY="your_ethereum_private_key_here"
ETH_RPC_URL="https://mainnet.infura.io/v3/YOUR_INFURA_KEY"  # or use sepolia for testnet

# Solana configuration  
SOLANA_PRIVATE_KEY="your_solana_private_key_here"
SOLANA_RPC_URL="https://mainnet.helius-rpc.com/?api-key=YOUR_HELIUS_KEY"  # or devnet for testnet

# Network selection (true for testnet, false for mainnet)
TESTNET=true

# Strategy selection (comma-separated)
STRATEGY="sandwich,arbitrage,snipe,frontrun"

# Logging level
RUST_LOG=info
```

2. Key configuration notes:
   - Use testnet private keys for testing to avoid financial risk
   - On mainnet, ensure your wallet has sufficient ETH/SOL for transaction fees
   - For testnet, you can generate new keys or use faucets for test tokens

## How to Test

### 1. Setup for Testing

For safe testing, use testnet networks:

- **Ethereum**: Sepolia testnet
- **Solana**: Devnet

### 2. Testnet Configuration

Set up your `.env` for testnet:

```env
TESTNET=true
ETH_PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" # Example Anvil/Ethereum test key
SOLANA_PRIVATE_KEY="5L3Q6fK4J69W41mTsyfxY7vU6EA7Xq5YxZJw1a4M7K7z2L8q9R4P3N1H6V5J2C9K7Z8" # Replace with new test key
RUST_LOG=debug
```

### 3. Running the Bot

To run the bot with specific strategies:

```bash
# Run with sandwich and arbitrage strategies (Ethereum only)
STRATEGY="sandwich,arbitrage" cargo run

# Run with snipe and frontrun strategies (Solana only)
STRATEGY="snipe,frontrun" cargo run

# Run with all strategies
STRATEGY="sandwich,arbitrage,snipe,frontrun" cargo run

# Run with verbose logging
RUST_LOG=debug STRATEGY="sandwich" cargo run
```

### 4. Testing Strategies

#### Ethereum Strategies (Sandwich/Arbitrage):

- The bot will monitor pending transactions on the Ethereum mempool
- On testnet: Look for transactions that simulate profitable DEX swaps
- The bot will attempt to sandwich or arbitrage these transactions using Flashbots
- Monitor logs for "Oportunidad detectada" messages

#### Solana Strategies (Snipe/Frontrun):

- The bot will monitor Raydium and other DEX transactions
- On testnet: Look for transactions on testnet DEX programs
- The bot will attempt to frontrun profitable transactions using Jito
- Monitor logs for "Oportunidad detectada" messages

### 5. Verification Steps

1. **Check logs**: Verify the bot is connecting to both networks
2. **Monitor mempool activity**: Look for "Monitoreando mempool" messages
3. **Test transaction detection**: You can broadcast test transactions to see if the bot detects them
4. **Check bundle submissions**: Verify bundle submission attempts (on testnet only)

### 6. Safe Testing Practices

- **Always start with testnet**: Never run the bot on mainnet until thoroughly tested
- **Use small amounts**: When testing on testnet, use transactions with small amounts
- **Monitor gas costs**: Ensure transaction fees don't exceed potential profits
- **Review strategies**: Understand each strategy before enabling it
- **Simulate profitably**: The current implementation has placeholder profitability checks

## Strategies Explained

### Sandwich Attack

- Front-run a large buy order to increase price
- Back-run after the victim transaction to capture profit
- Works on DEXs like Uniswap, Sushiswap, etc.

### Arbitrage

- Exploit price differences between exchanges
- Execute simultaneous buy/sell transactions
- Capture risk-free profit from price discrepancies

### Frontrun

- Execute profitable transactions ahead of detected transactions
- Intercept DEX swaps that would be profitable
- Capture value from price impact

### Snipe

- Identify and execute specific profitable opportunities
- Target token launches, liquidity additions, etc.

## Building for Production

To build an optimized version:

```bash
cargo build --release
```

The binary will be available at `target/release/rust-mev-hybrid-bot`

## Development

The project is organized as follows:

- `src/mempool/`: Mempool monitoring logic for both chains
- `src/executor/`: Transaction execution and bundle creation
- `src/strategy/`: MEV strategy logic and opportunity detection
- `src/utils/`: Helper functions and utilities

## Risks and Considerations

- **Financial Risk**: MEV strategies carry significant financial risk, especially on mainnet
- **Competition**: Intense competition with other MEV bots
- **Network Congestion**: High gas fees and transaction costs
- **Smart Contract Risk**: Possible contract exploits or changes
- **Regulatory Risk**: MEV activities may face regulatory changes

## License

This project is licensed under the MIT License - see the LICENSE file for details.




  Complete Setup Guide

  Step 1: Prerequisites

1. Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
2. Install Git: brew install git (on macOS) or download from git-scm.com
3. Clone the repository: git clone https://github.com/your-repo/crypto_mev_bot.git
4. Navigate to the project directory: cd crypto_mev_bot

  Step 2: Create Your .env File
  Create a .env file in the project root with the following content:

    1 # For testing on testnet (set to "true")
    2 TESTNET=true
    3
    4 # Ethereum Configuration (Testnet example)
    5 ETH_PRIVATE_KEY="your_sepolia_private_key_here"
    6
    7 # Solana Configuration (Testnet example)
    8 SOLANA_PRIVATE_KEY="your_solana_testnet_private_key_here"
    9
   10 # Logging level
   11 RUST_LOG=info

  Step 3: Get Testnet Keys and Funds

1. For Ethereum Sepolia:

   - Install MetaMask
   - Switch to Sepolia testnet
   - Generate a new account
   - Get test ETH from a faucet
2. For Solana Devnet:

   - Install Phantom wallet
   - Switch to Devnet
   - Generate a new account
   - Get test SOL from faucet

  Step 4: Add Your Private Keys
  IMPORTANT: Use only testnet keys for initial testing!

- In MetaMask: Account options → Export Private Key
- In Phantom: Settings → Export Secret Recovery Phrase (then derive your private key)

  Add these to your .env file as ETH_PRIVATE_KEY and SOLANA_PRIVATE_KEY.

  Step 5: Run the Bot

   1 # Run with specific strategies (e.g., only Ethereum sandwich)
   2 STRATEGY="sandwich" cargo run
   3
   4 # Or run with all strategies
   5 STRATEGY="sandwich,arbitrage,snipe,frontrun" cargo run

  Step 6: Monitor the Output
  Watch the console for:

- "Monitoreando mempool" messages (indicating successful connection)
- "Oportunidad detectada" (when potentially profitable transactions are found)
- Any error messages that might indicate configuration issues

  Remember: Start with testnet keys only. MEV strategies are complex and may result in losses. Understand the risks before using mainnet funds.
