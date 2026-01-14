//! Unified Atomiq Blockchain Binary
//!
//! Refactored main binary using the new modular architecture and factory patterns

use atomiq::{
    config::{AtomiqConfig, NetworkMode},
    factory::BlockchainFactory,
    benchmark::{BenchmarkRunner, BenchmarkConfig},
    errors::AtomiqResult,
};
use clap::{Parser, Subcommand};
use std::{path::PathBuf, time::Duration};
use tokio::time::sleep;

/// Atomiq Blockchain CLI
#[derive(Parser)]
#[command(name = "atomiq")]
#[command(about = "High-performance blockchain with HotStuff consensus")]
#[command(version = "2.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Data directory for blockchain storage
    #[arg(short, long, default_value = "./blockchain_data")]
    data_dir: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Run high-performance benchmark
    BenchmarkPerformance {
        /// Target transactions per second
        #[arg(short = 'r', long, default_value = "100000")]
        target_tps: u64,
        
        /// Total transactions to process
        #[arg(short, long, default_value = "100000")]
        total_transactions: usize,
        
        /// Number of concurrent submitters
        #[arg(short, long, default_value = "8")]
        concurrent_submitters: usize,
    },

    /// Run consensus correctness test
    BenchmarkConsensus {
        /// Total transactions to process
        #[arg(short, long, default_value = "1000")]
        total_transactions: usize,
        
        /// Test duration in seconds
        #[arg(short, long, default_value = "30")]
        duration_seconds: u64,
    },

    /// Run single validator blockchain
    SingleValidator {
        /// Maximum transactions per block
        #[arg(short = 'x', long, default_value = "1000")]
        max_tx_per_block: usize,
        
        /// Block time in milliseconds
        #[arg(short, long, default_value = "1000")]
        block_time_ms: u64,
    },

    /// Run throughput test without consensus
    ThroughputTest {
        /// Total transactions to submit
        #[arg(short, long, default_value = "50000")]
        total_transactions: usize,
        
        /// Batch size for submission
        #[arg(short, long, default_value = "1000")]
        batch_size: usize,
    },

    /// Inspect database contents
    InspectDb {
        /// Path to database directory
        #[arg(short, long)]
        db_path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> AtomiqResult<()> {
    let cli = Cli::parse();

    // Initialize logging
    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    // Load configuration
    let mut config = if let Some(config_path) = &cli.config {
        load_config_from_file(config_path)?
    } else {
        AtomiqConfig::default()
    };

    // Override with CLI options
    config.storage.data_directory = cli.data_dir;

    // Execute command
    match cli.command {
        Commands::BenchmarkPerformance { target_tps, total_transactions, concurrent_submitters } => {
            run_performance_benchmark(config, target_tps, total_transactions, concurrent_submitters).await
        }
        Commands::BenchmarkConsensus { total_transactions, duration_seconds } => {
            run_consensus_benchmark(config, total_transactions, duration_seconds).await
        }
        Commands::SingleValidator { max_tx_per_block, block_time_ms } => {
            run_single_validator(config, max_tx_per_block, block_time_ms).await
        }
        Commands::ThroughputTest { total_transactions, batch_size } => {
            run_throughput_test(config, total_transactions, batch_size).await
        }
        Commands::InspectDb { db_path } => {
            inspect_database(db_path.unwrap_or_else(|| PathBuf::from(&config.storage.data_directory))).await
        }
    }
}

async fn run_performance_benchmark(
    mut config: AtomiqConfig,
    target_tps: u64,
    total_transactions: usize,
    concurrent_submitters: usize,
) -> AtomiqResult<()> {
    println!("🚀 Starting High-Performance Benchmark");
    println!("=====================================");

    // Configure for high performance
    config = AtomiqConfig::high_performance();
    config.performance.target_tps = Some(target_tps);

    let (blockchain, _handle) = BlockchainFactory::create_blockchain(config.clone()).await?;

    let benchmark_config = BenchmarkConfig {
        total_transactions,
        concurrent_submitters,
        batch_size: 1000,
        warmup_duration: Duration::from_secs(5),
        test_duration: Duration::from_secs(120),
        enable_progress_reporting: true,
        report_interval: Duration::from_secs(2),
    };

    let runner = BenchmarkRunner::new(blockchain, benchmark_config);
    let results = runner.run_full_benchmark().await?;

    println!("\n🎯 Performance Benchmark Results:");
    println!("Target TPS: {}", target_tps);
    println!("Achieved TPS: {:.0}", results.processing_tps);
    println!("Efficiency: {:.1}%", (results.processing_tps / target_tps as f64) * 100.0);

    Ok(())
}

async fn run_consensus_benchmark(
    mut config: AtomiqConfig,
    total_transactions: usize,
    duration_seconds: u64,
) -> AtomiqResult<()> {
    println!("🏛️ Starting Consensus Correctness Benchmark");
    println!("==========================================");

    // Configure for consensus testing
    config = AtomiqConfig::consensus_testing();

    let (blockchain, _handle) = BlockchainFactory::create_blockchain(config.clone()).await?;

    let benchmark_config = BenchmarkConfig {
        total_transactions,
        concurrent_submitters: 2, // Conservative for correctness
        batch_size: 50,
        warmup_duration: Duration::from_secs(2),
        test_duration: Duration::from_secs(duration_seconds),
        enable_progress_reporting: true,
        report_interval: Duration::from_secs(5),
    };

    let runner = BenchmarkRunner::new(blockchain, benchmark_config);
    let results = runner.run_full_benchmark().await?;

    println!("\n🎯 Consensus Benchmark Results:");
    println!("Blocks created: {}", results.total_blocks_created);
    println!("Avg tx per block: {:.1}", results.average_transactions_per_block);
    println!("Consensus working: {}", results.total_blocks_created > 0);

    Ok(())
}

async fn run_single_validator(
    mut config: AtomiqConfig,
    max_tx_per_block: usize,
    block_time_ms: u64,
) -> AtomiqResult<()> {
    println!("⚡ Starting Single Validator Blockchain");
    println!("=====================================");

    // Configure for single validator
    config.network.mode = NetworkMode::SingleValidator;
    config.blockchain.max_transactions_per_block = max_tx_per_block;
    config.blockchain.max_block_time_ms = block_time_ms;

    let (blockchain, _handle) = BlockchainFactory::create_blockchain(config).await?;

    println!("✅ Single validator blockchain started");
    println!("📊 Max {} tx/block, {}ms block time", max_tx_per_block, block_time_ms);

    // Wait for initialization
    sleep(Duration::from_secs(3)).await;

    // Run a small test
    let benchmark_config = BenchmarkConfig {
        total_transactions: 1000,
        concurrent_submitters: 2,
        batch_size: 100,
        warmup_duration: Duration::from_secs(1),
        test_duration: Duration::from_secs(30),
        enable_progress_reporting: true,
        report_interval: Duration::from_secs(3),
    };

    let runner = BenchmarkRunner::new(blockchain, benchmark_config);
    let _results = runner.run_full_benchmark().await?;

    Ok(())
}

async fn run_throughput_test(
    mut config: AtomiqConfig,
    total_transactions: usize,
    batch_size: usize,
) -> AtomiqResult<()> {
    println!("⚡ Starting Throughput Test (No Consensus)");
    println!("========================================");

    // Configure for mock mode (no consensus)
    config.network.mode = NetworkMode::Mock;

    let (blockchain, _handle) = BlockchainFactory::create_blockchain(config).await?;

    let benchmark_config = BenchmarkConfig {
        total_transactions,
        concurrent_submitters: 4,
        batch_size,
        warmup_duration: Duration::ZERO,
        test_duration: Duration::from_secs(60),
        enable_progress_reporting: true,
        report_interval: Duration::from_secs(2),
    };

    let runner = BenchmarkRunner::new(blockchain, benchmark_config);
    let results = runner.run_throughput_benchmark().await?;

    println!("\n🎯 Throughput Test Results:");
    println!("Submission TPS: {:.0}", results.submission_tps);
    println!("Total time: {:?}", results.total_duration);

    Ok(())
}

async fn inspect_database(db_path: PathBuf) -> AtomiqResult<()> {
    println!("🔍 Inspecting Database: {:?}", db_path);
    println!("========================");

    // For now, just print basic info
    // In a full implementation, this would use the storage module to inspect contents
    if db_path.exists() {
        println!("✅ Database directory exists");
        
        if let Ok(entries) = std::fs::read_dir(&db_path) {
            let count = entries.count();
            println!("📁 Found {} database files", count);
        }
    } else {
        println!("❌ Database directory does not exist");
    }

    Ok(())
}

fn load_config_from_file(path: &PathBuf) -> AtomiqResult<AtomiqConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| atomiq::errors::AtomiqError::Configuration(
            atomiq::errors::ConfigurationError::LoadFailed(e.to_string())
        ))?;
    
    let config: AtomiqConfig = toml::from_str(&content)
        .map_err(|e| atomiq::errors::AtomiqError::Configuration(
            atomiq::errors::ConfigurationError::LoadFailed(e.to_string())
        ))?;
    
    config.validate().map_err(|e| atomiq::errors::AtomiqError::Configuration(
        atomiq::errors::ConfigurationError::ValidationFailed(e.to_string())
    ))?;
    
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use tempfile::tempdir;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert()
    }

    #[tokio::test]
    async fn test_mock_throughput() {
        let db_dir = tempdir().expect("create temp db dir");

        let config = AtomiqConfig {
            network: atomiq::config::NetworkConfig {
                mode: NetworkMode::Mock,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut config = config;
        config.storage.data_directory = db_dir.path().to_string_lossy().into_owned();
        config.storage.clear_on_start = true;

        let result = run_throughput_test(config, 100, 10).await;
        assert!(result.is_ok());
    }
}