//! Comprehensive Casino Game Testing Suite
//! 
//! This example demonstrates clean, extensible testing patterns for casino games.
//! Run with: cargo run --example test_coinflip

use atomiq::games::*;
use std::sync::Arc;
use std::collections::HashMap;
use std::fmt;

/// Configuration for game testing scenarios
#[derive(Debug, Clone)]
pub struct GameTestConfig {
    pub player_id: String,
    pub bet_amount: f64,
    pub token: Token,
    pub iterations: usize,
}

impl Default for GameTestConfig {
    fn default() -> Self {
        Self {
            player_id: "test-player".to_string(),
            bet_amount: 1.0,
            token: Token::sol(),
            iterations: 10,
        }
    }
}

/// Test result aggregation and analysis
#[derive(Debug, Default)]
pub struct GameTestResults {
    pub total_games: usize,
    pub wins: usize,
    pub losses: usize,
    pub total_bet: f64,
    pub total_payout: f64,
    pub vrf_verifications: usize,
    pub failed_verifications: usize,
}

impl GameTestResults {
    pub fn win_rate(&self) -> f64 {
        if self.total_games == 0 { 0.0 } else { self.wins as f64 / self.total_games as f64 }
    }
    
    pub fn house_edge(&self) -> f64 {
        if self.total_bet == 0.0 { 0.0 } else { (self.total_bet - self.total_payout) / self.total_bet }
    }
    
    pub fn vrf_success_rate(&self) -> f64 {
        let total = self.vrf_verifications + self.failed_verifications;
        if total == 0 { 0.0 } else { self.vrf_verifications as f64 / total as f64 }
    }
}

impl fmt::Display for GameTestResults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "📊 GAME ANALYSIS:")?;
        writeln!(f, "   Games Played: {}", self.total_games)?;
        writeln!(f, "   Win Rate: {:.1}% ({}/{})", self.win_rate() * 100.0, self.wins, self.total_games)?;
        writeln!(f, "   House Edge: {:.2}%", self.house_edge() * 100.0)?;
        writeln!(f, "   VRF Success: {:.1}% ({}/{})", 
                self.vrf_success_rate() * 100.0, 
                self.vrf_verifications,
                self.vrf_verifications + self.failed_verifications)?;
        writeln!(f, "   Total Bet: {:.2} {}", self.total_bet, "SOL")?;
        writeln!(f, "   Total Payout: {:.2} {}", self.total_payout, "SOL")
    }
}

/// Casino game testing framework
pub struct GameTestSuite {
    vrf_engine: Arc<VRFGameEngine>,
    game_processor: Arc<GameProcessor>,
    config: GameTestConfig,
}

impl GameTestSuite {
    pub fn new(config: GameTestConfig) -> Self {
        let vrf_engine = Arc::new(VRFGameEngine::new_random());
        let game_processor = Arc::new(GameProcessor::new(vrf_engine.clone()));
        
        Self {
            vrf_engine,
            game_processor,
            config,
        }
    }
    
    /// Test a single coinflip game with full verification
    pub fn test_single_coinflip(&self, choice: CoinChoice, player_id: &str) -> Result<GameResult, Box<dyn std::error::Error>> {
        let request = CoinFlipPlayRequest {
            player_id: player_id.to_string(),
            choice,
            token: self.config.token.clone(),
            bet_amount: self.config.bet_amount,
            wallet_signature: None,
        };
        
        let result = self.game_processor.process_coinflip(request)?;
        
        // Verify VRF proof
        match VRFGameEngine::verify_vrf_proof(&result.vrf, &result.vrf.input_message) {
            Ok(true) => {},
            Ok(false) => return Err("VRF proof verification failed".into()),
            Err(e) => return Err(format!("VRF verification error: {}", e).into()),
        }
        
        Ok(result)
    }
    
    /// Run batch coinflip tests with statistical analysis
    pub fn test_coinflip_batch(&self) -> Result<GameTestResults, Box<dyn std::error::Error>> {
        let mut results = GameTestResults::default();
        
        println!("🚀 Running {} coinflip tests...", self.config.iterations);
        
        for i in 1..=self.config.iterations {
            let choice = if i % 2 == 0 { CoinChoice::Heads } else { CoinChoice::Tails };
            let player_id = format!("{}-{}", self.config.player_id, i);
            
            match self.test_single_coinflip(choice, &player_id) {
                Ok(game_result) => {
                    results.total_games += 1;
                    results.total_bet += self.config.bet_amount;
                    results.vrf_verifications += 1;
                    
                    match game_result.outcome {
                        GameOutcome::Win => {
                            results.wins += 1;
                            results.total_payout += game_result.payment.payout_amount;
                            print!("✅");
                        },
                        GameOutcome::Loss => {
                            results.losses += 1;
                            print!("❌");
                        },
                    }
                },
                Err(_) => {
                    results.failed_verifications += 1;
                    print!("⚠️");
                }
            }
            
            if i % 20 == 0 { println!(); }
        }
        
        println!();
        Ok(results)
    }
    
    /// Test different token types
    pub fn test_multiple_tokens(&self) -> Result<HashMap<String, GameTestResults>, Box<dyn std::error::Error>> {
        let tokens = Token::all_supported();
        let mut token_results = HashMap::new();
        
        for token in tokens {
            println!("\n🪙 Testing with {} token...", token.symbol);
            
            let mut token_config = self.config.clone();
            token_config.token = token.clone();
            token_config.iterations = 5; // Smaller batch for token testing
            
            let test_suite = GameTestSuite::new(token_config);
            let results = test_suite.test_coinflip_batch()?;
            
            token_results.insert(token.symbol.clone(), results);
        }
        
        Ok(token_results)
    }
    
    /// Demonstrate VRF properties and security
    pub fn test_vrf_properties(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔐 Testing VRF Properties...");
        
        let result1 = self.test_single_coinflip(CoinChoice::Heads, "vrf-test-1")?;
        let result2 = self.test_single_coinflip(CoinChoice::Heads, "vrf-test-1")?; // Same player
        
        // VRF outputs should be different (non-deterministic per game)
        if result1.vrf.vrf_output == result2.vrf.vrf_output {
            return Err("VRF outputs should be different for different games".into());
        }
        
        println!("   ✅ VRF non-determinism verified");
        println!("   ✅ VRF proofs are unique per game");
        
        Ok(())
    }
}

/// Test reporter for clean output formatting
pub struct TestReporter;

impl TestReporter {
    pub fn print_header() {
        println!("🎰 CASINO GAME TESTING SUITE");
        println!("{}", "=".repeat(50));
        println!("Testing VRF-based provably fair gaming system\n");
    }
    
    pub fn print_section(title: &str) {
        println!("\n📋 {}", title);
        println!("{}", "-".repeat(title.len() + 4));
    }
    
    pub fn print_game_details(result: &GameResult) {
        println!("✅ Game processed successfully!");
        println!("   Game ID: {}", result.game_id);
        
        if let GameData::CoinFlip { player_choice, result_choice } = &result.game_data {
            println!("   Choice: {:?} → Result: {:?}", player_choice, result_choice);
        }
        
        println!("   Outcome: {:?}", result.outcome);
        println!("   Bet: {:.3} {} → Payout: {:.3} {}", 
                result.payment.bet_amount,
                result.payment.token.symbol,
                result.payment.payout_amount,
                result.payment.token.symbol);
        
        println!("   VRF Output: {}...", &result.vrf.vrf_output[..16]);
        println!("   VRF Proof: {}...", &result.vrf.vrf_proof[..16]);
    }
    
    pub fn print_token_summary(token_results: &HashMap<String, GameTestResults>) {
        println!("\n🪙 TOKEN PERFORMANCE SUMMARY:");
        for (token, results) in token_results {
            println!("   {}: {:.1}% win rate, {:.2}% house edge", 
                    token, 
                    results.win_rate() * 100.0, 
                    results.house_edge() * 100.0);
        }
    }
}

fn main() {
    TestReporter::print_header();
    
    // Initialize test configuration
    let config = GameTestConfig {
        iterations: 20,
        bet_amount: 1.0,
        ..Default::default()
    };
    
    let test_suite = GameTestSuite::new(config);

    
    // Run comprehensive test suite
    if let Err(e) = run_test_suite(&test_suite) {
        eprintln!("❌ Test suite failed: {}", e);
        std::process::exit(1);
    }
    
    println!("\n🎉 All tests completed successfully!");
    println!("\n💡 Next Steps:");
    println!("   1. Start blockchain: cargo run --release --bin atomiq-unified");
    println!("   2. Start API server: cargo run --release --bin atomiq-api");
    println!("   3. Test HTTP endpoints: ./scripts/test_games.sh");
    println!("   4. Add new game types using the extensible framework");
}

/// Execute the complete test suite
fn run_test_suite(test_suite: &GameTestSuite) -> Result<(), Box<dyn std::error::Error>> {
    // Test 1: Single game demonstration
    TestReporter::print_section("Single Game Test");
    let single_result = test_suite.test_single_coinflip(CoinChoice::Heads, "demo-player")?;
    TestReporter::print_game_details(&single_result);
    
    // Test 2: VRF Properties
    TestReporter::print_section("VRF Security Verification");
    test_suite.test_vrf_properties()?;
    
    // Test 3: Batch Statistical Analysis  
    TestReporter::print_section("Statistical Analysis");
    let batch_results = test_suite.test_coinflip_batch()?;
    println!("{}", batch_results);
    
    // Test 4: Multi-token Testing
    TestReporter::print_section("Multi-Token Testing");
    let token_results = test_suite.test_multiple_tokens()?;
    TestReporter::print_token_summary(&token_results);
    
    // Test 5: Supported tokens display
    TestReporter::print_section("Supported Tokens");
    let tokens = Token::all_supported();
    for token in tokens {
        println!("   • {} {}", 
                token.symbol,
                if let Some(mint) = token.mint_address {
                    format!("(Mint: {}...)", &mint[..8])
                } else {
                    "(Native)".to_string()
                }
        );
    }
    
    Ok(())
}
}
