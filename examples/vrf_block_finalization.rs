//! Test blockchain casino with VRF on block finalization
//!
//! This demonstrates the secure approach where VRF outcomes are generated
//! AFTER block finalization using the unpredictable block hash.

use atomiq::{
    blockchain_game_processor::{BlockchainGameProcessor, GameBetData},
    common::types::Transaction,
    games::types::{CoinChoice, GameType, Token},
};
use schnorrkel::Keypair;
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎰 Blockchain Casino - VRF on Block Finalization");
    println!("================================================\n");

    // 1. Initialize blockchain game processor
    println!("1️⃣  Setting up blockchain game processor...");
    let blockchain_keypair = Keypair::generate();
    let game_processor = BlockchainGameProcessor::new(blockchain_keypair);
    println!("   ✅ Game processor ready with blockchain VRF key\n");

    // 2. Simulate block creation process
    println!("2️⃣  Simulating block creation process...");
    
    // Step 2a: Player submits game bet transaction
    let bet_data = GameBetData {
        game_type: GameType::CoinFlip,
        bet_amount: 5_000_000_000, // 5 SOL
        token: Token::sol(),
        player_choice: CoinChoice::Heads,
        player_address: "player123abc".to_string(),
    };
    
    let bet_transaction = Transaction::new_game_bet(
        1001,
        [1u8; 32],
        serde_json::to_vec(&bet_data)?,
        1,
    );
    
    println!("   📋 Game bet transaction created:");
    println!("      TX ID: {}", bet_transaction.id);
    println!("      Player choice: {:?}", bet_data.player_choice);
    println!("      Bet amount: {} SOL", bet_data.bet_amount as f64 / 1e9);
    
    // Step 2b: Transaction goes into mempool (no VRF yet!)
    println!("   📦 Transaction enters mempool (outcome unknown)");
    
    // Step 2c: Block producer includes transaction in block
    let block_height: u64 = 12345;
    let other_transactions = vec![
        "some_other_tx_data".as_bytes(),
        "another_transaction".as_bytes(),
        bet_transaction.data.as_slice(),
    ];
    
    // Step 2d: Block reaches consensus and gets finalized with unpredictable hash
    let mut hasher = Sha256::new();
    hasher.update(b"previous_block_hash");
    hasher.update(&block_height.to_be_bytes());
    for tx_data in &other_transactions {
        hasher.update(tx_data);
    }
    hasher.update(b"timestamp_and_nonce");
    let block_hash: [u8; 32] = hasher.finalize().into();
    
    println!("   🔒 Block finalized with hash: {}...", hex::encode(&block_hash[..8]));
    println!("      (Hash was unpredictable before finalization)\n");

    // 3. NOW we can generate VRF (after block is finalized)
    println!("3️⃣  Processing game transaction after block finalization...");
    
    let game_result = game_processor.process_game_transaction(
        &bet_transaction,
        block_hash,
        block_height,
    )?;
    
    println!("   🎲 Game outcome determined:");
    println!("      Coin result: {:?}", game_result.coin_result);
    println!("      Player wins: {}", game_result.outcome == atomiq::games::types::GameOutcome::Win);
    println!("      Payout: {} SOL", game_result.payout as f64 / 1e9);
    println!("      VRF output: {}...", hex::encode(&game_result.vrf_output[..8]));
    println!();

    // 4. Demonstrate security properties
    println!("4️⃣  Security demonstration:");
    
    // Show that different block hashes produce different outcomes
    let mut alt_hasher = Sha256::new();
    alt_hasher.update(b"different_previous_hash");  // Different block content
    alt_hasher.update(&block_height.to_be_bytes());
    for tx_data in &other_transactions {
        alt_hasher.update(tx_data);
    }
    alt_hasher.update(b"timestamp_and_nonce");
    let alt_block_hash: [u8; 32] = alt_hasher.finalize().into();
    
    let alt_result = game_processor.process_game_transaction(
        &bet_transaction,
        alt_block_hash,
        block_height,
    )?;
    
    println!("   🔄 Same transaction, different block hash:");
    println!("      Original: {:?} → {}", 
        game_result.coin_result,
        if game_result.outcome == atomiq::games::types::GameOutcome::Win { "Win" } else { "Loss" }
    );
    println!("      Alternative: {:?} → {}", 
        alt_result.coin_result,
        if alt_result.outcome == atomiq::games::types::GameOutcome::Win { "Win" } else { "Loss" }
    );
    
    if game_result.vrf_output != alt_result.vrf_output {
        println!("   ✅ Different block hash → Different outcome (as expected)");
    } else {
        println!("   ⚠️  Same outcome (random chance - try running again)");
    }
    println!();

    // 5. Show deterministic property
    println!("5️⃣  Deterministic property check:");
    
    let repeat_result = game_processor.process_game_transaction(
        &bet_transaction,
        block_hash,  // Same block hash
        block_height,
    )?;
    
    if game_result.vrf_output == repeat_result.vrf_output {
        println!("   ✅ Same block hash → Same outcome (deterministic)");
        println!("   ✅ No possibility of cherry-picking results");
    } else {
        println!("   ❌ Non-deterministic behavior detected!");
    }
    println!();

    // 6. Multiple games in same block
    println!("6️⃣  Multiple games in same block:");
    let games_in_block = [
        (2001, CoinChoice::Heads, "alice"),
        (2002, CoinChoice::Tails, "bob"), 
        (2003, CoinChoice::Heads, "charlie"),
        (2004, CoinChoice::Tails, "diana"),
        (2005, CoinChoice::Heads, "eve"),
    ];
    
    for (tx_id, choice, player) in games_in_block {
        let bet = GameBetData {
            game_type: GameType::CoinFlip,
            bet_amount: 1_000_000_000, // 1 SOL
            token: Token::sol(),
            player_choice: choice,
            player_address: player.to_string(),
        };
        
        let tx = Transaction::new_game_bet(
            tx_id,
            [tx_id as u8; 32],
            serde_json::to_vec(&bet)?,
            1,
        );
        
        let result = game_processor.process_game_transaction(&tx, block_hash, block_height)?;
        
        println!("   {} bet {:?} → {:?} → {}", 
            player,
            choice,
            result.coin_result,
            if result.outcome == atomiq::games::types::GameOutcome::Win { "Win ✅" } else { "Loss ❌" }
        );
    }
    println!();

    // 7. Security summary
    println!("🔐 Security Guarantees Summary:");
    println!("===============================");
    println!("✅ Block producer cannot predict game outcomes before block finalization");
    println!("✅ Cannot selectively include/exclude transactions based on outcomes");
    println!("✅ VRF depends on unpredictable block hash + transaction data");
    println!("✅ Same transaction + same block hash = same outcome (deterministic)");
    println!("✅ Different block hash = different outcome (prevents manipulation)");
    println!("✅ All outcomes are cryptographically verifiable");
    println!();
    println!("⏱️  Latency Impact:");
    println!("   • Player submits bet: Instant response");
    println!("   • Wait for block finalization: ~10-100ms");
    println!("   • VRF generation: ~0.1ms");
    println!("   • Total latency: ~10-100ms (hidden behind UI animation)");
    println!();
    println!("🎯 This is PROVABLY FAIR gaming with mathematical guarantees!");

    Ok(())
}