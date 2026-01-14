use crate::games::types::{
    CoinChoice, CoinFlipPlayRequest, GameData, GameOutcome, GameResult, GameType, PaymentInfo,
    PlayerInfo,
};
use crate::games::vrf_engine::VRFGameEngine;
use std::sync::Arc;
use uuid::Uuid;

/// Processes game logic and generates results
pub struct GameProcessor {
    vrf_engine: Arc<VRFGameEngine>,
}

impl GameProcessor {
    /// Create a new game processor
    pub fn new(vrf_engine: Arc<VRFGameEngine>) -> Self {
        Self { vrf_engine }
    }

    /// Process a game request and generate result
    pub fn process_game(
        &self,
        game_type: GameType,
        _player_id: String,
        game_data: serde_json::Value,
    ) -> Result<GameResult, String> {
        match game_type {
            GameType::CoinFlip => {
                let request: CoinFlipPlayRequest = serde_json::from_value(game_data)
                    .map_err(|e| format!("Invalid coin flip data: {}", e))?;
                self.process_coinflip(request)
            }
        }
    }

    /// Process a coin flip game
    pub fn process_coinflip(&self, request: CoinFlipPlayRequest) -> Result<GameResult, String> {
        // Generate unique game ID
        let game_id = Uuid::new_v4().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate VRF proof
        let additional_data = format!("{}", request.choice);
        let vrf_bundle = self.vrf_engine.generate_outcome(
            &game_id,
            GameType::CoinFlip,
            &request.player_id,
            &additional_data,
        )?;

        // Compute result from VRF output
        let vrf_output = hex::decode(&vrf_bundle.vrf_output)
            .map_err(|e| format!("Invalid VRF output: {}", e))?;
        let result_choice = VRFGameEngine::compute_coinflip(&vrf_output);

        // Determine outcome
        let outcome = if request.choice == result_choice {
            GameOutcome::Win
        } else {
            GameOutcome::Loss
        };

        // Calculate payout (2x bet for win, 0 for loss)
        let payout_amount = if outcome == GameOutcome::Win {
            request.bet_amount * 2.0
        } else {
            0.0
        };

        // Create player info
        let player = PlayerInfo {
            player_id: request.player_id,
            wallet_signature: request.wallet_signature,
        };

        // Create payment info
        let payment = PaymentInfo {
            token: request.token,
            bet_amount: request.bet_amount,
            payout_amount,
            settlement_tx_id: None, // Will be added after Solana settlement
        };

        // Create game result
        Ok(GameResult::coin_flip(
            game_id,
            player,
            payment,
            vrf_bundle,
            request.choice,
            result_choice,
            outcome,
            timestamp,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::types::Token;

    #[test]
    fn test_process_coinflip() {
        let vrf_engine = Arc::new(VRFGameEngine::new_random());
        let processor = GameProcessor::new(vrf_engine);

        let request = CoinFlipPlayRequest {
            player_id: "test-player".to_string(),
            choice: CoinChoice::Heads,
            token: Token::sol(),
            bet_amount: 1.0,
            wallet_signature: None,
        };

        let result = processor.process_coinflip(request).expect("Processing failed");

        // Verify result structure
        assert_eq!(result.game_type, GameType::CoinFlip);
        assert_eq!(result.player.player_id, "test-player");
        assert_eq!(result.payment.bet_amount, 1.0);

        // Verify payout matches outcome
        match result.outcome {
            GameOutcome::Win => assert_eq!(result.payment.payout_amount, 2.0),
            GameOutcome::Loss => assert_eq!(result.payment.payout_amount, 0.0),
        }

        // Verify VRF proof can be verified
        let is_valid = VRFGameEngine::verify_vrf_proof(
            &result.vrf,
            &result.vrf.input_message,
        )
        .expect("Verification failed");
        assert!(is_valid);
    }

    #[test]
    fn test_multiple_games_different_results() {
        let vrf_engine = Arc::new(VRFGameEngine::new_random());
        let processor = GameProcessor::new(vrf_engine);

        let mut results = Vec::new();
        for i in 0..10 {
            let request = CoinFlipPlayRequest {
                player_id: format!("player-{}", i),
                choice: CoinChoice::Heads,
                token: Token::sol(),
                bet_amount: 1.0,
                wallet_signature: None,
            };
            results.push(processor.process_coinflip(request).unwrap());
        }

        // Verify all games have unique IDs
        let unique_ids: std::collections::HashSet<_> =
            results.iter().map(|r| &r.game_id).collect();
        assert_eq!(unique_ids.len(), 10);

        // Verify VRF proofs are all different
        let unique_proofs: std::collections::HashSet<_> =
            results.iter().map(|r| &r.vrf.vrf_proof).collect();
        assert_eq!(unique_proofs.len(), 10);
    }
}
