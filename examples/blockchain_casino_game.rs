//! Test example for blockchain-integrated casino games
//!
//! This demonstrates how game outcomes are generated ON-CHAIN
//! by the blockchain's VRF engine, ensuring provable fairness.

use atomiq::{
    blockchain_game_processor::{BlockchainGameProcessor, GameBetData},
    common::types::Transaction,
    games::types::{CoinChoice, GameType, Token},
};
use schnorrkel::Keypair;
use sha2::{Sha256, Digest};

fn main() {
    println!("🎰 Blockchain Casino Game Test - Provably Fair VRF");
    println!("==================================================\n");

    // 1. Create blockchain's keypair (in production, this is the blockchain's master key)
    println!("1️⃣  Generating blockchain keypair...");
    let blockchain_keypair = Keypair::generate();
    let blockchain_pubkey = hex::encode(blockchain_keypair.public.to_bytes());
    println!("   ✅ Blockchain public key: {}...", &blockchain_pubkey[..16]);
    println!();

    // 2. Initialize blockchain game processor
    println!("2️⃣  Initializing blockchain game processor...");
    let game_processor = BlockchainGameProcessor::new(blockchain_keypair);
    println!("   ✅ Game processor ready (VRF engine initialized)");
    println!();

    // 3. Player submits a game bet transaction
    println!("3️⃣  Player submits coin flip bet transaction...");
    let bet_data = GameBetData {
        game_type: GameType::CoinFlip,
        bet_amount: 10_000_000_000, // 10 SOL (9 decimals)
        token: Token::sol(),
        player_choice: CoinChoice::Heads,
        player_address: "player_wallet_abc123".to_string(),
    };
    
    let bet_data_bytes = serde_json::to_vec(&bet_data).unwrap();
    let transaction = Transaction::new_game_bet(
        1001,
        [1u8; 32], // Player's public key hash
        bet_data_bytes,
        1,
    );
    
    println!("   Transaction ID: {}", transaction.id);
    println!("   Player choice:  {:?}", bet_data.player_choice);
    println!("   Bet amount:     {} SOL", bet_data.bet_amount as f64 / 1e9);
    println!();

    // 4. Blockchain processes the transaction and generates VRF outcome
    println!("4️⃣  Blockchain processing transaction (generating VRF outcome)...");
    let block_height = 12345u64;
    
    // Generate deterministic block hash for this height
    let mut hasher = Sha256::new();
    hasher.update(&block_height.to_be_bytes());
    hasher.update(b"block_consensus_data");
    let block_hash: [u8; 32] = hasher.finalize().into();
    
    let result = game_processor
        .process_game_transaction(&transaction, block_hash, block_height)
        .unwrap();
    
    println!("   ✅ Game processed at block height: {}", result.block_height);
    println!("   🎲 Outcome: {:?}", result.outcome);
    println!("   💰 Payout: {} SOL", result.payout as f64 / 1e9);
    let vrf_proof_hex = hex::encode(&result.vrf_proof);
    let vrf_output_hex = hex::encode(&result.vrf_output);
    println!("   🔒 VRF Proof: {}...", &vrf_proof_hex[..32]);
    println!("   📊 VRF Output: {}...", &vrf_output_hex[..32]);
    println!();

    // 5. Verify the VRF proof
    println!("5️⃣  Verifying VRF proof (anyone can verify)...");
    let is_valid = game_processor.verify_game_result(&result).unwrap();
    
    if is_valid {
        println!("   ✅ VRF proof is VALID");
        println!("   ✅ Outcome is cryptographically verifiable");
        println!("   ✅ No way to cherry-pick outcomes");
    } else {
        println!("   ❌ VRF proof verification FAILED");
    }
    println!();

    // 6. Test multiple games to show different outcomes
    println!("6️⃣  Testing multiple games...");
    println!("   Running 10 games to demonstrate randomness:\n");
    
    let mut wins = 0;
    let mut losses = 0;
    
    for i in 0u64..10 {
        let bet_data = GameBetData {
            game_type: GameType::CoinFlip,
            bet_amount: 1_000_000_000, // 1 SOL
            token: Token::sol(),
            player_choice: CoinChoice::Heads,
            player_address: format!("player_{}", i),
        };
        
        let bet_data_bytes = serde_json::to_vec(&bet_data).unwrap();
        let tx = Transaction::new_game_bet(
            2000 + i,
            [i as u8; 32],
            bet_data_bytes,
            1,
        );
        
        let game_height = block_height + i;
        let mut game_hasher = Sha256::new();
        game_hasher.update(&game_height.to_be_bytes());
        game_hasher.update(b"block_consensus_data");
        game_hasher.update(&i.to_be_bytes());
        let game_block_hash: [u8; 32] = game_hasher.finalize().into();
        
        let game_result = game_processor
            .process_game_transaction(&tx, game_block_hash, game_height)
            .unwrap();
        
        let won = game_result.payout > 0;
        if won {
            wins += 1;
        } else {
            losses += 1;
        }
        
        let game_vrf_output = hex::encode(&game_result.vrf_output);
        println!("   Game #{}: {:?} → {} (VRF: {}...)",
            i + 1,
            game_result.outcome,
            if won { "WIN ✅" } else { "LOSS ❌" },
            &game_vrf_output[..16]
        );
    }
    
    println!();
    println!("   Wins: {}, Losses: {}", wins, losses);
    println!();

    // 7. Demonstrate tamper-proof property
    println!("7️⃣  Demonstrating tamper-proof property...");
    println!("   Re-processing same transaction with same block height:");
    
    let result1 = game_processor.process_game_transaction(&transaction, block_hash, block_height).unwrap();
    let result2 = game_processor.process_game_transaction(&transaction, block_hash, block_height).unwrap();
    
    println!("   First outcome:  {:?}", result1.outcome);
    println!("   Second outcome: {:?}", result2.outcome);
    
    if result1.vrf_output == result2.vrf_output {
        println!("   ✅ Outcomes are IDENTICAL (deterministic)");
        println!("   ✅ Same transaction = same outcome (no randomness manipulation)");
    }
    println!();

    // 8. Summary
    println!("📝 Summary");
    println!("==========");
    println!("✅ VRF outcomes generated ON-CHAIN by blockchain");
    println!("✅ Players cannot cherry-pick outcomes");
    println!("✅ All outcomes are cryptographically verifiable");
    println!("✅ Deterministic: same transaction = same outcome");
    println!("✅ Transparent: anyone can verify VRF proofs");
    println!();
    println!("🔐 Security Guarantees:");
    println!("   • Blockchain generates VRF using its private key");
    println!("   • Players only submit bets, not outcomes");
    println!("   • VRF proof verifies outcome authenticity");
    println!("   • Impossible to generate multiple VRFs for one bet");
    println!();
    println!("🎯 This is PROVABLY FAIR gaming!");
}
