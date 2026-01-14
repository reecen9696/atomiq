use crate::games::types::GameResult;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Pending game with a channel to send the result back
pub struct PendingGame {
    pub game_id: String,
    pub sender: oneshot::Sender<GameResult>,
}

/// Thread-safe pool of pending games waiting for blockchain confirmation
pub struct PendingGamesPool {
    /// Map of game_id -> oneshot sender
    pending: Arc<DashMap<String, oneshot::Sender<GameResult>>>,
}

impl PendingGamesPool {
    /// Create a new pending games pool
    pub fn new() -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
        }
    }

    /// Add a pending game to the pool
    pub fn add_pending(&self, game_id: String, sender: oneshot::Sender<GameResult>) {
        self.pending.insert(game_id, sender);
    }

    /// Complete a game and send result to waiting client
    pub fn complete_game(&self, game_id: &str, result: GameResult) -> bool {
        if let Some((_, sender)) = self.pending.remove(game_id) {
            // Send result (ignore if receiver dropped)
            let _ = sender.send(result);
            true
        } else {
            false
        }
    }

    /// Get number of pending games
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Remove a pending game (e.g., on timeout)
    pub fn remove_pending(&self, game_id: &str) -> bool {
        self.pending.remove(game_id).is_some()
    }

    /// Check if a game is pending
    pub fn is_pending(&self, game_id: &str) -> bool {
        self.pending.contains_key(game_id)
    }
}

impl Default for PendingGamesPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::types::{
        CoinChoice, GameData, GameOutcome, GameType, PaymentInfo, PlayerInfo, Token, VRFBundle,
    };

    fn create_test_result(game_id: String) -> GameResult {
        GameResult {
            game_id: game_id.clone(),
            game_type: GameType::CoinFlip,
            player: PlayerInfo {
                player_id: "test-player".to_string(),
                wallet_signature: None,
            },
            payment: PaymentInfo {
                token: Token::sol(),
                bet_amount: 1.0,
                payout_amount: 2.0,
                settlement_tx_id: None,
            },
            vrf: VRFBundle {
                vrf_output: "test".to_string(),
                vrf_proof: "test".to_string(),
                public_key: "test".to_string(),
                input_message: "test".to_string(),
            },
            outcome: GameOutcome::Win,
            timestamp: 0,
            game_data: GameData::CoinFlip {
                player_choice: CoinChoice::Heads,
                result_choice: CoinChoice::Heads,
            },
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_pending_pool() {
        let pool = PendingGamesPool::new();
        let (tx, rx) = oneshot::channel();

        pool.add_pending("game-1".to_string(), tx);
        assert_eq!(pool.pending_count(), 1);
        assert!(pool.is_pending("game-1"));

        let result = create_test_result("game-1".to_string());
        assert!(pool.complete_game("game-1", result.clone()));

        let received = rx.await.expect("Should receive result");
        assert_eq!(received.game_id, "game-1");
        assert_eq!(pool.pending_count(), 0);
    }

    #[test]
    fn test_remove_pending() {
        let pool = PendingGamesPool::new();
        let (tx, _rx) = oneshot::channel();

        pool.add_pending("game-1".to_string(), tx);
        assert!(pool.remove_pending("game-1"));
        assert!(!pool.is_pending("game-1"));
    }
}
