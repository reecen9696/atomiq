use atomiq::Block;
use rocksdb::DB;

fn main() {
    let db = DB::open_default("./blockchain_data").expect("Failed to open database");
    
    // Get latest height
    let latest_height = if let Ok(Some(data)) = db.get(b"latest_height") {
        let bytes: [u8; 8] = data[..8].try_into().unwrap();
        u64::from_le_bytes(bytes)
    } else {
        println!("❌ No blockchain data found");
        return;
    };
    
    println!("🔍 Blockchain Chain Verification");
    println!("================================");
    println!("Latest Height: {}\n", latest_height);
    
    // Test chain continuity from last 10 blocks
    let start_check = latest_height.saturating_sub(10);
    println!("🔗 Testing Chain Linkage (blocks {} to {}):\n", start_check, latest_height);
    
    let mut prev_block: Option<Block> = None;
    let mut chain_valid = true;
    let mut blocks_checked: u64 = 0;
    
    for height in start_check..=latest_height {
        let key = format!("block:height:{}", height);
        if let Ok(Some(data)) = db.get(key.as_bytes()) {
            if let Ok(block) = bincode::deserialize::<Block>(&data) {
                blocks_checked += 1;
                
                // Verify block integrity
                if !block.verify_hash() {
                    println!("   ❌ Block {} hash verification FAILED!", height);
                    chain_valid = false;
                }
                if !block.verify_transactions_root() {
                    println!("   ❌ Block {} Merkle root verification FAILED!", height);
                    chain_valid = false;
                }
                
                // Verify chain linkage
                if let Some(ref prev) = prev_block {
                    if block.previous_block_hash == prev.block_hash {
                        println!("   ✅ Block {} -> {} linked correctly", height - 1, height);
                        println!("      Hash: {}...{}", 
                            &hex::encode(&block.block_hash)[..16],
                            &hex::encode(&block.block_hash)[48..]);
                    } else {
                        println!("   ❌ Block {} -> {} linkage BROKEN!", height - 1, height);
                        println!("      Expected prev: {}", hex::encode(&prev.block_hash));
                        println!("      Got prev:      {}", hex::encode(&block.previous_block_hash));
                        chain_valid = false;
                    }
                }
                
                prev_block = Some(block);
            }
        }
    }
    
    println!("\n📊 Verification Summary:");
    println!("   Blocks Checked: {}", blocks_checked);
    println!("   Chain Links: {}", blocks_checked.saturating_sub(1));
    
    if chain_valid && blocks_checked > 0 {
        println!("\n✅ BLOCKCHAIN INTEGRITY VERIFIED!");
        println!("   {} consecutive blocks properly linked", blocks_checked);
        println!("   All block hashes verified");
        println!("   All transaction Merkle roots verified");
    } else if !chain_valid {
        println!("\n❌ BLOCKCHAIN INTEGRITY FAILED!");
    }
}
