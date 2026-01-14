# Casino Games Implementation

This document describes the provably fair casino game system integrated into the Atomiq blockchain.

## Overview

The casino game system uses VRF (Verifiable Random Function) proofs to ensure games are provably fair and cryptographically secure. Game results are immutably stored on the blockchain for transparency.

## Architecture

### Components

1. **VRF Engine** (`src/games/vrf_engine.rs`)

   - Uses schnorrkel (Polkadot's VRF implementation)
   - Generates cryptographic proofs for game outcomes
   - 50-100μs proof generation time
   - 96-byte proofs with full verifiability

2. **Game Processor** (`src/games/processor.rs`)

   - Processes game requests
   - Generates VRF proofs
   - Calculates payouts

3. **Pending Games Pool** (`src/games/pending_pool.rs`)

   - Thread-safe pool using DashMap
   - Manages pending games awaiting blockchain confirmation
   - Supports concurrent game submissions

4. **API Endpoints** (`src/api/games.rs`)
   - HTTP-only architecture (no WebSocket needed for simple games)
   - Timeout + polling pattern for async confirmation
   - VRF verification endpoint for transparency

## Supported Games

### Coin Flip

Simple 50/50 game where player chooses heads or tails.

**Payout**: 2x bet amount on win, 0 on loss

## API Endpoints

### 1. Play Coin Flip

```http
POST /api/coinflip/play
Content-Type: application/json

{
  "player_id": "player-123",
  "choice": "heads",
  "token": {
    "symbol": "SOL",
    "mint_address": null
  },
  "bet_amount": 1.0
}
```

**Response** (immediate if confirmed within 2s):

```json
{
  "status": "complete",
  "game_id": "abc-def-123",
  "result": {
    "game_id": "abc-def-123",
    "game_type": "coinflip",
    "outcome": "win",
    "payment": {
      "bet_amount": 1.0,
      "payout_amount": 2.0,
      "token": { "symbol": "SOL" }
    },
    "vrf": {
      "vrf_output": "a1b2c3...",
      "vrf_proof": "d4e5f6...",
      "public_key": "789abc...",
      "input_message": "abc-def-123:coinflip:player-123:heads"
    }
  }
}
```

**Response** (pending if takes >2s):

```json
{
  "status": "pending",
  "game_id": "abc-def-123",
  "message": "Game pending blockchain confirmation"
}
```

### 2. Get Game Result

```http
GET /api/game/{game_id}
```

Poll this endpoint if you received a "pending" response.

### 3. Verify VRF Proof

```http
POST /api/verify/vrf
Content-Type: application/json

{
  "vrf_output": "a1b2c3...",
  "vrf_proof": "d4e5f6...",
  "public_key": "789abc...",
  "input_message": "abc-def-123:coinflip:player-123:heads",
  "game_type": "coinflip"
}
```

**Response**:

```json
{
  "is_valid": true,
  "computed_result": {
    "game_type": "coinflip",
    "result": "heads"
  },
  "explanation": "VRF proof is cryptographically valid. The VRF output a1b2c3d4e5f6... produces the result shown above using deterministic mapping."
}
```

### 4. List Supported Tokens

```http
GET /api/tokens
```

**Response**:

```json
[
  {
    "symbol": "SOL",
    "mint_address": null
  },
  {
    "symbol": "USDC",
    "mint_address": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
  },
  {
    "symbol": "USDT",
    "mint_address": "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"
  }
]
```

## Provably Fair Verification

Anyone can verify game fairness by:

1. **Get the game result** from blockchain
2. **Extract VRF components**: output, proof, public key, input message
3. **Call verification endpoint** or verify locally
4. **Check outcome calculation**:
   - Coin Flip: Last byte of VRF output % 2 (0=Heads, 1=Tails)

## How It Works

### Game Flow

1. **Player submits game request** via API
2. **Game processor generates VRF proof**
   - Input: game_id + game_type + player_id + player_choice
   - Output: cryptographic proof + deterministic outcome
3. **Game added to pending pool**
4. **API waits 2 seconds** for blockchain confirmation
5. **On next block**:
   - DirectCommit engine includes game in block
   - Pending pool notifies waiting clients
6. **Result sent to player** (or polling retrieves it)

### VRF Process

```
Input Message: "game-123:coinflip:player-456:heads"
        ↓
    VRF Sign (using server's private key)
        ↓
VRF Output (32 bytes) + VRF Proof (64 bytes)
        ↓
    Deterministic Mapping
        ↓
    Game Result (heads/tails)
```

### Verification

```
VRF Output + VRF Proof + Public Key + Input Message
        ↓
    Verify Signature
        ↓
  Recompute Result
        ↓
    Match? ✅
```

## Security Properties

1. **Unpredictable**: Player cannot predict outcome before submission
2. **Provable**: Anyone can verify the outcome was fairly generated
3. **Immutable**: Results stored on blockchain cannot be altered
4. **Non-repudiable**: Server cannot change outcome after generation
5. **Transparent**: VRF proof enables third-party verification

## Future Enhancements

### Planned Features

1. **Solana Settlement**

   - Automatic token transfers after game conclusion
   - Transaction ID stored with game result

2. **Wallet Signatures**

   - Player authentication via Solana wallet signatures
   - Prevents unauthorized game submissions

3. **Additional Games**

   - Dice: Roll number with configurable targets
   - Plinko: Ball drop with multipliers
   - Crash: Multiplayer exponential game

4. **Game History**
   - Query player game history
   - Statistics and analytics endpoints

## Testing

Run the test suite:

```bash
# Test games module
cargo test --lib games

# Test VRF engine specifically
cargo test --lib games::vrf_engine

# Test all
cargo test --lib
```

## Performance

- **VRF Generation**: 50-100μs per game
- **Block Interval**: 10ms (configurable)
- **Expected Throughput**: Thousands of games per second
- **API Latency**: <2s for immediate confirmation, or poll for pending

## Dependencies

- `schnorrkel = "0.11"` - VRF implementation
- `dashmap = "5.5"` - Concurrent pending games pool
- `tokio` - Async runtime for timeouts

## Example Integration

```rust
use atomiq::games::*;

// Create VRF engine
let vrf_engine = VRFGameEngine::new_random();
let game_processor = GameProcessor::new(Arc::new(vrf_engine));

// Process a coin flip
let request = CoinFlipPlayRequest {
    player_id: "player-123".to_string(),
    choice: CoinChoice::Heads,
    token: Token::sol(),
    bet_amount: 1.0,
    wallet_signature: None,
};

let result = game_processor.process_coinflip(request)?;

// Verify the result
let is_valid = VRFGameEngine::verify_vrf_proof(
    &result.vrf,
    &result.vrf.input_message,
)?;

assert!(is_valid);
```

## Architecture Decisions

### Why HTTP-only (no WebSocket)?

- **Simplicity**: HTTP sufficient for request-response pattern
- **Scalability**: Easier to load balance
- **Industry standard**: Stake.com, Rollbit use HTTP for simple games
- **WebSocket reserved for**: Multiplayer games (Crash) and live chat

### Why Timeout + Polling?

- **Prevents connection exhaustion**: Don't hold connections open
- **Better concurrency**: Supports thousands of concurrent games
- **Graceful degradation**: Works even if blockchain is slow

### Why VRF instead of hashed server seed?

- **Cryptographic proof**: VRF provides verifiable randomness
- **No seed commitment needed**: Server can't change outcome after generation
- **Battle-tested**: Used by Polkadot, Algorand for consensus
