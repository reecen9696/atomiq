// Standalone test for coin flip game functionality
// Run with: cargo run --example test_coinflip

use atomiq::games::*;
use std::sync::Arc;

fn main() {
    println!("🎰 Coin Flip Game Test\n");
    println!("Testing VRF-based provably fair coin flip...\n");

    // Create VRF engine and game processor
    let vrf_engine = Arc::new(VRFGameEngine::new_random());
    let game_processor = Arc::new(GameProcessor::new(vrf_engine.clone()));

    // Test 1: Single coin flip (Heads)
    println!("📋 Test 1: Play Coin Flip (Heads)");
    let request_heads = CoinFlipPlayRequest {
        player_id: "test-player-123".to_string(),
        choice: CoinChoice::Heads,
        token: Token::sol(),
        bet_amount: 1.0,
        wallet_signature: None,
    };

    match game_processor.process_coinflip(request_heads) {
        Ok(result) => {
            println!("✅ Game processed successfully!");
            println!("   Game ID: {}", result.game_id);
            println!("   Player choice: {:?}", match &result.game_data {
                GameData::CoinFlip { player_choice, .. } => player_choice,
            });
            println!("   Result: {:?}", match &result.game_data {
                GameData::CoinFlip { result_choice, .. } => result_choice,
            });
            println!("   Outcome: {:?}", result.outcome);
            println!("   Bet amount: {} SOL", result.payment.bet_amount);
            println!("   Payout: {} SOL", result.payment.payout_amount);
            println!("\n🔐 VRF Proof:");
            println!("   VRF Output: {}...", &result.vrf.vrf_output[..16]);
            println!("   VRF Proof: {}...", &result.vrf.vrf_proof[..16]);
            println!("   Public Key: {}...", &result.vrf.public_key[..16]);
            
            // Verify the VRF proof
            println!("\n🔍 Verifying VRF Proof...");
            match VRFGameEngine::verify_vrf_proof(&result.vrf, &result.vrf.input_message) {
                Ok(true) => println!("   ✅ VRF proof is valid!"),
                Ok(false) => println!("   ❌ VRF proof is invalid!"),
                Err(e) => println!("   ❌ Verification error: {}", e),
            }
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!("\n{}", "=".repeat(60));

    // Test 2: Single coin flip (Tails)
    println!("\n📋 Test 2: Play Coin Flip (Tails)");
    let request_tails = CoinFlipPlayRequest {
        player_id: "test-player-456".to_string(),
        choice: CoinChoice::Tails,
        token: Token::usdc(),
        bet_amount: 10.0,
        wallet_signature: None,
    };

    match game_processor.process_coinflip(request_tails) {
        Ok(result) => {
            println!("✅ Game processed successfully!");
            println!("   Game ID: {}", result.game_id);
            println!("   Player choice: {:?}", match &result.game_data {
                GameData::CoinFlip { player_choice, .. } => player_choice,
            });
            println!("   Result: {:?}", match &result.game_data {
                GameData::CoinFlip { result_choice, .. } => result_choice,
            });
            println!("   Outcome: {:?}", result.outcome);
            println!("   Bet amount: {} {}", result.payment.bet_amount, result.payment.token.symbol);
            println!("   Payout: {} {}", result.payment.payout_amount, result.payment.token.symbol);
        }
        Err(e) => println!("❌ Error: {}", e),
    }

    println!("\n{}", "=".repeat(60));

    // Test 3: Multiple games
    println!("\n📋 Test 3: Batch Test (10 Games)");
    let mut wins = 0;
    let mut losses = 0;

    for i in 1..=10 {
        let choice = if i % 2 == 0 { CoinChoice::Heads } else { CoinChoice::Tails };
        let request = CoinFlipPlayRequest {
            player_id: format!("player-{}", i),
            choice,
            token: Token::sol(),
            bet_amount: 0.1,
            wallet_signature: None,
        };

        if let Ok(result) = game_processor.process_coinflip(request) {
            match result.outcome {
                GameOutcome::Win => {
                    wins += 1;
                    println!("   Game {}: {:?} → Win ✅", i, choice);
                }
                GameOutcome::Loss => {
                    losses += 1;
                    println!("   Game {}: {:?} → Loss ❌", i, choice);
                }
            }
        }
    }

    println!("\n📊 Results:");
    println!("   Wins: {}/10 ({}%)", wins, wins * 10);
    println!("   Losses: {}/10 ({}%)", losses, losses * 10);

    println!("\n{}", "=".repeat(60));

    // Test 4: Supported tokens
    println!("\n📋 Test 4: Supported Tokens");
    let tokens = Token::all_supported();
    for token in tokens {
        println!("   • {} {}", token.symbol, 
            if let Some(mint) = token.mint_address {
                format!("(Mint: {}...)", &mint[..8])
            } else {
                "(Native)".to_string()
            }
        );
    }

    println!("\n🎉 All tests completed!");
    println!("\n💡 To test the HTTP API, you would:");
    println!("   1. Start blockchain: cargo run --release --bin atomiq-unified");
    println!("   2. Start API server: cargo run --release --bin atomiq-api");
    println!("   3. Test endpoint: ./scripts/test_games.sh");
}
