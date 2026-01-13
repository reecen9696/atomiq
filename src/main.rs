//! Atomiq - High Performance Single Node Blockchain
//! 
//! Optimized for maximum TPS measurement with proper validation.

use atomiq::{
    AtomiqApp, BlockchainConfig, Transaction,
    network::MockNetwork, metrics::PerformanceMonitor, storage::OptimizedStorage,
};
use hotstuff_rs::{
    replica::{Configuration, Replica, ReplicaSpec},
    types::{
        crypto_primitives::SigningKey,
        data_types::{ChainID, BufferSize, EpochLength},
        update_sets::{AppStateUpdates, ValidatorSetUpdates},
        validator_set::{ValidatorSet, ValidatorSetState},
    },
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
    fs,
};
use tokio::time::sleep;

struct BenchmarkConfig {
    pub total_transactions: usize,
    pub batch_size: usize,
    pub concurrent_submitters: usize,
    pub blockchain_config: BlockchainConfig,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            total_transactions: 100_000,
            batch_size: 100,
            concurrent_submitters: 4,
            blockchain_config: BlockchainConfig {
                max_transactions_per_block: 5000,
                max_block_time_ms: 5, // Aggressive 5ms target
                enable_state_validation: true, // Enable for real TPS
                batch_size_threshold: 1000,
            },
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("🚀 Starting Lean Blockchain Performance Test");
    println!("============================================");

    // Clean up old data
    let _ = fs::remove_dir_all("./blockchain_data");

    let benchmark_config = BenchmarkConfig::default();
    
    // Start blockchain
    let (blockchain, _replica) = start_blockchain(benchmark_config.blockchain_config.clone()).await?;
    
    // Wait for blockchain to initialize
    sleep(Duration::from_millis(100)).await;
    
    // Run throughput benchmark
    run_throughput_benchmark(&blockchain, &benchmark_config).await?;
    
    Ok(())
}

async fn start_blockchain(config: BlockchainConfig) -> Result<(Arc<AtomiqApp>, hotstuff_rs::replica::Replica<OptimizedStorage>), Box<dyn std::error::Error>> {
    // Create signing key
    let mut rng = rand::rngs::OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();
    
    // Create app
    let app = Arc::new(AtomiqApp::new(config.clone()));
    
    // Create network (mock for single node)
    let network = MockNetwork::new(verifying_key);
    
    // Create storage
    let kv_store = OptimizedStorage::new("./blockchain_data")?;
    
    // Initialize validator set (single validator)
    let mut initial_validator_set = ValidatorSet::new();
    let mut validator_set_updates = ValidatorSetUpdates::new();
    validator_set_updates.insert(verifying_key, hotstuff_rs::types::data_types::Power::new(1));
    initial_validator_set.apply_updates(&validator_set_updates);
    
    let validator_set_state = ValidatorSetState::new(
        initial_validator_set.clone(),
        initial_validator_set,
        None,
        true,
    );
    
    // Initialize replica storage
    Replica::initialize(kv_store.clone(), AppStateUpdates::new(), validator_set_state.clone());
    
    // Configure replica for high performance single validator
    let configuration = Configuration::builder()
        .me(signing_key)
        .chain_id(ChainID::new(1))
        .block_sync_request_limit(100)
        .block_sync_server_advertise_time(Duration::from_secs(1))
        .block_sync_response_timeout(Duration::from_secs(1))
        .block_sync_blacklist_expiry_time(Duration::from_secs(10))
        .block_sync_trigger_min_view_difference(1) // Single validator
        .block_sync_trigger_timeout(Duration::from_secs(5))
        .progress_msg_buffer_capacity(BufferSize::new(10240))
        .epoch_length(EpochLength::new(100))
        .max_view_time(Duration::from_millis(50)) // Increased for better single validator timing
        .log_events(true) // Enable to see what's happening
        .build();

    // Start replica and keep handle alive to maintain consensus
    let replica = ReplicaSpec::builder()
        .app((*app).clone())
        .network(network)
        .kv_store(kv_store)
        .configuration(configuration)
        .build()
        .start();

    // Give replica a moment to initialize
    tokio::time::sleep(Duration::from_millis(100)).await;

    println!("✅ Blockchain started successfully");
    println!("📊 Config: {} max tx/block, {}ms target block time", 
        config.max_transactions_per_block, config.max_block_time_ms);
    
    // Start a task to monitor and help trigger consensus for single validator
    let app_monitor = app.clone();
    tokio::spawn(async move {
        let mut last_tx_count = 0;
        let mut stale_count = 0;
        
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let current_tx_count = app_monitor.transaction_counter().load(std::sync::atomic::Ordering::SeqCst);
            let pool_size = app_monitor.pool_size();
            
            if pool_size > 0 {
                log::info!("Pool has {} transactions, total processed: {}", pool_size, current_tx_count);
                
                // Check if transactions are stuck
                if current_tx_count == last_tx_count && pool_size > 0 {
                    stale_count += 1;
                    if stale_count >= 5 { // 500ms without progress
                        log::warn!("Transactions appear stuck - pool size: {}, processed: {}", pool_size, current_tx_count);
                        // In a real implementation, we might trigger consensus here
                    }
                } else {
                    stale_count = 0;
                }
                last_tx_count = current_tx_count;
            }
        }
    });
    
    Ok((app, replica))
}

async fn run_throughput_benchmark(
    blockchain: &Arc<AtomiqApp>,
    config: &BenchmarkConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let monitor = Arc::new(PerformanceMonitor::new());
    
    println!("\n🏁 Starting Throughput Benchmark");
    println!("Target: {} transactions", config.total_transactions);
    println!("Batch size: {}", config.batch_size);
    println!("Concurrent submitters: {}", config.concurrent_submitters);
    
    let start_time = Instant::now();
    
    // Create transaction submitter tasks
    let mut handles = vec![];
    let transactions_per_submitter = config.total_transactions / config.concurrent_submitters;
    
    for submitter_id in 0..config.concurrent_submitters {
        let blockchain_clone = blockchain.clone();
        let monitor_clone = monitor.clone();
        let batch_size = config.batch_size;
        
        let handle = tokio::spawn(async move {
            submit_transactions(
                blockchain_clone,
                monitor_clone,
                submitter_id,
                transactions_per_submitter,
                batch_size,
            ).await
        });
        
        handles.push(handle);
    }
    
    // Start monitoring task
    let blockchain_monitor = blockchain.clone();
    let monitor_clone = monitor.clone();
    let monitoring_handle = tokio::spawn(async move {
        monitor_progress(blockchain_monitor, monitor_clone).await
    });
    
    // Wait for all submitters to complete
    for handle in handles {
        handle.await?;
    }
    
    let submission_time = start_time.elapsed();
    
    println!("\n📤 All transactions submitted in {:?}", submission_time);
    println!("⏳ Waiting for blockchain to process all transactions...");
    
    // Wait for all transactions to be processed
    wait_for_processing_completion(blockchain, config.total_transactions).await;
    
    let total_time = start_time.elapsed();
    
    // Final metrics
    let final_metrics = blockchain.get_metrics();
    let actual_tps = final_metrics.total_transactions as f64 / total_time.as_secs_f64();
    
    println!("\n🎯 BENCHMARK RESULTS");
    println!("==================");
    println!("Total transactions: {}", final_metrics.total_transactions);
    println!("Total blocks: {}", final_metrics.total_blocks);
    println!("Total time: {:?}", total_time);
    println!("🚄 Actual TPS: {:.2}", actual_tps);
    println!("📦 Avg transactions per block: {:.1}", 
        final_metrics.total_transactions as f64 / final_metrics.total_blocks as f64);
    
    // Theoretical vs actual performance
    let theoretical_tps = config.blockchain_config.max_transactions_per_block as f64 
        / (config.blockchain_config.max_block_time_ms as f64 / 1000.0);
    println!("🎯 Theoretical max TPS: {:.2}", theoretical_tps);
    println!("📊 Efficiency: {:.1}%", (actual_tps / theoretical_tps) * 100.0);
    
    monitoring_handle.abort();
    Ok(())
}

async fn submit_transactions(
    blockchain: Arc<AtomiqApp>,
    monitor: Arc<PerformanceMonitor>,
    submitter_id: usize,
    transaction_count: usize,
    batch_size: usize,
) {
    println!("🔄 Submitter {} starting with {} transactions", submitter_id, transaction_count);
    
    let mut submitted = 0;
    let sender = [submitter_id as u8; 32];
    let mut nonce = 1;
    
    while submitted < transaction_count {
        let batch_end = std::cmp::min(submitted + batch_size, transaction_count);
        
        for _ in submitted..batch_end {
            let transaction = Transaction {
                id: 0, // Will be assigned by blockchain
                sender,
                data: format!("data_{}_{}", submitter_id, nonce).into_bytes(),
                timestamp: 0, // Will be assigned by blockchain
                nonce,
            };
            
            blockchain.submit_transaction(transaction);
            nonce += 1;
        }
        
        let batch_count = batch_end - submitted;
        monitor.record_transactions(batch_count as u64);
        submitted = batch_end;
        
        // Small delay to prevent overwhelming the system
        if submitted % (batch_size * 10) == 0 {
            tokio::task::yield_now().await;
        }
    }
    
    println!("✅ Submitter {} completed {} transactions", submitter_id, transaction_count);
}

async fn monitor_progress(
    blockchain: Arc<AtomiqApp>,
    monitor: Arc<PerformanceMonitor>,
) {
    let mut last_metrics = blockchain.get_metrics();
    
    loop {
        sleep(Duration::from_secs(2)).await;
        
        let current_metrics = blockchain.get_metrics();
        let current_tps = monitor.calculate_tps();
        
        let tx_delta = current_metrics.total_transactions - last_metrics.total_transactions;
        let block_delta = current_metrics.total_blocks - last_metrics.total_blocks;
        
        println!(
            "📊 Progress: {} tx ({} +{}), {} blocks (+{}), TPS: {:.1}, Pool: {}",
            current_metrics.total_transactions,
            current_metrics.total_transactions,
            tx_delta,
            current_metrics.total_blocks,
            block_delta,
            current_tps,
            current_metrics.pending_transactions,
        );
        
        last_metrics = current_metrics;
    }
}

async fn wait_for_processing_completion(blockchain: &Arc<AtomiqApp>, target_transactions: usize) {
    loop {
        let metrics = blockchain.get_metrics();
        
        if metrics.total_transactions >= target_transactions as u64 && metrics.pending_transactions == 0 {
            break;
        }
        
        sleep(Duration::from_millis(100)).await;
    }
}