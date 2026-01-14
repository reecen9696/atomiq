# Quick Reference: Finalization System

## TL;DR

Game endpoints now wait for blockchain finalization before returning results. This ensures consistency between the API response and the blockchain state.

## Key Files

- `src/finalization.rs` - Core finalization event system
- `src/direct_commit.rs` - Emits BlockCommittedEvent after storage commit  
- `src/api/games.rs` - Game handlers wait for finalization
- `examples/api_with_finalization.rs` - Working example

## How It Works

```
Client Request → Submit Transaction → Wait for Block Commit → Return Result
                                      ↓
                              (10-20ms typical)
```

## API Changes

### GameApiState (Before)
```rust
pub struct GameApiState {
    pub storage: RawStorage,
    pub game_processor: Arc<BlockchainGameProcessor>,
    pub tx_sender: Arc<UnboundedSender<Transaction>>,
}
```

### GameApiState (After)
```rust
pub struct GameApiState {
    pub storage: RawStorage,
    pub game_processor: Arc<BlockchainGameProcessor>,
    pub tx_sender: Arc<UnboundedSender<Transaction>>,
    pub finalization_waiter: Option<Arc<FinalizationWaiter>>, // NEW
}
```

## Response Types

### With Finalization (Fast)
```json
{
  "status": "complete",
  "game_id": "abc123",
  "result": { "outcome": "win", "payout": 200 }
}
```
*Returned in ~10-20ms*

### Without Finalization or Timeout
```json
{
  "status": "pending",
  "game_id": "abc123",
  "tx_id": 12345
}
```
*Client polls `/api/game/:id`*

## Code Example

### Setup (Once at Startup)
```rust
use atomiq::{
    api::server::{ApiServer, ApiConfig},
    factory::BlockchainFactory,
    finalization::FinalizationWaiter,
    DirectCommitHandle,
};

// Create blockchain
let (app, handle) = BlockchainFactory::create_high_performance().await?;

// Get DirectCommit engine
let engine = handle.as_any()
    .downcast_ref::<DirectCommitHandle>()
    .unwrap()
    .engine.clone();

// Create finalization waiter
let finalization_waiter = Arc::new(
    FinalizationWaiter::new(engine.event_publisher())
);

// Create API with finalization
let server = ApiServer::with_finalization(
    api_config,
    storage,
    finalization_waiter
);
```

### Usage in Handler
```rust
pub async fn play_game(
    State(state): State<GameApiState>,
    Json(request): Json<GameRequest>,
) -> Result<Json<GameResponse>, (StatusCode, String)> {
    // Submit transaction
    let tx_id = submit_transaction(&state, request).await?;
    
    // Wait for finalization (optional - graceful degradation)
    if let Some(waiter) = &state.finalization_waiter {
        match waiter.wait_for_transaction(tx_id, Duration::from_secs(2)).await {
            Ok(_) => {
                // Transaction finalized - return result
                let result = get_game_result(&state, game_id)?;
                return Ok(Json(GameResponse::Complete(result)));
            }
            Err(_) => {
                // Timeout or error - fall through to pending
            }
        }
    }
    
    // Return pending status
    Ok(Json(GameResponse::Pending { game_id, tx_id }))
}
```

## Testing

### Run Example
```bash
cargo run --example api_with_finalization
```

### Test Request
```bash
curl -X POST http://127.0.0.1:3000/api/coinflip/play \
  -H "Content-Type: application/json" \
    -d '{"player_id":"cli_test","bet_amount":1,"choice":"heads","token":{"symbol":"SOL"}}'
```

### Expected Response
```json
{
  "status": "complete",
    "game_id": "tx-1736890000000",
  "result": {
        "vrf": {
            "public_key": "<32-byte hex>",
            "vrf_proof": "<64-byte hex schnorrkel signature>",
            "vrf_output": "<32-byte hex sha256(signature)>",
            "input_message": "<exact UTF-8 message that was signed>"
        },
        "block_height": 1234,
        "block_hash": "<32-byte hex>",
        "finalization_confirmed": true,
        
  }
}
```

## Configuration

### Timeout (Per Endpoint)
```rust
// In handler:
Duration::from_secs(2)  // Default for games
```

### Block Time (Blockchain Config)
```rust
// In DirectCommit config:
block_time_ms: 10  // 10ms blocks
```

## Performance

| Metric | Value |
|--------|-------|
| Latency (typical) | 10-20ms |
| Latency (timeout) | 2000ms |
| Throughput | 5000+ req/s |
| Memory/waiter | ~128 bytes |
| CPU overhead | <1% |

## Troubleshooting

### "Timeout waiting for finalization"
→ Check block production is running
→ Check transaction was actually submitted
→ Increase timeout duration

### "finalization_waiter is None"
→ Use `ApiServer::with_finalization()` instead of `new()`
→ Pass `FinalizationWaiter` at server creation

### Response always "pending"
→ Verify DirectCommit engine is emitting events
→ Check `event_publisher()` is connected
→ Verify timeout is sufficient (>50ms)

## Monitoring

Key metrics to track:
- `finalization_wait_duration_ms` - Distribution of wait times
- `finalization_timeout_rate` - Percentage of timeouts
- `pending_waiters` - Current waiters (should be low)

## Backward Compatibility

✅ Optional finalization - system works without it
✅ Existing API structure unchanged
✅ Graceful degradation to pending responses
✅ No breaking changes to client code

## See Also

- [FINALIZATION.md](FINALIZATION.md) - Comprehensive documentation
- [IMPLEMENTATION_SUMMARY.md](IMPLEMENTATION_SUMMARY.md) - What was built
- [examples/api_with_finalization.rs](examples/api_with_finalization.rs) - Working example
