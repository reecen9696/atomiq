use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported game types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum GameType {
    CoinFlip,
    // Future games: Dice, Crash, Plinko, etc.
}

impl fmt::Display for GameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameType::CoinFlip => write!(f, "coinflip"),
        }
    }
}

/// Solana token with mint address
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Token {
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint_address: Option<String>,
}

impl Token {
    /// Native SOL token
    pub fn sol() -> Self {
        Self {
            symbol: "SOL".to_string(),
            mint_address: None,
        }
    }

    /// USDC SPL token
    pub fn usdc() -> Self {
        Self {
            symbol: "USDC".to_string(),
            mint_address: Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()),
        }
    }

    /// USDT SPL token
    pub fn usdt() -> Self {
        Self {
            symbol: "USDT".to_string(),
            mint_address: Some("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string()),
        }
    }

    /// List of all supported tokens
    pub fn all_supported() -> Vec<Self> {
        vec![Self::sol(), Self::usdc(), Self::usdt()]
    }
}

/// Coin flip choice
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CoinChoice {
    Heads,
    Tails,
}

impl fmt::Display for CoinChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoinChoice::Heads => write!(f, "heads"),
            CoinChoice::Tails => write!(f, "tails"),
        }
    }
}

/// Game outcome
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GameOutcome {
    Win,
    Loss,
}

/// Coin flip result (what the coin landed on)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CoinFlipResult {
    Heads,
    Tails,
}

/// VRF bundle containing cryptographic proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VRFBundle {
    /// Hex-encoded VRF output (32 bytes)
    pub vrf_output: String,
    /// Hex-encoded VRF proof (96 bytes for schnorrkel)
    pub vrf_proof: String,
    /// Hex-encoded public key (32 bytes)
    pub public_key: String,
    /// Input message used for VRF
    pub input_message: String,
}

/// Payment information for the game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentInfo {
    pub token: Token,
    pub bet_amount: f64,
    pub payout_amount: f64,
    /// Future: Solana transaction ID after settlement
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_tx_id: Option<String>,
}

/// Player information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    /// Player identifier (wallet address or session ID)
    pub player_id: String,
    /// Future: Solana wallet signature for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_signature: Option<String>,
}

/// Complete game result stored on blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResult {
    pub game_id: String,
    pub game_type: GameType,
    pub player: PlayerInfo,
    pub payment: PaymentInfo,
    pub vrf: VRFBundle,
    pub outcome: GameOutcome,
    pub timestamp: u64,
    
    // Game-specific data
    #[serde(flatten)]
    pub game_data: GameData,
    
    /// Extensible metadata for future features
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Game-specific data (discriminated union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "game_type_data", rename_all = "lowercase")]
pub enum GameData {
    CoinFlip {
        player_choice: CoinChoice,
        result_choice: CoinChoice,
    },
    // Future: Dice { target: u8, roll: u8 }
}

impl GameResult {
    /// Create a coin flip game result
    pub fn coin_flip(
        game_id: String,
        player: PlayerInfo,
        payment: PaymentInfo,
        vrf: VRFBundle,
        player_choice: CoinChoice,
        result_choice: CoinChoice,
        outcome: GameOutcome,
        timestamp: u64,
    ) -> Self {
        Self {
            game_id,
            game_type: GameType::CoinFlip,
            player,
            payment,
            vrf,
            outcome,
            timestamp,
            game_data: GameData::CoinFlip {
                player_choice,
                result_choice,
            },
            metadata: None,
        }
    }
}

/// Request to play coin flip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinFlipPlayRequest {
    pub player_id: String,
    pub choice: CoinChoice,
    pub token: Token,
    pub bet_amount: f64,
    /// Future: wallet signature for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_signature: Option<String>,
}

/// Response for game play (can be immediate or pending)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum GameResponse {
    Complete {
        game_id: String,
        result: GameResult,
    },
    Pending {
        game_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Request to verify VRF proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyVRFRequest {
    pub vrf_output: String,
    pub vrf_proof: String,
    pub public_key: String,
    pub input_message: String,
    pub game_type: GameType,
}

/// Response from VRF verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyVRFResponse {
    pub is_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub computed_result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}
