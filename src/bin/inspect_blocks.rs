use atomiq::Block;
use rocksdb::DB;
use std::path::Path;

fn main() {
    let db_path = Path::new("./blockchain_data");
    
    if !db_path.exists() {
        println!("❌ No blockchain data found at {:?}", db_path);
        return;
    }
    
    let db = DB::open_default(db_path).expect("Failed to open database");
    
    // Get latest height
    let latest_height = if let Ok(Some(data)) = db.get(b"latest_height") {
        let bytes: [u8; 8] = data[..8].try_into().unwrap();
        u64::from_le_bytes(bytes)
    } else {
        println!("❌ No latest_height key found");
        return;
    };
    
    println!("🔍 Blockchain Inspector");
    println!("=======================");
    println!("Latest Height: {}\n", latest_height);
    
    // List all keys
    println!("📋 Database Keys:");
    let mut count = 0;
    for item in db.iterator(rocksdb::IteratorMode::Start) {
        if let Ok((key, _value)) = item {
            let key_str = String::from_utf8_lossy(&key);
            println!("   {}", key_str);
            count += 1;
            if count >= 20 {
                break;
            }
        }
    }
    println!();
    
    // Inspect latest block and a few before it
    for height in latest_height.saturating_sub(2)..=latest_height {
        let key = format!("block:height:{}", height);
        match db.get(key.as_bytes()) {
            Ok(Some(data)) => {
                match bincode::deserialize::<Block>(&data) {
                    Ok(block) => {
                        println!("📦 Block #{}", height);
                        println!("   Hash: {}", hex::encode(&block.block_hash));
                        println!("   Previous Hash: {}", hex::encode(&block.previous_block_hash));
                        println!("   Height: {}", block.height);
                        println!("   Transactions: {}", block.transaction_count);
                        println!("   Transactions Root: {}", hex::encode(&block.transactions_root));
                        println!("   State Root: {}", hex::encode(&block.state_root));
                        println!("   Timestamp: {}", block.timestamp);
                        println!("   ✓ Hash verified: {}", block.verify_hash());
                        println!("   ✓ TX root verified: {}", block.verify_transactions_root());
                        
                        // Show first transaction if available
                        if !block.transactions.is_empty() {
                            let tx = &block.transactions[0];
                            println!("   First TX: {} (hash: {})", tx.id, hex::encode(&tx.hash()));
                        }
                        println!();
                    }
                    Err(e) => println!("   ❌ Failed to deserialize block {}: {}\n", height, e),
                }
            }
            Ok(None) => println!("   ⚠️  Block {} not found\n", height),
            Err(e) => println!("   ❌ Error reading block {}: {}\n", height, e),
        }
    }
    
    // Verify chain linkage for blocks that exist
    if latest_height > 1 {
        println!("🔗 Chain Linkage Verification:");
        let mut valid = true;
        let start_height = latest_height.saturating_sub(10);
        
        for height in (start_height + 1)..=latest_height {
            let current_key = format!("block:height:{}", height);
            let prev_key = format!("block:height:{}", height - 1);
            
            match (
                db.get(current_key.as_bytes()),
                db.get(prev_key.as_bytes())
            ) {
                (Ok(Some(curr_data)), Ok(Some(prev_data))) => {
                    if let (Ok(curr_block), Ok(prev_block)) = (
                        bincode::deserialize::<Block>(&curr_data),
                        bincode::deserialize::<Block>(&prev_data)
                    ) {
                        if curr_block.previous_block_hash != prev_block.block_hash {
                            println!("   ❌ Block {} -> {} linkage broken!", height - 1, height);
                            println!("      Expected: {}", hex::encode(&prev_block.block_hash));
                            println!("      Got: {}", hex::encode(&curr_block.previous_block_hash));
                            valid = false;
                        } else {
                            println!("   ✅ Block {} -> {} linked correctly", height - 1, height);
                        }
                    }
                }
                _ => {
                    println!("   ⚠️  Could not verify linkage for blocks {} -> {}", height - 1, height);
                }
            }
        }
        
        if valid {
            println!("\n✅ All checked blocks properly linked!");
        }
    }
}
