//! Casino Game Testing Framework
//! 
//! Extensible testing utilities for casino games with clean separation of concerns.
//! Designed to support multiple game types and settlement patterns.

use crate::games::*;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// Test scenario definitions for different game types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameTestScenario {
    /// Single game test with specific parameters
    SingleGame {
        game_type: GameType,
        player_id: String,
        bet_amount: f64,
        token: Token,
    },
    /// Batch testing for statistical analysis
    BatchTest {
        game_type: GameType,
        player_count: usize,
        games_per_player: usize,
        bet_amount: f64,
        token: Token,
    },
    /// Multi-token compatibility testing
    TokenCompatibility {
        game_type: GameType,
        tokens: Vec<Token>,
        games_per_token: usize,
    },
    /// Performance testing under load
    LoadTest {
        game_type: GameType,
        concurrent_players: usize,
        games_per_player: usize,
        duration: Duration,
    },
}

/// Comprehensive test results with metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComprehensiveTestResults {
    pub scenario: String,
    pub game_type: GameType,
    pub execution_time: Duration,
    pub games_played: usize,
    pub successful_games: usize,
    pub failed_games: usize,
    pub vrf_verifications: usize,
    pub total_bet_amount: f64,
    pub total_payout: f64,
    pub win_rate: f64,
    pub house_edge: f64,
    pub average_game_time: Duration,
    pub token_breakdown: HashMap<String, TokenTestResults>,
}

/// Per-token test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTestResults {
    pub games_played: usize,
    pub win_rate: f64,
    pub total_bet: f64,
    pub total_payout: f64,
    pub house_edge: f64,
}

/// Game testing framework with extensible architecture
pub struct GameTestFramework {
    game_processor: Arc<GameProcessor>,
    vrf_engine: Arc<VRFGameEngine>,
}

impl GameTestFramework {
    /// Create a new test framework instance
    pub fn new() -> Self {
        let vrf_engine = Arc::new(VRFGameEngine::new_random());
        let game_processor = Arc::new(GameProcessor::new(vrf_engine.clone()));
        
        Self {
            game_processor,
            vrf_engine,
        }
    }

    /// Execute a test scenario and return comprehensive results
    pub async fn execute_scenario(&self, scenario: GameTestScenario) -> Result<ComprehensiveTestResults, GameTestError> {
        let start_time = Instant::now();
        
        match scenario {
            GameTestScenario::SingleGame { game_type, player_id, bet_amount, token } => {
                self.execute_single_game_test(game_type, &player_id, bet_amount, token).await
            },
            GameTestScenario::BatchTest { game_type, player_count, games_per_player, bet_amount, token } => {
                self.execute_batch_test(game_type, player_count, games_per_player, bet_amount, token).await
            },
            GameTestScenario::TokenCompatibility { game_type, tokens, games_per_token } => {
                self.execute_token_compatibility_test(game_type, tokens, games_per_token).await
            },
            GameTestScenario::LoadTest { game_type, concurrent_players, games_per_player, duration } => {
                self.execute_load_test(game_type, concurrent_players, games_per_player, duration).await
            },
        }
    }

    /// Test a single game with full validation
    async fn execute_single_game_test(
        &self, 
        game_type: GameType, 
        player_id: &str, 
        bet_amount: f64, 
        token: Token
    ) -> Result<ComprehensiveTestResults, GameTestError> {
        let start_time = Instant::now();
        
        match game_type {
            GameType::CoinFlip => {
                let request = CoinFlipPlayRequest {
                    player_id: player_id.to_string(),
                    choice: CoinChoice::Heads, // Default for single test
                    bet_amount,
                    token: token.clone(),
                    wallet_signature: None,
                };
                
                let result = self.game_processor.process_coinflip(request)
                    .map_err(|e| GameTestError::GameProcessingError(e.to_string()))?;
                
                // Verify VRF proof
                let vrf_verified = VRFGameEngine::verify_vrf_proof(&result.vrf, &result.vrf.input_message)
                    .map_err(|e| GameTestError::VRFVerificationError(e.to_string()))?;
                
                if !vrf_verified {
                    return Err(GameTestError::VRFVerificationError("VRF proof validation failed".to_string()));
                }
                
                let execution_time = start_time.elapsed();
                let win_rate = if result.outcome == GameOutcome::Win { 1.0 } else { 0.0 };
                let house_edge = (bet_amount - result.payment.payout_amount) / bet_amount;
                
                let mut token_breakdown = HashMap::new();
                token_breakdown.insert(token.symbol.clone(), TokenTestResults {
                    games_played: 1,
                    win_rate,
                    total_bet: bet_amount,
                    total_payout: result.payment.payout_amount,
                    house_edge,
                });
                
                Ok(ComprehensiveTestResults {
                    scenario: "SingleGame".to_string(),
                    game_type,
                    execution_time,
                    games_played: 1,
                    successful_games: 1,
                    failed_games: 0,
                    vrf_verifications: 1,
                    total_bet_amount: bet_amount,
                    total_payout: result.payment.payout_amount,
                    win_rate,
                    house_edge,
                    average_game_time: execution_time,
                    token_breakdown,
                })
            },
            _ => Err(GameTestError::UnsupportedGameType(format!("{:?}", game_type))),
        }
    }

    /// Execute batch testing for statistical analysis
    async fn execute_batch_test(
        &self,
        game_type: GameType,
        player_count: usize,
        games_per_player: usize,
        bet_amount: f64,
        token: Token,
    ) -> Result<ComprehensiveTestResults, GameTestError> {
        let start_time = Instant::now();
        let total_games = player_count * games_per_player;
        
        let mut successful_games = 0;
        let mut failed_games = 0;
        let mut vrf_verifications = 0;
        let mut total_bet = 0.0;
        let mut total_payout = 0.0;
        let mut wins = 0;
        let mut game_times = Vec::new();

        for player_idx in 0..player_count {
            let player_id = format!("test-player-{}", player_idx);
            
            for game_idx in 0..games_per_player {
                let game_start = Instant::now();
                
                match game_type {
                    GameType::CoinFlip => {
                        let choice = if game_idx % 2 == 0 { CoinChoice::Heads } else { CoinChoice::Tails };
                        let request = CoinFlipPlayRequest {
                            player_id: format!("{}-game-{}", player_id, game_idx),
                            choice,
                            bet_amount,
                            token: token.clone(),
                            wallet_signature: None,
                        };
                        
                        match self.game_processor.process_coinflip(request) {
                            Ok(result) => {
                                successful_games += 1;
                                total_bet += bet_amount;
                                total_payout += result.payment.payout_amount;
                                
                                if result.outcome == GameOutcome::Win {
                                    wins += 1;
                                }
                                
                                // Verify VRF
                                if VRFGameEngine::verify_vrf_proof(&result.vrf, &result.vrf.input_message).unwrap_or(false) {
                                    vrf_verifications += 1;
                                }
                            },
                            Err(_) => {
                                failed_games += 1;
                            }
                        }
                    },
                    _ => return Err(GameTestError::UnsupportedGameType(format!("{:?}", game_type))),
                }
                
                game_times.push(game_start.elapsed());
            }
        }

        let execution_time = start_time.elapsed();
        let win_rate = if successful_games > 0 { wins as f64 / successful_games as f64 } else { 0.0 };
        let house_edge = if total_bet > 0.0 { (total_bet - total_payout) / total_bet } else { 0.0 };
        let average_game_time = if !game_times.is_empty() { 
            game_times.iter().sum::<Duration>() / game_times.len() as u32 
        } else { 
            Duration::default() 
        };

        let mut token_breakdown = HashMap::new();
        token_breakdown.insert(token.symbol.clone(), TokenTestResults {
            games_played: successful_games,
            win_rate,
            total_bet,
            total_payout,
            house_edge,
        });

        Ok(ComprehensiveTestResults {
            scenario: "BatchTest".to_string(),
            game_type,
            execution_time,
            games_played: total_games,
            successful_games,
            failed_games,
            vrf_verifications,
            total_bet_amount: total_bet,
            total_payout,
            win_rate,
            house_edge,
            average_game_time,
            token_breakdown,
        })
    }

    /// Test compatibility across different tokens
    async fn execute_token_compatibility_test(
        &self,
        game_type: GameType,
        tokens: Vec<Token>,
        games_per_token: usize,
    ) -> Result<ComprehensiveTestResults, GameTestError> {
        let start_time = Instant::now();
        
        let mut token_breakdown = HashMap::new();
        let mut total_games = 0;
        let mut successful_games = 0;
        let mut failed_games = 0;
        let mut vrf_verifications = 0;
        let mut total_bet_amount = 0.0;
        let mut total_payout = 0.0;
        let mut total_wins = 0;
        let mut game_times = Vec::new();

        for token in tokens {
            let batch_result = self.execute_batch_test(
                game_type,
                1, // Single player per token
                games_per_token,
                1.0, // Standard bet amount
                token.clone(),
            ).await?;
            
            total_games += batch_result.games_played;
            successful_games += batch_result.successful_games;
            failed_games += batch_result.failed_games;
            vrf_verifications += batch_result.vrf_verifications;
            total_bet_amount += batch_result.total_bet_amount;
            total_payout += batch_result.total_payout;
            total_wins += (batch_result.win_rate * batch_result.successful_games as f64) as usize;
            
            if let Some(token_result) = batch_result.token_breakdown.get(&token.symbol) {
                token_breakdown.insert(token.symbol.clone(), token_result.clone());
            }
        }

        let execution_time = start_time.elapsed();
        let win_rate = if successful_games > 0 { total_wins as f64 / successful_games as f64 } else { 0.0 };
        let house_edge = if total_bet_amount > 0.0 { (total_bet_amount - total_payout) / total_bet_amount } else { 0.0 };
        let average_game_time = execution_time / total_games as u32;

        Ok(ComprehensiveTestResults {
            scenario: "TokenCompatibility".to_string(),
            game_type,
            execution_time,
            games_played: total_games,
            successful_games,
            failed_games,
            vrf_verifications,
            total_bet_amount,
            total_payout,
            win_rate,
            house_edge,
            average_game_time,
            token_breakdown,
        })
    }

    /// Execute load testing with concurrent players
    async fn execute_load_test(
        &self,
        game_type: GameType,
        concurrent_players: usize,
        games_per_player: usize,
        _duration: Duration,
    ) -> Result<ComprehensiveTestResults, GameTestError> {
        // For now, simulate load test with batch test
        // In a real implementation, this would use tokio tasks for concurrency
        self.execute_batch_test(
            game_type,
            concurrent_players,
            games_per_player,
            1.0,
            Token::sol(),
        ).await
    }
}

/// Test framework error types
#[derive(Debug, thiserror::Error)]
pub enum GameTestError {
    #[error("Game processing failed: {0}")]
    GameProcessingError(String),
    
    #[error("VRF verification failed: {0}")]
    VRFVerificationError(String),
    
    #[error("Unsupported game type: {0}")]
    UnsupportedGameType(String),
    
    #[error("Test execution failed: {0}")]
    TestExecutionError(String),
}

/// Test result reporting utilities
pub struct TestResultReporter;

impl TestResultReporter {
    /// Generate a comprehensive test report
    pub fn generate_report(results: &ComprehensiveTestResults) -> String {
        let mut report = String::new();
        
        report.push_str(&format!("🎯 {} Test Results for {:?}\n", results.scenario, results.game_type));
        report.push_str(&format!("{}\n", "=".repeat(50)));
        report.push_str(&format!("⏱️  Execution Time: {:?}\n", results.execution_time));
        report.push_str(&format!("🎮 Games: {} played, {} successful, {} failed\n", 
                                results.games_played, results.successful_games, results.failed_games));
        report.push_str(&format!("🏆 Win Rate: {:.1}%\n", results.win_rate * 100.0));
        report.push_str(&format!("🏠 House Edge: {:.2}%\n", results.house_edge * 100.0));
        report.push_str(&format!("💰 Total Bet: {:.2}, Total Payout: {:.2}\n", 
                                results.total_bet_amount, results.total_payout));
        report.push_str(&format!("🔐 VRF Verifications: {}\n", results.vrf_verifications));
        report.push_str(&format!("⚡ Avg Game Time: {:?}\n", results.average_game_time));
        
        if !results.token_breakdown.is_empty() {
            report.push_str("\n🪙 Token Breakdown:\n");
            for (token, token_results) in &results.token_breakdown {
                report.push_str(&format!("   {} - {} games, {:.1}% wins, {:.2}% house edge\n",
                                       token, token_results.games_played, 
                                       token_results.win_rate * 100.0,
                                       token_results.house_edge * 100.0));
            }
        }
        
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_single_game_scenario() {
        let framework = GameTestFramework::new();
        let scenario = GameTestScenario::SingleGame {
            game_type: GameType::CoinFlip,
            player_id: "test-player".to_string(),
            bet_amount: 1.0,
            token: Token::sol(),
        };
        
        let result = framework.execute_scenario(scenario).await;
        assert!(result.is_ok());
        
        let test_result = result.unwrap();
        assert_eq!(test_result.games_played, 1);
        assert_eq!(test_result.successful_games, 1);
        assert_eq!(test_result.vrf_verifications, 1);
    }
    
    #[tokio::test] 
    async fn test_batch_scenario() {
        let framework = GameTestFramework::new();
        let scenario = GameTestScenario::BatchTest {
            game_type: GameType::CoinFlip,
            player_count: 2,
            games_per_player: 5,
            bet_amount: 1.0,
            token: Token::sol(),
        };
        
        let result = framework.execute_scenario(scenario).await;
        assert!(result.is_ok());
        
        let test_result = result.unwrap();
        assert_eq!(test_result.games_played, 10);
        assert!(test_result.win_rate >= 0.0 && test_result.win_rate <= 1.0);
    }
}