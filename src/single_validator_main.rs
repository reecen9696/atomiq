//! Single Validator Atomiq Blockchain - HotStuff BFT with 1 Validator
//! 
//! Clean implementation based on HotStuff-rs patterns

use atomiq::{
    AtomiqApp, BlockchainConfig, Transaction,
    storage::OptimizedStorage,
};
use hotstuff_rs::{
    replica::{Configuration, Replica, ReplicaSpec},
    types::{
        crypto_primitives::SigningKey,
        data_types::{ChainID, BufferSize, EpochLength, Power},
        update_sets::{AppStateUpdates, ValidatorSetUpdates},
        validator_set::{ValidatorSet, ValidatorSetState},
    },
    networking::{messages::Message, network::Network},
};
use std::{
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
    fs,
};
use tokio::time::sleep;
use ed25519_dalek::VerifyingKey;
use rand_core::OsRng;

// Single validator mock network (based on HotStuff-rs test pattern)
#[derive(Clone)]
struct SingleValidatorNetwork {
    my_verifying_key: VerifyingKey,
    // For single validator, we just need a channel to self
    sender: Sender<(VerifyingKey, Message)>,
    receiver: Arc<Mutex<Receiver<(VerifyingKey, Message)>>>,
}

impl Network for SingleValidatorNetwork {
    fn init_validator_set(&mut self, _: ValidatorSet) {}
    fn update_validator_set(&mut self, _: ValidatorSetUpdates) {}

    fn send(&mut self, _peer: VerifyingKey, message: Message) {
        // For single validator, send to self
        let _ = self.sender.send((self.my_verifying_key, message));
    }

    fn broadcast(&mut self, message: Message) {
        // For single validator, broadcast to self
        let _ = self.sender.send((self.my_verifying_key, message));
    }

    fn recv(&mut self) -> Option<(VerifyingKey, Message)> {
        match self.receiver.lock().unwrap().try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None, // Don't panic for single validator
        }
    }
}

fn create_single_validator_network(validator_key: VerifyingKey) -> SingleValidatorNetwork {
    let (sender, receiver) = mpsc::channel();
    SingleValidatorNetwork {
        my_verifying_key: validator_key,
        sender,
        receiver: Arc::new(Mutex::new(receiver)),
    }
}

struct BenchmarkConfig {
    pub total_transactions: usize,
    pub batch_size: usize,
    pub blockchain_config: BlockchainConfig,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            total_transactions: 1_000, // Small for testing
            batch_size: 50,
            blockchain_config: BlockchainConfig {
                max_transactions_per_block: 100, // Small blocks
                max_block_time_ms: 1000, // 1 second for consensus to work
                enable_state_validation: true,
                batch_size_threshold: 50,
            },
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("🚀 Single Validator Atomiq Blockchain Test");
    println!("========================================");
    
    // Clean up old data
    let _ = fs::remove_dir_all("./blockchain_data_single");
    
    let benchmark_config = BenchmarkConfig::default();
    
    // 1. Generate signing key for single validator  
    let mut csprg = OsRng;
    let keypair = SigningKey::generate(&mut csprg);
    let verifying_key = keypair.verifying_key();
    
    println!("🔑 Generated key for single validator");
    
    // 2. Create single validator network
    let network = create_single_validator_network(verifying_key);
    
    // 3. Initialize validator set with single validator
    let mut init_vs_updates = ValidatorSetUpdates::new();
    init_vs_updates.insert(verifying_key, Power::new(1));
    
    println!("⚖️ Created validator set with 1 validator");
    
    // 4. Start the validator
    let validator = start_single_validator(
        keypair,
        network,
        init_vs_updates,
        benchmark_config.blockchain_config.clone(),
    ).await?;
    
    println!("✅ Single validator started successfully");
    println!("📊 Config: {} max tx/block, {}ms target block time", 
        benchmark_config.blockchain_config.max_transactions_per_block,
        benchmark_config.blockchain_config.max_block_time_ms);
    
    // 5. Wait longer for consensus to initialize
    sleep(Duration::from_secs(3)).await;
    
    // 6. Run benchmark 
    run_single_validator_benchmark(validator, benchmark_config).await?;
    
    Ok(())
}

async fn start_single_validator(
    keypair: SigningKey,
    network: SingleValidatorNetwork,
    init_vs_updates: ValidatorSetUpdates,
    config: BlockchainConfig,
) -> Result<Arc<AtomiqApp>, Box<dyn std::error::Error>> {
    
    // Create app
    let app = Arc::new(AtomiqApp::new(config.clone()));
    
    // Create storage 
    let storage_path = "./blockchain_data_single";
    let kv_store = OptimizedStorage::new(storage_path)?;
    
    // Initialize validator set (following HotStuff-rs test pattern)
    let mut initial_validator_set = ValidatorSet::new();
    initial_validator_set.apply_updates(&init_vs_updates);
    
    let validator_set_state = ValidatorSetState::new(
        initial_validator_set.clone(),
        initial_validator_set,
        None,
        true, // Set as decided
    );
    
    // Initialize replica storage (crucial step!)
    Replica::initialize(kv_store.clone(), AppStateUpdates::new(), validator_set_state);
    
    println!("🔧 Replica storage initialized");
    
    // Configure replica for single validator (following NumberApp test pattern)
    let configuration = Configuration::builder()
        .me(keypair)
        .chain_id(ChainID::new(1))
        .block_sync_request_limit(10)
        .block_sync_server_advertise_time(Duration::from_secs(10))
        .block_sync_response_timeout(Duration::from_secs(3))
        .block_sync_blacklist_expiry_time(Duration::from_secs(10))
        .block_sync_trigger_min_view_difference(2)
        .block_sync_trigger_timeout(Duration::from_secs(60))
        .progress_msg_buffer_capacity(BufferSize::new(1024))
        .epoch_length(EpochLength::new(50)) // Standard epoch length
        .max_view_time(Duration::from_millis(2000)) // 2 seconds like NumberApp tests
        .log_events(true) // Enable logs to see what's happening
        .build();
    
    // Start replica
    let _replica = ReplicaSpec::builder()
        .app((*app).clone())
        .network(network)
        .kv_store(kv_store)
        .configuration(configuration)
        .build()
        .start();
    
    println!("🏛️ Single validator replica started");
    
    Ok(app)
}

async fn run_single_validator_benchmark(
    validator: Arc<AtomiqApp>,
    config: BenchmarkConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🏁 Starting Single Validator Throughput Benchmark");
    println!("Target: {} transactions", config.total_transactions);
    println!("Batch size: {}", config.batch_size);
    
    let start_time = Instant::now();
    
    // Submit transactions 
    println!("🔄 Submitting {} transactions", config.total_transactions);
    
    for i in 0..config.total_transactions {
        let tx = Transaction {
            id: 0, // Will be assigned
            sender: [(i % 256) as u8; 32],
            data: format!("single_validator_tx_{}", i).into_bytes(),
            timestamp: 0,
            nonce: i as u64,
        };
        
        validator.submit_transaction(tx);
        
        // Small yield every batch to allow processing
        if i % config.batch_size == 0 {
            tokio::task::yield_now().await;
        }
    }
    
    let submission_time = start_time.elapsed();
    println!("📤 All transactions submitted in {:?}", submission_time);
    
    // Monitor progress 
    let monitor_start = Instant::now();
    let mut last_progress_time = monitor_start;
    let mut last_tx_count = 0;
    let mut last_block_count = 0;
    
    loop {
        let metrics = validator.get_metrics();
        let current_time = Instant::now();
        let elapsed_since_last = current_time.duration_since(last_progress_time);
        
        if elapsed_since_last >= Duration::from_secs(2) {
            let tx_progress = metrics.total_transactions - last_tx_count;
            let block_progress = metrics.total_blocks - last_block_count;
            
            println!(
                "📊 Processed: {} tx (+{}), {} blocks (+{}), Pool: {}",
                metrics.total_transactions,
                tx_progress,
                metrics.total_blocks, 
                block_progress,
                metrics.pending_transactions
            );
            
            last_progress_time = current_time;
            last_tx_count = metrics.total_transactions;
            last_block_count = metrics.total_blocks;
        }
        
        // Check completion - need blocks to be created AND pool to be empty
        if metrics.total_blocks > 0 && metrics.pending_transactions == 0 {
            println!("\n🎉 All transactions processed and committed!");
            break;
        }
        
        // More lenient timeout for single validator
        if current_time.duration_since(monitor_start) > Duration::from_secs(60) {
            println!("\n⏰ Benchmark timeout reached");
            println!("Final state: {} tx processed, {} blocks, {} pending", 
                metrics.total_transactions,
                metrics.total_blocks,
                metrics.pending_transactions);
            break;
        }
        
        sleep(Duration::from_millis(500)).await;
    }
    
    // Final results
    let total_time = start_time.elapsed();
    let metrics = validator.get_metrics();
    let final_tps = metrics.total_transactions as f64 / total_time.as_secs_f64();
    
    println!("\n📊 Single Validator Final Results");
    println!("=================================");
    println!("Transactions processed: {}", metrics.total_transactions);
    println!("Blocks created: {}", metrics.total_blocks);
    println!("Total time: {:?}", total_time);
    println!("Overall TPS: {:.0}", final_tps);
    println!("Consensus working: {}", metrics.total_blocks > 0);
    
    if metrics.total_blocks > 0 {
        let avg_tx_per_block = metrics.total_transactions as f64 / metrics.total_blocks as f64;
        println!("Average transactions per block: {:.1}", avg_tx_per_block);
    }
    
    Ok(())
}