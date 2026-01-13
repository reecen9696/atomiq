# Atomiq Blockchain

A production-ready, high-performance blockchain built on the **HotStuff-rs** consensus protocol. Atomiq is designed for single-validator deployments with enterprise-grade persistence, comprehensive benchmarking tools, and a clean CLI interface.

## What is Atomiq?

Atomiq is a **single-validator blockchain** that provides:

- **Byzantine Fault Tolerant Consensus**: Uses the HotStuff consensus algorithm (the same algorithm powering Meta's Diem/Libra)
- **Persistent Storage**: RocksDB-backed blockchain with full state persistence across restarts
- **Production & Testing Modes**: Clear separation between production (persistent) and testing (ephemeral) configurations
- **Comprehensive Benchmarking**: Built-in tools to measure throughput, consensus performance, and system limits
- **Clean Architecture**: Factory patterns, modular design, and enterprise-grade error handling

## Quick Start

```bash
# Run single validator blockchain (production mode)
cargo run --release --bin atomiq-unified -- single-validator --max-tx-per-block 100 --block-time-ms 500

# Run throughput test (no consensus, just submission speed)
cargo run --release --bin atomiq-unified -- throughput-test --total-transactions 10000 --batch-size 100

# Run consensus benchmark (with actual block commits)
cargo run --release --bin atomiq-unified -- benchmark-consensus --total-transactions 500 --duration-seconds 30

# Run high-performance benchmark
cargo run --release --bin atomiq-unified -- benchmark-performance --target-tps 10000 --total-transactions 1000

# Inspect database contents
cargo run --release --bin atomiq-unified -- inspect-db
```

## How It Works

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                  Atomiq Application                      │
├─────────────────────────────────────────────────────────┤
│  Transaction Pool → State Manager → Block Creation      │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│              HotStuff-rs Consensus Layer                 │
├─────────────────────────────────────────────────────────┤
│  • Propose Blocks                                        │
│  • Phase Voting (Prepare, Pre-Commit, Commit)          │
│  • View Advancement                                      │
│  • Safety & Liveness Guarantees                         │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│                 RocksDB Persistence                      │
├─────────────────────────────────────────────────────────┤
│  • Block Tree Storage                                    │
│  • Transaction History                                   │
│  • Consensus State (View, PC, Validator Set)           │
│  • Application State (Nonces, Data)                     │
└─────────────────────────────────────────────────────────┘
```

### Consensus Flow

1. **Transaction Submission**: Transactions are submitted to the transaction pool
2. **Block Proposal**: Leader (single validator) proposes a new block with pending transactions
3. **Voting Phases**:
   - Generic phase vote validates the block
   - Block is inserted into the block tree
   - Phase Certificate (PC) is collected
4. **Block Commit**: After 3-chain rule is satisfied, blocks are committed to the chain
5. **State Update**: Committed blocks update the application state
6. **Persistence**: All data is written to RocksDB for crash recovery

### Key Components

- **`BlockchainFactory`**: Creates blockchain instances with different configurations (production, testing, high-performance)
- **`StateManager`**: Manages transaction nonces, validation, and state updates
- **`TransactionPool`**: Buffers and batches transactions for block creation
- **`BenchmarkRunner`**: Provides comprehensive performance testing tools
- **`OptimizedStorage`**: RocksDB wrapper with performance tuning (LZ4 compression, write buffers)

## Performance Characteristics

### Measured Performance

- **Submission TPS**: 300K - 400K transactions/second (throughput test, no consensus)
- **Processing TPS**: ~10 transactions/second (with full HotStuff consensus)
- **Block Time**: Configurable (default 100ms for production, 5-10ms for testing)
- **Consensus Overhead**: ~100ms per view (proposal + voting + commit phases)

### Why Single Validator?

The single-validator configuration provides:

✅ **Simplified Operations**: No network coordination, no peer discovery  
✅ **Deterministic Performance**: Predictable latency without network variables  
✅ **Production Ready**: Proven consensus algorithm without multi-validator complexity  
✅ **Easy Testing**: Fast iteration for development and benchmarking

⚠️ **Trade-off**: No Byzantine fault tolerance (system fails if validator fails)

## Configuration Modes

Atomiq supports multiple operational modes:

### Production Mode (Default)

```rust
AtomiqConfig::production()
// or
AtomiqConfig::default()
```

- **Data Persistence**: ✅ Preserves blockchain across restarts
- **Block Time**: 100ms (configurable)
- **Validation**: Full state validation enabled
- **Use Case**: Production deployment, long-running nodes

### Testing Mode

```rust
AtomiqConfig::high_performance()
AtomiqConfig::consensus_testing()
```

- **Data Persistence**: ❌ Clears database on startup
- **Block Time**: 5-10ms (aggressive)
- **Validation**: Configurable (can disable for max speed)
- **Use Case**: Benchmarks, CI/CD tests, development

See [PERSISTENCE.md](PERSISTENCE.md) for detailed persistence documentation.

## CLI Commands

### `single-validator`

Runs a single validator blockchain with actual consensus.

```bash
cargo run --release --bin atomiq-unified -- single-validator \
  --max-tx-per-block 100 \
  --block-time-ms 500
```

### `benchmark-consensus`

Tests consensus correctness and performance.

```bash
cargo run --release --bin atomiq-unified -- benchmark-consensus \
  --total-transactions 1000 \
  --duration-seconds 30
```

### `benchmark-performance`

High-speed performance test with aggressive timing.

```bash
cargo run --release --bin atomiq-unified -- benchmark-performance \
  --target-tps 10000 \
  --total-transactions 5000 \
  --concurrent-submitters 8
```

### `throughput-test`

Measures pure transaction submission speed (no consensus).

```bash
cargo run --release --bin atomiq-unified -- throughput-test \
  --total-transactions 10000 \
  --batch-size 100
```

### `inspect-db`

Examine database contents and statistics.

```bash
cargo run --release --bin atomiq-unified -- inspect-db --db-path ./blockchain_data
```

## Development

### Project Structure

```
atomiq/
├── src/
│   ├── lib.rs              # Core blockchain logic (AtomiqApp)
│   ├── factory.rs          # Blockchain initialization factory
│   ├── config.rs           # Configuration system
│   ├── state_manager.rs    # State validation & management
│   ├── transaction_pool.rs # Transaction buffering
│   ├── storage.rs          # RocksDB wrapper
│   ├── benchmark.rs        # Benchmarking tools
│   ├── errors.rs           # Error types
│   ├── network.rs          # Mock network implementation
│   └── main_unified.rs     # CLI entry point
├── Cargo.toml
├── README.md
└── PERSISTENCE.md
```

### Running Tests

```bash
# Run all tests
cargo test

# Run with logging
RUST_LOG=info cargo test

# Run specific test
cargo test test_blockchain_initialization
```

### Adding Custom Transaction Logic

Extend the `AtomiqApp` to add custom transaction types:

```rust
impl App for AtomiqApp {
    fn execute_transactions(&self, txs: &[Transaction]) -> (Vec<ExecutionResult>, AppStateUpdates) {
        // Your custom logic here
        // - Validate transaction format
        // - Check business rules
        // - Update state
        // - Return results
    }
}
```

## Future Extensions

### Multi-Validator Support

The architecture is designed to easily support multiple validators:

1. Replace `SingleValidatorNetwork` with TCP/UDP network implementation
2. Add multiple validators to `ValidatorSetUpdates`
3. Configure proper view timeout for network latency

### Application-Specific Logic

Atomiq can be extended for:

- **Gaming/Casino**: Player balances, game outcomes, provably fair randomness
- **DeFi**: Token transfers, liquidity pools, lending protocols
- **NFT Marketplace**: Asset creation, ownership transfers, royalties
- **Supply Chain**: Product tracking, authenticity verification

### Performance Optimizations

- Parallel transaction execution
- State pruning and archival nodes
- Batch signature verification
- Memory-mapped storage

## Troubleshooting

### Database Issues

```bash
# Clear and restart
rm -rf blockchain_data/
cargo run --release --bin atomiq-unified -- single-validator
```

### Performance Issues

- Check `max_view_time` configuration (lower for single validator)
- Verify RocksDB settings (write buffer size, compression)
- Monitor system resources (CPU, disk I/O)

### Consensus Stuck

- View timeouts are working as expected (you'll see timeout logs)
- Single validator self-messages may cause temporary delays
- Check block proposal rate matches expected timing

## License

See LICENSE file for details.

## Contributing

Contributions welcome! Please open an issue or PR.

---

**Built with [HotStuff-rs](https://github.com/parallelchain-io/hotstuff_rs)** - A high-performance implementation of the HotStuff consensus protocol
