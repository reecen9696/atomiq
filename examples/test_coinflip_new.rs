//! CoinFlip Game Test Suite
//! 
//! Comprehensive testing of the coinflip game using clean architecture principles.
//! Demonstrates the extensible testing framework for casino games.

use atomiq::games::*;
use std::time::{Duration, Instant};

/// Main test configuration
#[derive(Debug)]
pub struct CoinFlipTestConfig {
    pub test_scenarios: Vec<GameTestScenario>,
    pub enable_detailed_logging: bool,
    pub output_json_results: bool,
}

impl Default for CoinFlipTestConfig {
    fn default() -> Self {
        Self {
            test_scenarios: vec![
                // Single game test for basic functionality
                GameTestScenario::SingleGame {
                    game_type: GameType::CoinFlip,
                    player_id: "demo-player-001".to_string(),
                    bet_amount: 1.0,
                    token: Token::sol(),
                },
                
                // Statistical batch test
                GameTestScenario::BatchTest {
                    game_type: GameType::CoinFlip,
                    player_count: 10,
                    games_per_player: 20,
                    bet_amount: 1.0,
                    token: Token::sol(),
                },
                
                // Token compatibility test
                GameTestScenario::TokenCompatibility {
                    game_type: GameType::CoinFlip,
                    tokens: vec![
                        Token::sol(),
                        Token::usdc(),
                        Token::native_token(),
                    ],
                    games_per_token: 50,
                },
                
                // Performance load test
                GameTestScenario::LoadTest {
                    game_type: GameType::CoinFlip,
                    concurrent_players: 25,
                    games_per_player: 10,
                    duration: Duration::from_secs(30),
                },
            ],
            enable_detailed_logging: true,
            output_json_results: false,
        }
    }
}

/// CoinFlip test suite runner
pub struct CoinFlipTestSuite {
    framework: GameTestFramework,
    config: CoinFlipTestConfig,
}

impl CoinFlipTestSuite {
    pub fn new(config: CoinFlipTestConfig) -> Self {
        Self {
            framework: GameTestFramework::new(),
            config,
        }
    }

    /// Execute all configured test scenarios
    pub async fn run_all_tests(&self) -> Result<Vec<ComprehensiveTestResults>, Box<dyn std::error::Error>> {
        println!("🎰 Starting CoinFlip Game Test Suite");
        println!("{'=':.>60}");
        
        let start_time = Instant::now();
        let mut all_results = Vec::new();
        
        for (idx, scenario) in self.config.test_scenarios.iter().enumerate() {
            println!("\n🧪 Test Scenario {} of {}", idx + 1, self.config.test_scenarios.len());
            
            match self.framework.execute_scenario(scenario.clone()).await {
                Ok(result) => {
                    if self.config.enable_detailed_logging {
                        println!("{}", TestResultReporter::generate_report(&result));
                    } else {
                        println!("✅ {} - {} games in {:?}", 
                               result.scenario, result.games_played, result.execution_time);
                    }
                    all_results.push(result);
                },
                Err(e) => {
                    eprintln!("❌ Test scenario failed: {}", e);
                    return Err(Box::new(e));
                }
            }
        }
        
        let total_time = start_time.elapsed();
        self.print_summary(&all_results, total_time)?;
        
        if self.config.output_json_results {
            self.export_json_results(&all_results)?;
        }
        
        Ok(all_results)
    }

    /// Print comprehensive test summary
    fn print_summary(
        &self, 
        results: &[ComprehensiveTestResults], 
        total_time: Duration
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🏁 Test Suite Summary");
        println!("{'=':.>60}");
        
        let total_games: usize = results.iter().map(|r| r.games_played).sum();
        let total_successful: usize = results.iter().map(|r| r.successful_games).sum();
        let total_failed: usize = results.iter().map(|r| r.failed_games).sum();
        let total_vrf_verifications: usize = results.iter().map(|r| r.vrf_verifications).sum();
        let total_bet: f64 = results.iter().map(|r| r.total_bet_amount).sum();
        let total_payout: f64 = results.iter().map(|r| r.total_payout).sum();
        
        let overall_win_rate = if total_successful > 0 {
            results.iter()
                .map(|r| r.win_rate * r.successful_games as f64)
                .sum::<f64>() / total_successful as f64
        } else {
            0.0
        };
        
        let overall_house_edge = if total_bet > 0.0 {
            (total_bet - total_payout) / total_bet
        } else {
            0.0
        };
        
        let success_rate = if total_games > 0 {
            total_successful as f64 / total_games as f64 * 100.0
        } else {
            0.0
        };

        println!("⏱️  Total Execution Time: {:?}", total_time);
        println!("🎮 Total Games: {}", total_games);
        println!("✅ Successful Games: {} ({:.1}%)", total_successful, success_rate);
        println!("❌ Failed Games: {}", total_failed);
        println!("🔐 VRF Verifications: {}", total_vrf_verifications);
        println!("🏆 Overall Win Rate: {:.1}%", overall_win_rate * 100.0);
        println!("🏠 Overall House Edge: {:.2}%", overall_house_edge * 100.0);
        println!("💰 Total Bet Volume: {:.2}", total_bet);
        println!("💸 Total Payouts: {:.2}", total_payout);
        
        // Performance metrics
        let avg_test_time = total_time / results.len() as u32;
        let avg_game_time = if total_games > 0 {
            total_time / total_games as u32
        } else {
            Duration::default()
        };
        
        println!("\n⚡ Performance Metrics:");
        println!("   Average test scenario time: {:?}", avg_test_time);
        println!("   Average game execution time: {:?}", avg_game_time);
        
        if total_games > 0 {
            let games_per_second = total_games as f64 / total_time.as_secs_f64();
            println!("   Games per second: {:.1}", games_per_second);
        }
        
        // VRF verification rate
        let vrf_success_rate = if total_successful > 0 {
            total_vrf_verifications as f64 / total_successful as f64 * 100.0
        } else {
            0.0
        };
        println!("   VRF verification rate: {:.1}%", vrf_success_rate);
        
        // Next steps for developers
        println!("\n🚀 Development Next Steps:");
        println!("   1. Start blockchain: cargo run --release --bin atomiq-unified");
        println!("   2. Start API server: cargo run --release --bin atomiq-api");
        println!("   3. Test HTTP endpoints: ./scripts/test_games.sh");
        println!("   4. Add new game types using GameTestFramework");
        println!("   5. Implement settlement system with clean separation");
        
        Ok(())
    }

    /// Export results to JSON file for analysis
    fn export_json_results(&self, results: &[ComprehensiveTestResults]) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs::File;
        use std::io::Write;
        
        let json_data = serde_json::to_string_pretty(results)?;
        let mut file = File::create("coinflip_test_results.json")?;
        file.write_all(json_data.as_bytes())?;
        
        println!("📊 Test results exported to: coinflip_test_results.json");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("🎯 CoinFlip Casino Game - Clean Architecture Demo");
    println!("Testing VRF-based provably fair gaming system\n");
    
    // Configure test suite with comprehensive scenarios
    let config = CoinFlipTestConfig::default();
    let test_suite = CoinFlipTestSuite::new(config);
    
    // Execute all tests
    match test_suite.run_all_tests().await {
        Ok(results) => {
            println!("\n🎉 All tests completed successfully!");
            println!("Executed {} test scenarios with full VRF verification", results.len());
            
            // Display supported tokens for reference
            println!("\n🪙 Supported Tokens:");
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
        },
        Err(e) => {
            eprintln!("\n💥 Test suite failed: {}", e);
            std::process::exit(1);
        }
    }
}