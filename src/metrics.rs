//! Performance monitoring and metrics collection

use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct PerformanceMonitor {
    start_time: Instant,
    transaction_count: Arc<AtomicU64>,
    block_count: Arc<AtomicU64>,
    last_tps_calculation: Arc<std::sync::RwLock<Instant>>,
    last_transaction_count: Arc<AtomicU64>,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            transaction_count: Arc::new(AtomicU64::new(0)),
            block_count: Arc::new(AtomicU64::new(0)),
            last_tps_calculation: Arc::new(std::sync::RwLock::new(Instant::now())),
            last_transaction_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn record_transactions(&self, count: u64) {
        self.transaction_count.fetch_add(count, Ordering::SeqCst);
    }

    pub fn record_block(&self) {
        self.block_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn calculate_tps(&self) -> f64 {
        let now = Instant::now();
        let mut last_time = self.last_tps_calculation.write().unwrap();
        let time_diff = now.duration_since(*last_time);
        
        if time_diff < Duration::from_secs(1) {
            return 0.0; // Too early to calculate
        }

        let current_tx_count = self.transaction_count.load(Ordering::SeqCst);
        let last_tx_count = self.last_transaction_count.load(Ordering::SeqCst);
        let tx_diff = current_tx_count - last_tx_count;

        let tps = tx_diff as f64 / time_diff.as_secs_f64();

        // Update for next calculation
        *last_time = now;
        self.last_transaction_count.store(current_tx_count, Ordering::SeqCst);

        tps
    }

    pub fn total_runtime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn average_tps(&self) -> f64 {
        let total_seconds = self.total_runtime().as_secs_f64();
        if total_seconds < 1.0 {
            return 0.0;
        }
        self.transaction_count.load(Ordering::SeqCst) as f64 / total_seconds
    }
}