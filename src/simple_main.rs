//! Simple High-Performance Blockchain Demo
//! 
//! Bypasses HotStuff consensus for pure throughput testing

use atomiq::{AtomiqApp, BlockchainConfig, Transaction};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    time::sleep,
    task,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Simple Lean Blockchain Performance Test");
    println!("============================================");
    
    // Create blockchain without HotStuff for pure performance test
    let config = BlockchainConfig {
        max_transactions_per_block: 10000,
        max_block_time_ms: 1,
        enable_state_validation: true,
        batch_size_threshold: 1000,
    };
    
    let app = Arc::new(AtomiqApp::new(config.clone()));
    
    println!("✅ Simple blockchain app created");
    println!("📊 Config: {} max tx/block, {}ms target block time", 
        config.max_transactions_per_block, config.max_block_time_ms);

    // Start block production task
    let app_clone = app.clone();
    let block_producer = task::spawn(async move {
        let mut block_count = 0;
        loop {
            let pool_size = app_clone.pool_size();
            if pool_size > 0 {
                println!("🔄 Creating block with {} transactions", pool_size);
                
                // Manually trigger block creation by simulating HotStuff calls
                let block_data = format!("block_{}", block_count).into_bytes();
                
                // Get all transactions from pool
                let transactions = app_clone.drain_transaction_pool();
                
                if !transactions.is_empty() {
                    println!("📦 Processing {} transactions in block {}", transactions.len(), block_count);
                    
                    // Process transactions (would be called by HotStuff)
                    let start = Instant::now();
                    let (results, _updates) = app_clone.execute_transactions(&transactions);
                    let process_time = start.elapsed();
                    
                    let tps = transactions.len() as f64 / process_time.as_secs_f64();
                    println!("⚡ Processed {} txs in {:?} (TPS: {:.0})", 
                        transactions.len(), process_time, tps);
                    
                    app_clone.block_counter().fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    block_count += 1;
                }
            }
            
            sleep(Duration::from_millis(config.max_block_time_ms)).await;
        }
    });

    // Run throughput test
    run_throughput_test(app.clone()).await?;

    // Keep block producer running briefly to process final transactions
    sleep(Duration::from_secs(2)).await;
    block_producer.abort();
    
    Ok(())
}

async fn run_throughput_test(app: Arc<AtomiqApp>) -> Result<(), Box<dyn std::error::Error>> {
    let total_transactions = 100_000;
    let batch_size = 100;
    let concurrent_submitters = 4;
    
    println!("\n🏁 Starting Throughput Benchmark");
    println!("Target: {} transactions", total_transactions);
    println!("Batch size: {}", batch_size);
    println!("Concurrent submitters: {}", concurrent_submitters);
    
    let start_time = Instant::now();
    
    // Create concurrent submitters
    let mut handles = Vec::new();
    let transactions_per_submitter = total_transactions / concurrent_submitters;
    
    for submitter_id in 0..concurrent_submitters {
        let app_clone = app.clone();
        let handle = task::spawn(async move {
            println!("🔄 Submitter {} starting with {} transactions", submitter_id, transactions_per_submitter);
            
            let mut transactions_submitted = 0;
            while transactions_submitted < transactions_per_submitter {
                let batch_end = std::cmp::min(transactions_submitted + batch_size, transactions_per_submitter);
                let current_batch_size = batch_end - transactions_submitted;
                
                // Submit batch of transactions
                for i in 0..current_batch_size {
                    let tx = Transaction {
                        id: 0, // Will be assigned by submit_transaction
                        sender: [(submitter_id as u8); 32],
                        data: format!("tx_{}_{}", submitter_id, i).into_bytes(),
                        timestamp: 0,
                        nonce: i as u64,
                    };
                    
                    app_clone.submit_transaction(tx);
                }
                
                transactions_submitted += current_batch_size;
                
                // Small yield to allow other submitters
                tokio::task::yield_now().await;
            }
            
            println!("✅ Submitter {} completed {} transactions", submitter_id, transactions_per_submitter);
        });
        
        handles.push(handle);
    }
    
    // Wait for all submitters to complete
    for handle in handles {
        handle.await?;
    }
    
    let submission_time = start_time.elapsed();
    println!("\n📤 All transactions submitted in {:?}", submission_time);
    
    // Calculate submission TPS
    let submission_tps = total_transactions as f64 / submission_time.as_secs_f64();
    println!("⚡ Submission TPS: {:.0}", submission_tps);
    
    // Wait for processing and show final stats
    sleep(Duration::from_secs(1)).await;
    
    let final_tx_count = app.transaction_counter().load(std::sync::atomic::Ordering::SeqCst);
    let final_block_count = app.block_counter().load(std::sync::atomic::Ordering::SeqCst);
    
    println!("\n📊 Final Results");
    println!("===============");
    println!("Total transactions processed: {}", final_tx_count);
    println!("Total blocks created: {}", final_block_count);
    println!("Average transactions per block: {:.1}", final_tx_count as f64 / final_block_count as f64);
    
    Ok(())
}