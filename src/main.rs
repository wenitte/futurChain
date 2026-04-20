use std::sync::{Arc, Mutex};
use tokio::time::{interval, Duration};
use clap::Parser;
use futurchain::{chain::Chain, crypto::Keypair, mempool::Mempool, rpc, types::hex_address};

#[derive(Parser)]
#[command(name = "futurchain", about = "FuturChain node — Solana-inspired blockchain")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    #[arg(long, default_value_t = 8899)]
    port: u16,

    /// Milliseconds per slot (Solana uses ~400ms)
    #[arg(long, default_value_t = 400)]
    slot_ms: u64,

    #[arg(long, default_value_t = 1000)]
    block_size: usize,

    #[arg(long, default_value_t = 1_000_000_000)]
    genesis_supply: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let keypair   = Keypair::generate();
    let node_addr = keypair.address();
    println!("══════════════════════════════════════════════════════════════════");
    println!("  FuturChain Node");
    println!("  Address : {}", hex_address(&node_addr));
    println!("  RPC     : http://{}:{}", cli.host, cli.port);
    println!("  Slot    : {}ms", cli.slot_ms);
    println!("══════════════════════════════════════════════════════════════════");

    let chain   = Arc::new(Mutex::new(Chain::new()));
    let mempool = Arc::new(Mutex::new(Mempool::new(100_000)));

    {
        let mut c = chain.lock().unwrap();
        c.ledger.airdrop(node_addr, cli.genesis_supply);
        println!("Genesis: {} tokens → {}", cli.genesis_supply, hex_address(&node_addr));
    }

    // Block production loop
    let chain_bg   = chain.clone();
    let mempool_bg = mempool.clone();
    let block_size = cli.block_size;
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(cli.slot_ms));
        loop {
            ticker.tick().await;
            let txs   = mempool_bg.lock().unwrap().drain(block_size);
            let block = chain_bg.lock().unwrap().produce_block(txs, node_addr);
            if block.header.tx_count > 0 || block.header.slot % 10 == 0 {
                println!(
                    "slot {:>6} | txs {:>4} | poh {}…",
                    block.header.slot,
                    block.header.tx_count,
                    hex::encode(&block.hash[..4])
                );
            }
        }
    });

    let addr     = format!("{}:{}", cli.host, cli.port);
    let app      = rpc::router(chain, mempool);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("RPC listening — Ctrl+C to stop\n");
    axum::serve(listener, app).await?;
    Ok(())
}
