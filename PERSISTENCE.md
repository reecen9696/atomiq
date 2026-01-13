# Blockchain Persistence Modes

## Overview

The Atomiq blockchain now supports two distinct operational modes:

1. **Production Mode** (default) - Preserves blockchain data across restarts
2. **Testing Mode** - Clears database on startup for clean test runs

## Configuration

The mode is controlled by the `clear_on_start` flag in `StorageConfig`:

```rust
pub struct StorageConfig {
    pub data_directory: String,
    // ... other fields ...
    pub clear_on_start: bool,  // false = production, true = testing
}
```

## Usage

### Production Mode (Persistent)

**Default behavior** - preserves blockchain data:

```rust
// Using default config
let config = AtomiqConfig::default();  // clear_on_start = false
let (app, handle) = BlockchainFactory::create_blockchain(config).await?;

// Using production preset
let (app, handle) = BlockchainFactory::create_production().await?;
```

**CLI:**

```bash
# Single validator (production)
cargo run --bin atomiq-unified -- single-validator --max-tx-per-block 100 --block-time-ms 100

# Throughput test (production, uses default config)
cargo run --bin atomiq-unified -- throughput-test --total-transactions 1000 --batch-size 100
```

**Output:**

```
📦 Production mode: Preserving existing blockchain data at ./blockchain_data
```

### Testing Mode (Clear on Start)

**For benchmarks and tests** - clears database on startup:

```rust
// Using high-performance preset
let (app, handle) = BlockchainFactory::create_high_performance().await?;

// Using consensus-testing preset
let (app, handle) = BlockchainFactory::create_consensus_testing().await?;

// Explicit configuration
let mut config = AtomiqConfig::default();
config.storage.clear_on_start = true;
let (app, handle) = BlockchainFactory::create_blockchain(config).await?;
```

**CLI:**

```bash
# Performance benchmark (testing mode)
cargo run --bin atomiq-unified -- benchmark-performance --target-tps 10000 --total-transactions 1000

# Consensus benchmark (testing mode)
cargo run --bin atomiq-unified -- benchmark-consensus --max-tx-per-block 100 --block-time-ms 500
```

**Output:**

```
⚠️  Testing mode: Clearing database at ./blockchain_data
```

## Production Deployment

### Starting Fresh

```bash
# First time - creates new blockchain
cargo run --release --bin atomiq-unified -- single-validator
```

### Stopping & Restarting

```bash
# Stop with Ctrl+C
^C

# Restart - resumes from last committed block
cargo run --release --bin atomiq-unified -- single-validator
```

### Important Notes

1. **Data Persistence**: RocksDB stores all blocks, consensus state, and validator set in `./blockchain_data/`

2. **Clean Shutdown**: Use Ctrl+C for graceful shutdown. The blockchain state is continuously persisted.

3. **Backup**: To backup blockchain state:

   ```bash
   cp -r ./blockchain_data ./blockchain_data.backup
   ```

4. **Recovery**: To restore from backup:

   ```bash
   rm -rf ./blockchain_data
   cp -r ./blockchain_data.backup ./blockchain_data
   ```

5. **Testing**: Always use testing presets for benchmarks to avoid data conflicts

## Configuration Presets

| Preset                | Mode       | clear_on_start | Use Case                    |
| --------------------- | ---------- | -------------- | --------------------------- |
| `default()`           | Production | `false`        | General production use      |
| `production()`        | Production | `false`        | Production deployment       |
| `high_performance()`  | Testing    | `true`         | Performance benchmarks      |
| `consensus_testing()` | Testing    | `true`         | Consensus correctness tests |

## Code Changes Required

To switch an existing config from testing to production:

```rust
// Before (testing)
let config = AtomiqConfig::high_performance();

// After (production)
let mut config = AtomiqConfig::high_performance();
config.storage.clear_on_start = false;  // Preserve data

// Or use production preset
let config = AtomiqConfig::production();
```

## Future Enhancements

Current persistence capabilities:

- ✅ Block tree persistence (via HotStuff-rs)
- ✅ Consensus state persistence
- ✅ Validator set persistence
- ⚠️ Application state is in-memory (StateManager uses HashMap)

Planned improvements:

- [ ] Persist application state (transaction nonces, app data)
- [ ] Add state migration system
- [ ] Implement checkpoint/restore API
- [ ] Add recovery from corrupted state
- [ ] Hot reload configuration
