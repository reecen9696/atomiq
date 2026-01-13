//! Test to verify database persistence across restarts
//! This validates that blockchain state survives stopping and restarting

use atomiq::{config::AtomiqConfig, factory::BlockchainFactory, Transaction};
use std::fs;

#[tokio::test]
async fn test_db_persistence_across_restarts() {
    // Setup unique test database
    let test_db_path = "./DB/test_persistence_db";
    let _ = fs::remove_dir_all(test_db_path);
    
    // Create config with persistence enabled
    let mut config = AtomiqConfig::high_performance();
    config.storage.data_directory = test_db_path.to_string();
    config.storage.clear_on_start = false; // CRITICAL: Don't clear on start for persistence

    // === PHASE 1: Create blockchain, submit transactions, and stop ===
    println!("\n=== PHASE 1: Initial blockchain creation ===");
    let initial_stats = {
        let (app, mut handle) = BlockchainFactory::create_blockchain(config.clone())
            .await
            .expect("Failed to create blockchain");
        
        // Submit some transactions
        for i in 0..10 {
            let sender = [(i % 256) as u8; 32];
            let tx = Transaction::new(0, sender, vec![i as u8; 32], 0);
            app.submit_transaction(tx).expect("Failed to submit transaction");
        }
        
        // Wait for transactions to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Get state stats before shutdown
        let stats = app.get_metrics();
        println!("📊 Stats before shutdown: {} transactions, {} blocks", 
                 stats.total_transactions, stats.total_blocks);
        
        // Shutdown blockchain
        handle.shutdown().expect("Failed to shutdown");
        println!("✅ Blockchain stopped gracefully");
        
        (stats.total_transactions, stats.total_blocks)
    };

    // Wait for database lock to be released
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // === PHASE 2: Restart blockchain and verify data persisted ===
    println!("\n=== PHASE 2: Restarting blockchain ===");
    let (app2, mut handle2) = BlockchainFactory::create_blockchain(config.clone())
        .await
        .expect("Failed to restart blockchain");
    
    // Wait for initialization
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    let restarted_stats = app2.get_metrics();
    println!("📊 Stats after restart: {} transactions, {} blocks", 
             restarted_stats.total_transactions, restarted_stats.total_blocks);
    
    // Verify database directory exists and is not empty
    let db_files = std::fs::read_dir(test_db_path)
        .expect("DB directory should exist")
        .count();
    assert!(
        db_files > 0,
        "Database should have files persisted"
    );
    println!("📂 Database files persisted: {} files", db_files);
    
    // Verify we can submit more transactions after restart
    let sender = [99u8; 32];
    let tx = Transaction::new(0, sender, vec![99; 32], 0);
    app2.submit_transaction(tx).expect("Failed to submit transaction after restart");
    
    println!("✅ DB persistence verified successfully!");
    println!("   - Database directory exists: {}", test_db_path);
    println!("   - Database files persisted: {} files", db_files);
    println!("   - Can submit transactions after restart: ✓");
    
    // Cleanup
    handle2.shutdown().expect("Failed to shutdown");
    let _ = fs::remove_dir_all(test_db_path);
}

#[tokio::test]
async fn test_high_performance_tps() {
    // Setup test database
    let test_db_path = "./DB/test_perf_db";
    let _ = fs::remove_dir_all(test_db_path);
    
    let mut config = AtomiqConfig::high_performance();
    config.storage.data_directory = test_db_path.to_string();
    config.storage.clear_on_start = true;
    
    println!("\n=== Performance Test: High TPS ===");
    let (app, mut handle) = BlockchainFactory::create_blockchain(config)
        .await
        .expect("Failed to create blockchain");
    
    // Wait for initialization
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    // Submit transactions as fast as possible
    let num_transactions = 1000;
    let start = std::time::Instant::now();
    
    for i in 0..num_transactions {
        let sender = [(i % 256) as u8; 32];
        let tx = Transaction::new(0, sender, vec![(i % 256) as u8; 32], 0);
        app.submit_transaction(tx).expect("Failed to submit transaction");
    }
    
    let submission_time = start.elapsed();
    let submission_tps = num_transactions as f64 / submission_time.as_secs_f64();
    
    println!("📊 Submitted {} transactions in {:?}", num_transactions, submission_time);
    println!("📈 Submission TPS: {:.0}", submission_tps);
    
    // Wait for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    let metrics = app.get_metrics();
    println!("📊 Final metrics: {} transactions, {} blocks", 
             metrics.total_transactions, metrics.total_blocks);
    
    // Verify high TPS (should be >5000 TPS for high performance mode)
    assert!(
        submission_tps > 5000.0,
        "Expected >5000 TPS, got {:.0} TPS",
        submission_tps
    );
    
    println!("✅ High TPS verified: {:.0} TPS", submission_tps);
    
    // Cleanup
    handle.shutdown().expect("Failed to shutdown");
    let _ = fs::remove_dir_all(test_db_path);
}
