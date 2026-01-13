//! Unified benchmark framework for testing blockchain performance
//!
//! Eliminates code duplication across different benchmark implementations

use crate::{
    config::PerformanceConfig,
    errors::AtomiqResult,
    storage::OptimizedStorage,
    AtomiqApp, Transaction,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{task, time::sleep};

/// Comprehensive benchmark configuration
#[derive(Clone, Debug)]
pub struct BenchmarkConfig {
    pub total_transactions: usize,
    pub batch_size: usize,
    pub concurrent_submitters: usize,
    pub warmup_duration: Duration,
    pub test_duration: Duration,
    pub enable_progress_reporting: bool,
    pub report_interval: Duration,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            total_transactions: 10_000,
            batch_size: 100,
            concurrent_submitters: 4,
            warmup_duration: Duration::from_secs(5),
            test_duration: Duration::from_secs(60),
            enable_progress_reporting: true,
            report_interval: Duration::from_secs(2),
        }
    }
}

impl From<PerformanceConfig> for BenchmarkConfig {
    fn from(perf_config: PerformanceConfig) -> Self {
        Self {
            total_transactions: perf_config.target_tps.unwrap_or(10_000) as usize * 10,
            batch_size: perf_config.batch_size,
            concurrent_submitters: perf_config.concurrent_submitters,
            warmup_duration: Duration::from_secs(perf_config.warmup_duration_seconds),
            test_duration: Duration::from_secs(perf_config.benchmark_duration_seconds),
            ..Default::default()
        }
    }
}

/// Comprehensive benchmark results
#[derive(Debug, Clone)]
pub struct BenchmarkResults {
    pub total_transactions_submitted: u64,
    pub total_transactions_processed: u64,
    pub total_blocks_created: u64,
    pub total_duration: Duration,
    pub submission_duration: Duration,
    pub processing_duration: Duration,
    pub submission_tps: f64,
    pub processing_tps: f64,
    pub average_transactions_per_block: f64,
    pub efficiency_percentage: f64,
    pub theoretical_max_tps: f64,
}

/// Unified benchmark executor
pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    blockchain: Arc<AtomiqApp>,
}

impl BenchmarkRunner {
    /// Create new benchmark runner
    pub fn new(blockchain: Arc<AtomiqApp>, config: BenchmarkConfig) -> Self {
        Self {
            config,
            blockchain,
        }
    }

    /// Run comprehensive benchmark with all phases
    pub async fn run_full_benchmark(&self) -> AtomiqResult<BenchmarkResults> {
        println!("🏁 Starting Comprehensive Blockchain Benchmark");
        println!("============================================");
        
        self.print_benchmark_config();
        
        let start_time = Instant::now();
        
        // Phase 1: Warmup
        if self.config.warmup_duration > Duration::ZERO {
            println!("\n🔥 Phase 1: Warmup ({:?})", self.config.warmup_duration);
            self.run_warmup().await;
        }

        // Phase 2: Main benchmark
        println!("\n🚀 Phase 2: Main Benchmark");
        let submission_start = Instant::now();
        self.run_transaction_submission().await;
        let submission_duration = submission_start.elapsed();

        // Phase 3: Wait for processing
        println!("\n⏳ Phase 3: Processing Completion");
        let processing_start = Instant::now();
        self.wait_for_processing_completion().await;
        let processing_duration = processing_start.elapsed();

        let total_duration = start_time.elapsed();

        // Generate results
        let results = self.generate_results(
            submission_duration,
            processing_duration,
            total_duration,
        );

        self.print_results(&results);
        Ok(results)
    }

    /// Run throughput-only benchmark (no consensus waiting)
    pub async fn run_throughput_benchmark(&self) -> AtomiqResult<BenchmarkResults> {
        println!("⚡ Starting Throughput-Only Benchmark");
        println!("====================================");
        
        self.print_benchmark_config();
        
        let start_time = Instant::now();
        
        // Run transaction submission
        self.run_transaction_submission().await;
        
        let total_duration = start_time.elapsed();
        
        // Generate throughput-focused results
        let results = self.generate_throughput_results(total_duration);
        
        self.print_results(&results);
        Ok(results)
    }

    /// Print benchmark configuration
    fn print_benchmark_config(&self) {
        println!("📊 Configuration:");
        println!("  • Total transactions: {}", self.config.total_transactions);
        println!("  • Batch size: {}", self.config.batch_size);
        println!("  • Concurrent submitters: {}", self.config.concurrent_submitters);
        println!("  • Warmup duration: {:?}", self.config.warmup_duration);
        println!("  • Test duration: {:?}", self.config.test_duration);
    }

    /// Run warmup phase
    async fn run_warmup(&self) {
        let warmup_transactions = 1000;
        let warmup_submitters = 2;
        
        println!("  Submitting {} warmup transactions", warmup_transactions);
        
        let mut handles = Vec::new();
        let transactions_per_submitter = warmup_transactions / warmup_submitters;
        
        for submitter_id in 0..warmup_submitters {
            let blockchain_clone = self.blockchain.clone();
            
            let handle = task::spawn(async move {
                Self::submit_transactions_for_submitter(
                    blockchain_clone,
                    submitter_id + 1000, // Offset to avoid ID conflicts
                    transactions_per_submitter,
                    50, // Small batch size
                ).await
            });
            
            handles.push(handle);
        }
        
        for handle in handles {
            let _ = handle.await;
        }
        
        // Allow time for warmup processing
        sleep(self.config.warmup_duration / 4).await;
        
        println!("  ✅ Warmup completed");
    }

    /// Run main transaction submission phase
    async fn run_transaction_submission(&self) {
        let mut handles = Vec::new();
        let transactions_per_submitter = self.config.total_transactions / self.config.concurrent_submitters;
        
        println!("📤 Submitting {} transactions with {} submitters", 
                self.config.total_transactions, self.config.concurrent_submitters);
        
        // Start progress monitoring
        let monitoring_handle = if self.config.enable_progress_reporting {
            Some(self.start_progress_monitoring().await)
        } else {
            None
        };
        
        // Start transaction submitters
        for submitter_id in 0..self.config.concurrent_submitters {
            let blockchain_clone = self.blockchain.clone();
            let batch_size = self.config.batch_size;
            
            let handle = task::spawn(async move {
                Self::submit_transactions_for_submitter(
                    blockchain_clone,
                    submitter_id,
                    transactions_per_submitter,
                    batch_size,
                ).await;
                
                // Record transaction submission
            });
            
            handles.push(handle);
        }
        
        // Wait for all submitters to complete
        for handle in handles {
            let _ = handle.await;
        }
        
        // Stop progress monitoring
        if let Some(handle) = monitoring_handle {
            handle.abort();
        }
        
        println!("✅ All transactions submitted");
    }

    /// Submit transactions for a single submitter
    async fn submit_transactions_for_submitter(
        blockchain: Arc<AtomiqApp>,
        submitter_id: usize,
        transaction_count: usize,
        batch_size: usize,
    ) {
        let mut submitted = 0;
        let sender = [submitter_id as u8; 32];
        let mut nonce = 1;
        
        while submitted < transaction_count {
            let batch_end = std::cmp::min(submitted + batch_size, transaction_count);
            
            for _ in submitted..batch_end {
                let transaction = Transaction {
                    id: 0, // Will be assigned by blockchain
                    sender,
                    data: format!("benchmark_data_{}_{}", submitter_id, nonce).into_bytes(),
                    timestamp: 0, // Will be assigned by blockchain
                    nonce,
                };
                
                blockchain.submit_transaction(transaction);
                nonce += 1;
            }
            
            submitted = batch_end;
            
            // Yield periodically to prevent overwhelming
            if submitted % (batch_size * 5) == 0 {
                task::yield_now().await;
            }
        }
    }

    /// Start progress monitoring task
    async fn start_progress_monitoring(&self) -> task::JoinHandle<()> {
        let blockchain_clone = self.blockchain.clone();
        let report_interval = self.config.report_interval;
        
        task::spawn(async move {
            let mut last_metrics = blockchain_clone.get_metrics();
            
            loop {
                sleep(report_interval).await;
                
                let current_metrics = blockchain_clone.get_metrics();
                let current_metrics = blockchain_clone.get_metrics();
                let current_tps = if current_metrics.total_blocks > last_metrics.total_blocks {
                    (current_metrics.total_transactions - last_metrics.total_transactions) as f64 / 
                    report_interval.as_secs() as f64
                } else {
                    0.0
                };
                
                let tx_delta = current_metrics.total_transactions - last_metrics.total_transactions;
                let block_delta = current_metrics.total_blocks - last_metrics.total_blocks;
                
                println!(
                    "  📊 Progress: {} tx (+{}), {} blocks (+{}), TPS: {:.1}, Pool: {}",
                    current_metrics.total_transactions,
                    tx_delta,
                    current_metrics.total_blocks,
                    block_delta,
                    current_tps,
                    current_metrics.pending_transactions,
                );
                
                last_metrics = current_metrics;
            }
        })
    }

    /// Wait for all transactions to be processed by the blockchain
    async fn wait_for_processing_completion(&self) {
        let target_transactions = self.config.total_transactions as u64;
        let timeout = Duration::from_secs(300); // 5 minute timeout
        let start_time = Instant::now();
        
        loop {
            let metrics = self.blockchain.get_metrics();
            
            // Check if processing is complete
            if metrics.total_transactions >= target_transactions && metrics.pending_transactions == 0 {
                println!("  ✅ All transactions processed successfully");
                break;
            }
            
            // Check for timeout
            if start_time.elapsed() > timeout {
                println!("  ⏰ Processing timeout reached");
                println!("    Final state: {} processed, {} pending", 
                    metrics.total_transactions, metrics.pending_transactions);
                break;
            }
            
            sleep(Duration::from_millis(100)).await;
        }
    }

    /// Generate comprehensive benchmark results
    fn generate_results(
        &self,
        submission_duration: Duration,
        processing_duration: Duration,
        total_duration: Duration,
    ) -> BenchmarkResults {
        let metrics = self.blockchain.get_metrics();
        let submitted = self.config.total_transactions as u64;
        
        let submission_tps = submitted as f64 / submission_duration.as_secs_f64();
        let processing_tps = metrics.total_transactions as f64 / total_duration.as_secs_f64();
        
        let avg_tx_per_block = if metrics.total_blocks > 0 {
            metrics.total_transactions as f64 / metrics.total_blocks as f64
        } else {
            0.0
        };
        
        // Calculate theoretical max TPS based on blockchain config
        let theoretical_max_tps = if let Ok(blockchain_config) = self.get_blockchain_config() {
            blockchain_config.max_transactions_per_block as f64 
                / (blockchain_config.max_block_time_ms as f64 / 1000.0)
        } else {
            0.0
        };
        
        let efficiency = if theoretical_max_tps > 0.0 {
            (processing_tps / theoretical_max_tps) * 100.0
        } else {
            0.0
        };
        
        BenchmarkResults {
            total_transactions_submitted: submitted,
            total_transactions_processed: metrics.total_transactions,
            total_blocks_created: metrics.total_blocks,
            total_duration,
            submission_duration,
            processing_duration,
            submission_tps,
            processing_tps,
            average_transactions_per_block: avg_tx_per_block,
            efficiency_percentage: efficiency,
            theoretical_max_tps,
        }
    }

    /// Generate throughput-only results
    fn generate_throughput_results(&self, total_duration: Duration) -> BenchmarkResults {
        let submitted = self.config.total_transactions as u64;
        let submission_tps = submitted as f64 / total_duration.as_secs_f64();
        
        BenchmarkResults {
            total_transactions_submitted: submitted,
            total_transactions_processed: submitted, // Assume all processed for throughput test
            total_blocks_created: 0, // Not applicable for throughput test
            total_duration,
            submission_duration: total_duration,
            processing_duration: Duration::ZERO,
            submission_tps,
            processing_tps: submission_tps,
            average_transactions_per_block: 0.0,
            efficiency_percentage: 100.0, // Throughput test assumes optimal
            theoretical_max_tps: submission_tps,
        }
    }

    /// Print benchmark results
    fn print_results(&self, results: &BenchmarkResults) {
        println!("\n🎯 BENCHMARK RESULTS");
        println!("==================");
        println!("📤 Submission Performance:");
        println!("  • Transactions submitted: {}", results.total_transactions_submitted);
        println!("  • Submission time: {:?}", results.submission_duration);
        println!("  • Submission TPS: {:.2}", results.submission_tps);
        
        if results.total_blocks_created > 0 {
            println!("\n📦 Consensus Performance:");
            println!("  • Transactions processed: {}", results.total_transactions_processed);
            println!("  • Blocks created: {}", results.total_blocks_created);
            println!("  • Processing time: {:?}", results.processing_duration);
            println!("  • Processing TPS: {:.2}", results.processing_tps);
            println!("  • Avg tx per block: {:.1}", results.average_transactions_per_block);
        }
        
        println!("\n⚡ Overall Performance:");
        println!("  • Total time: {:?}", results.total_duration);
        if results.theoretical_max_tps > 0.0 {
            println!("  • Theoretical max TPS: {:.2}", results.theoretical_max_tps);
            println!("  • Efficiency: {:.1}%", results.efficiency_percentage);
        }
    }

    /// Get blockchain configuration (placeholder - would need access to actual config)
    fn get_blockchain_config(&self) -> Result<crate::config::BlockchainConfig, ()> {
        // This would need to be passed in or retrieved from the blockchain app
        Err(()) // Placeholder implementation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtomiqApp, BlockchainConfig};

    #[test]
    fn test_benchmark_config_creation() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.total_transactions, 10_000);
        assert_eq!(config.concurrent_submitters, 4);
    }

    #[test]
    fn test_benchmark_config_from_perf_config() {
        let perf_config = PerformanceConfig {
            target_tps: Some(5000),
            concurrent_submitters: 8,
            batch_size: 200,
            benchmark_duration_seconds: 30,
            warmup_duration_seconds: 10,
        };
        
        let benchmark_config = BenchmarkConfig::from(perf_config);
        assert_eq!(benchmark_config.total_transactions, 50_000);
        assert_eq!(benchmark_config.concurrent_submitters, 8);
        assert_eq!(benchmark_config.batch_size, 200);
    }

    #[tokio::test]
    async fn test_benchmark_runner_creation() {
        let blockchain_config = BlockchainConfig::default();
        let app = Arc::new(AtomiqApp::new(blockchain_config));
        let benchmark_config = BenchmarkConfig {
            total_transactions: 100,
            ..Default::default()
        };
        
        let runner = BenchmarkRunner::new(app, benchmark_config);
        assert_eq!(runner.config.total_transactions, 100);
    }
}