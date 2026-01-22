use atomiq::game_store::{load_game_result, store_game_result};
use atomiq::storage::OptimizedStorage;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./DB/blockchain_data".to_string());
    
    let tx_id: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .expect("Usage: backfill_settlement_index <db_path> <tx_id>");

    println!("Opening database: {}", db_path);
    let storage = Arc::new(OptimizedStorage::new(&db_path)?);
    
    println!("Loading game result for tx_id: {}", tx_id);
    let game_result = match load_game_result(&storage, tx_id) {
        Ok(Some(result)) => result,
        Ok(None) => {
            println!("❌ Game result not found for tx_id: {}", tx_id);
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    
    println!("Current status: {:?}", game_result.settlement_status);
    println!("Version: {}", game_result.version);
    
    // Re-store the game result - this will trigger the settlement index write
    println!("Re-storing game result to trigger settlement index update...");
    store_game_result(&storage, &game_result)?;
    
    println!("✅ Settlement index backfilled successfully");
    
    Ok(())
}
