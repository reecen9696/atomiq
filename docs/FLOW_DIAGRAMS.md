# Finalization Flow Diagram

## Complete Request Flow

```
┌──────────────────────────────────────────────────────────────────────────┐
│                         CLIENT (HTTP REQUEST)                             │
│  POST /api/coinflip/play                                                  │
│  { "bet_amount": 100, "coin_choice": "Heads" }                           │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                       API HANDLER (games.rs)                              │
│  1. Validate request                                                      │
│  2. Create game transaction                                               │
│  3. Submit to transaction pool                                            │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                     TRANSACTION POOL (Mempool)                            │
│  - Queues transaction                                                     │
│  - Returns transaction ID                                                 │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                   DIRECTCOMMIT ENGINE (Consensus)                         │
│  - Collects transactions every 10ms                                       │
│  - Creates new block                                                      │
│  - Computes block hash                                                    │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                    STORAGE LAYER (RocksDB)                                │
│  - Persists block to disk                                                 │
│  - Updates blockchain state                                               │
│  - Commits transaction                                                    │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│               EVENT EMISSION (BlockCommittedEvent)                        │
│  {                                                                        │
│    block_height: 1234,                                                    │
│    block_hash: [0xAB, ...],                                              │
│    transactions: [tx_100, tx_101, ...],                                  │
│    timestamp: 1704067200000                                              │
│  }                                                                        │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              │ Broadcast via tokio::broadcast
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                   FINALIZATION WAITER (Listener)                          │
│  - Receives broadcast event                                               │
│  - Checks if transaction ID matches                                       │
│  - Resolves oneshot channel                                              │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                    API HANDLER (games.rs)                                 │
│  - Wait completes (10-20ms elapsed)                                       │
│  - Retrieves game result from processor                                   │
│  - Constructs response                                                    │
└─────────────────────────────┬────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                      CLIENT (HTTP RESPONSE)                               │
│  {                                                                        │
│    "status": "complete",                                                  │
│    "game_id": "abc123",                                                   │
│    "result": {                                                            │
│      "outcome": "win",                                                    │
│      "payout": 200,                                                       │
│      "vrf_proof": "0x..."                                                 │
│    }                                                                      │
│  }                                                                        │
└──────────────────────────────────────────────────────────────────────────┘

Total Time: ~10-20ms
```

## Timeout Scenario

```
┌─────────────────────┐
│   API Handler       │
│   Waits 2 seconds   │
└──────────┬──────────┘
           │
           │ No event received
           │ within timeout
           │
           ▼
┌─────────────────────┐
│  Return Pending     │
│  {                  │
│    status: pending  │
│    game_id: "123"   │
│    tx_id: 100       │
│  }                  │
└─────────────────────┘
           │
           ▼
┌─────────────────────┐
│  Client Polls       │
│  GET /api/game/123  │
└─────────────────────┘
```

## Parallel Requests

```
Request 1 ───┐
             ├──▶ Transaction Pool ──▶ DirectCommit ──▶ Block N
Request 2 ───┤
             │
Request 3 ───┘
     │
     │ All wait on FinalizationWaiter
     │
     ▼
┌──────────────────────────────────┐
│     BlockCommittedEvent          │
│     (Broadcast to all waiters)   │
└──────────────────────────────────┘
     │
     ├──▶ Request 1 completes
     ├──▶ Request 2 completes
     └──▶ Request 3 completes

Total Time: Same as single request (~10-20ms)
Scalability: O(1) - all waiters notified simultaneously
```

## Event Channel Architecture

```
┌─────────────────────┐
│ DirectCommitEngine  │
│                     │
│  event_publisher    │─────┐
│  (Sender)           │     │
└─────────────────────┘     │
                            │ tokio::broadcast::channel(1000)
                            │
        ┌───────────────────┼───────────────────┬───────────────┐
        │                   │                   │               │
        ▼                   ▼                   ▼               ▼
┌────────────┐      ┌────────────┐      ┌────────────┐  ┌────────────┐
│  Waiter 1  │      │  Waiter 2  │      │  Waiter 3  │  │  Waiter N  │
│  (tx: 100) │      │  (tx: 101) │      │  (tx: 102) │  │  (tx: N)   │
└────────────┘      └────────────┘      └────────────┘  └────────────┘
     │                   │                   │               │
     │ Match tx_id       │ Match tx_id       │ No match      │ Match tx_id
     ▼                   ▼                   │               ▼
┌────────────┐      ┌────────────┐          │          ┌────────────┐
│ Complete   │      │ Complete   │          │          │ Complete   │
│ Response   │      │ Response   │          │          │ Response   │
└────────────┘      └────────────┘          │          └────────────┘
                                            │
                                      Keeps waiting
                                      (or timeout)
```

## Component Interaction Diagram

```
┌────────────────────┐
│   HttpServer       │
│   (Axum)           │
└─────────┬──────────┘
          │
          │ HTTP Request
          ▼
┌────────────────────┐        ┌────────────────────┐
│   API Handler      │───────▶│  GameProcessor     │
│   (games.rs)       │        │  (VRF + Logic)     │
└─────────┬──────────┘        └────────────────────┘
          │
          │ Submit Transaction
          ▼
┌────────────────────┐
│  Transaction Pool  │
│  (Mempool)         │
└─────────┬──────────┘
          │
          │ Pulled by engine
          ▼
┌────────────────────┐        ┌────────────────────┐
│  DirectCommit      │───────▶│   RocksDB          │
│  Engine            │ Store  │   (Persistent)     │
└─────────┬──────────┘        └────────────────────┘
          │
          │ Emit event
          ▼
┌────────────────────┐
│  Event Publisher   │─ ─ ─ ─ ─Broadcast─ ─ ─ ─ ─ ─ ┐
│  (broadcast::Tx)   │
└────────────────────┘                                │
                                                      │
          ┌─────────────────────────────────────────┘
          │
          ▼
┌────────────────────┐
│ Finalization       │
│ Waiter             │
└─────────┬──────────┘
          │
          │ Resolves wait
          ▼
┌────────────────────┐
│   API Handler      │
│   (Continues)      │
└─────────┬──────────┘
          │
          │ HTTP Response
          ▼
┌────────────────────┐
│   Client           │
└────────────────────┘
```

## State Transitions

```
Game Request State Machine:

    [START]
       │
       ▼
  ┌─────────┐
  │ Receive │
  │ Request │
  └────┬────┘
       │
       ▼
  ┌─────────┐
  │ Validate│
  └────┬────┘
       │
       ▼
  ┌─────────┐
  │ Submit  │
  │   TX    │
  └────┬────┘
       │
       ├───────────────────────┐
       │                       │
       ▼                       ▼
  ┌─────────┐           ┌─────────┐
  │  Wait   │           │   No    │
  │  for    │           │ Waiter  │
  │ Commit  │           └────┬────┘
  └────┬────┘                │
       │                     │
       ├──────┬──────────────┘
       │      │
       ▼      ▼
  ┌─────┐  ┌────────┐
  │ OK  │  │Timeout │
  └──┬──┘  └───┬────┘
     │         │
     ▼         ▼
┌─────────┐ ┌─────────┐
│Complete │ │ Pending │
│Response │ │Response │
└─────────┘ └─────────┘
     │         │
     └────┬────┘
          │
          ▼
       [END]
```

## Performance Characteristics

```
Latency Distribution (DirectCommit 10ms blocks):

 0ms ├─
     │
10ms ├──────────────────────────────────┐ ◄─ P50 (Typical)
     │                                  │
15ms ├────────────────────────────────────┐ ◄─ P90
     │                                    │
20ms ├──────────────────────────────────────┐ ◄─ P95
     │                                      │
     │                                      │
2s   ├────────────────────────────────────────┐ ◄─ P99 (Timeout)
     │                                        │
     └────────────────────────────────────────┘

Legend:
  ├──────────┐  = Request distribution
  ◄─ Pxx      = Percentile marker
```

## Concurrency Model

```
Multiple Concurrent Requests:

Request A ────┬──── Wait ────┐
              │              │
Request B ────┼──── Wait ────┼──── Block Committed ──── Broadcast
              │              │              │               │
Request C ────┴──── Wait ────┘              │               │
                                            ▼               │
                                       ┌────────┐           │
                                       │RocksDB │           │
                                       └────────┘           │
                                                            │
                ┌───────────────────────────────────────────┘
                │
    ┌───────────┼───────────┐
    │           │           │
    ▼           ▼           ▼
Response A  Response B  Response C

All complete simultaneously (~10-20ms total)
No lock contention
Event-driven coordination
```
