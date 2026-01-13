//! Fast mode blockchain binary - DirectCommit consensus for maximum throughput
//!
//! This binary runs the blockchain in DirectCommit mode, which bypasses
//! HotStuff consensus for ultra-high performance single-validator operation.

use atomiq::{
    config::AtomiqConfig, BlockchainFactory, ConsensusMode, Transaction,
};
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::time::{interval, Duration, Instant};

#[derive(Parser)]
#[command(name = "atomiq-fast")]
#[command(about = "High-performance DirectCommit blockchain", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run blockchain in fast mode with continuous transaction generation
    Run {
        /// Target transactions per second
        #[arg(short = 'r', long, default_value_t = 10000)]
        target_tps: u64,
        
        /// Block production interval in milliseconds
        #[arg(short = 'i', long, default_value_t = 10)]
        block_interval_ms: u64,
        
        /// Maximum transactions per block
        #[arg(short = 'x', long, default_value_t = 10000)]
        max_tx_per_block: usize,
        
        /// Run duration in seconds (0 = infinite)
        #[arg(short = 'd', long, default_value_t = 0)]
        duration_secs: u64,
    },
    
    /// Benchmark mode with detailed metrics
    Benchmark {
        /// Total transactions to process
        #[arg(short = 't', long, default_value_t = 100000)]
        total_transactions: u64,
        
        /// Target TPS
        #[arg(short = 'r', long, default_value_t = 50000)]
        target_tps: u64,
        
        /// Block interval in milliseconds
        #[arg(short = 'i', long, default_value_t = 10)]
        block_interval_ms: u64,
    },
    
    /// Test mode with verification
    Test {
        /// Number of transactions to test
        #[arg(short = 't', long, default_value_t = 1000)]
        transaction_count: u64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Run {
            target_tps,
            block_interval_ms,
            max_tx_per_block,
            duration_secs,
        } => {
            run_mode(target_tps, block_interval_ms, max_tx_per_block, duration_secs).await?;
        }
        Commands::Benchmark {
            total_transactions,
            target_tps,
            block_interval_ms,
        } => {
            benchmark_mode(total_transactions, target_tps, block_interval_ms).await?;
        }
        Commands::Test { transaction_count } => {
            test_mode(transaction_count).await?;
        }
    }
    
    Ok(())
}

async fn run_mode(
    target_tps: u64,
    block_interval_ms: u64,
    max_tx_per_block: usize,
    duration_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Atomiq Fast Mode");
    println!("==================");
    println!("Consensus: DirectCommit (No BFT overhead)");
    println!("Target TPS: {}", target_tps);
    println!("Block Interval: {}ms", block_interval_ms);
    println!("Max TX/Block: {}", max_tx_per_block);
    if duration_secs > 0 {
        println!("Duration: {}s", duration_secs);
    } else {
        println!("Duration: Infinite (Ctrl+C to stop)");
    }
    println!();

    // Create config
    let mut config = AtomiqConfig::production();
    config.consensus.mode = ConsensusMode::DirectCommit;
    config.consensus.direct_commit_interval_ms = block_interval_ms;
    config.blockchain.max_transactions_per_block = max_tx_per_block;
    config.blockchain.batch_size_threshold = 10_000; // Larger batches for high throughput

    // Create blockchain
    let (app, _handle) = BlockchainFactory::create_blockchain(config).await?;
    
    // Wait for initialization
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Start transaction generator
    let interval_micros = 1_000_000 / target_tps;
    let mut tx_interval = interval(Duration::from_micros(interval_micros));
    let start_time = Instant::now();
    
    let mut tx_count = 0u64;
    let mut stats_interval = interval(Duration::from_secs(5));
    
    loop {
        tokio::select! {
            _ = tx_interval.tick() => {
                let tx = Transaction {
                    id: tx_count,
                    sender: [1u8; 32],
                    data: format!("tx_{}", tx_count).into_bytes(),
                    timestamp: 0,
                    nonce: tx_count,
                };
                
                if let Err(e) = app.submit_transaction(tx) {
                    eprintln!("Failed to submit transaction: {}", e);
                }
                
                tx_count += 1;
                
                if duration_secs > 0 && tx_count >= target_tps * duration_secs {
                    break;
                }
            }
            
            _ = stats_interval.tick() => {
                let metrics = app.get_metrics();
                let elapsed = start_time.elapsed().as_secs_f64();
                let actual_tps = metrics.total_transactions as f64 / elapsed;
                
                println!("📊 Stats | Submitted: {} | Processed: {} | Blocks: {} | TPS: {:.0} | Pending: {}",
                    tx_count,
                    metrics.total_transactions,
                    metrics.total_blocks,
                    actual_tps,
                    metrics.pending_transactions
                );
            }
        }
    }
    
    // Final stats
    let elapsed = start_time.elapsed();
    let metrics = app.get_metrics();
    let actual_tps = metrics.total_transactions as f64 / elapsed.as_secs_f64();
    
    println!("\n✅ Run Complete");
    println!("Duration: {:.2}s", elapsed.as_secs_f64());
    println!("Transactions: {}", metrics.total_transactions);
    println!("Blocks: {}", metrics.total_blocks);
    println!("Average TPS: {:.0}", actual_tps);
    println!("Avg TX/Block: {:.1}", metrics.total_transactions as f64 / metrics.total_blocks as f64);
    
    Ok(())
}

async fn benchmark_mode(
    total_transactions: u64,
    target_tps: u64,
    block_interval_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔥 Atomiq Fast Mode Benchmark");
    println!("============================");
    println!("Total TX: {}", total_transactions);
    println!("Target TPS: {}", target_tps);
    println!("Block Interval: {}ms\n", block_interval_ms);

    // Create config
    let mut config = AtomiqConfig::default();
    config.consensus.mode = ConsensusMode::DirectCommit;
    config.consensus.direct_commit_interval_ms = block_interval_ms;
    config.blockchain.max_transactions_per_block = 50_000;
    config.blockchain.batch_size_threshold = 10_000;
    config.storage.clear_on_start = true;

    // Create blockchain
    let (app, _handle) = BlockchainFactory::create_blockchain(config).await?;
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("📤 Submitting transactions...");
    let submit_start = Instant::now();
    
    // Submit all transactions as fast as possible
    for i in 0..total_transactions {
        let tx = Transaction {
            id: i,
            sender: [(i % 256) as u8; 32],
            data: format!("benchmark_tx_{}", i).into_bytes(),
            timestamp: 0,
            nonce: i,
        };
        
        app.submit_transaction(tx)?;
        
        if i > 0 && i % 10000 == 0 {
            print!("\r  Submitted: {} / {}", i, total_transactions);
        }
    }
    
    let submit_duration = submit_start.elapsed();
    let submit_tps = total_transactions as f64 / submit_duration.as_secs_f64();
    
    println!("\n✅ All transactions submitted in {:.2}s ({:.0} TPS)", 
        submit_duration.as_secs_f64(), submit_tps);
    
    println!("⏳ Waiting for processing...");
    
    // Wait for all transactions to be processed
    let mut last_count = 0u64;
    let mut stable_iterations = 0;
    
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let metrics = app.get_metrics();
        
        if metrics.total_transactions >= total_transactions {
            println!("✅ All transactions processed!");
            break;
        }
        
        if metrics.total_transactions == last_count {
            stable_iterations += 1;
            if stable_iterations > 10 {
                println!("⚠️  Processing stalled at {}/{}", metrics.total_transactions, total_transactions);
                break;
            }
        } else {
            stable_iterations = 0;
        }
        
        last_count = metrics.total_transactions;
        print!("\r  Processed: {} / {} ({} blocks)", 
            metrics.total_transactions, total_transactions, metrics.total_blocks);
    }
    
    let total_duration = submit_start.elapsed();
    let metrics = app.get_metrics();
    let processing_tps = metrics.total_transactions as f64 / total_duration.as_secs_f64();
    
    println!("\n\n📊 Final Results");
    println!("===============");
    println!("Total Duration: {:.2}s", total_duration.as_secs_f64());
    println!("Transactions: {}", metrics.total_transactions);
    println!("Blocks: {}", metrics.total_blocks);
    println!("Submission TPS: {:.0}", submit_tps);
    println!("Processing TPS: {:.0}", processing_tps);
    println!("Avg TX/Block: {:.1}", metrics.total_transactions as f64 / metrics.total_blocks as f64);
    println!("State Size: {:.2} MB", metrics.state_utilization_mb());
    
    Ok(())
}

async fn test_mode(transaction_count: u64) -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Atomiq Fast Mode Test");
    println!("========================");
    println!("Testing with {} transactions\n", transaction_count);

    let mut config = AtomiqConfig::default();
    config.consensus.mode = ConsensusMode::DirectCommit;
    config.consensus.direct_commit_interval_ms = 10;
    config.storage.clear_on_start = true;

    let (app, _handle) = BlockchainFactory::create_blockchain(config).await?;
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Submit transactions
    for i in 0..transaction_count {
        let tx = Transaction {
            id: i,
            sender: [1u8; 32],
            data: format!("test_{}", i).into_bytes(),
            timestamp: 0,
            nonce: i,
        };
        app.submit_transaction(tx)?;
    }
    
    println!("✅ Submitted {} transactions", transaction_count);
    
    // Wait for processing
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    let metrics = app.get_metrics();
    println!("\n📊 Results:");
    println!("  Processed: {}/{}", metrics.total_transactions, transaction_count);
    println!("  Blocks: {}", metrics.total_blocks);
    println!("  Pending: {}", metrics.pending_transactions);
    
    if metrics.total_transactions == transaction_count {
        println!("\n✅ TEST PASSED");
    } else {
        println!("\n❌ TEST FAILED - Not all transactions processed");
    }
    
    Ok(())
}
