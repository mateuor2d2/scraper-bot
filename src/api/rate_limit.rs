use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;

use crate::db::Db;

/// A single rate limit window entry.
#[derive(Debug, Clone)]
struct WindowEntry {
    timestamps: Vec<Instant>,
}

/// In-memory rate limiter using sliding window per API key.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// key -> list of request timestamps
    windows: Arc<RwLock<HashMap<String, WindowEntry>>>,
    /// Window size in seconds
    window_secs: u64,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
            window_secs: 60, // 1 minute window
        }
    }

    /// Check if a request is allowed for the given key.
    /// Returns (allowed, retry_after_secs).
    /// The limit depends on the plan stored in the DB for this key.
    /// For now we use a default: free=10/min, pro=60/min.
    pub async fn check(&self, key: &str, _db: &Db) -> (bool, Option<u64>) {
        let limit = 10; // default free limit

        let now = Instant::now();
        let window = self.window_secs;

        let mut windows = self.windows.write().await;

        let entry = windows.entry(key.to_string()).or_insert_with(|| WindowEntry {
            timestamps: Vec::new(),
        });

        // Remove old timestamps outside the window
        entry
            .timestamps
            .retain(|&t| now.duration_since(t).as_secs() < window);

        if entry.timestamps.len() < limit {
            entry.timestamps.push(now);
            (true, None)
        } else {
            // Calculate when the oldest entry will expire
            let oldest = entry.timestamps.first().unwrap();
            let retry_after = window - now.duration_since(*oldest).as_secs();
            (false, Some(retry_after))
        }
    }

    /// Check with a specific rate limit.
    pub async fn check_with_limit(&self, key: &str, limit: usize) -> (bool, Option<u64>) {
        let now = Instant::now();
        let window = self.window_secs;

        let mut windows = self.windows.write().await;

        let entry = windows.entry(key.to_string()).or_insert_with(|| WindowEntry {
            timestamps: Vec::new(),
        });

        entry
            .timestamps
            .retain(|&t| now.duration_since(t).as_secs() < window);

        if entry.timestamps.len() < limit {
            entry.timestamps.push(now);
            (true, None)
        } else {
            let oldest = entry.timestamps.first().unwrap();
            let retry_after = window - now.duration_since(*oldest).as_secs();
            (false, Some(retry_after))
        }
    }

    /// Get the rate limit for a plan.
    pub fn limit_for_plan(plan: &str) -> usize {
        match plan {
            "pro" => 60,
            _ => 10, // free
        }
    }

    /// Get the monthly limit for a plan.
    pub fn monthly_limit_for_plan(plan: &str) -> i64 {
        match plan {
            "pro" => 10_000,
            _ => 100,
        }
    }

    /// Periodic cleanup of expired entries (call from a background task).
    pub async fn cleanup(&self) {
        let now = Instant::now();
        let window = self.window_secs;

        let mut windows = self.windows.write().await;
        windows.retain(|_, entry| {
            entry
                .timestamps
                .retain(|&t| now.duration_since(t).as_secs() < window);
            !entry.timestamps.is_empty()
        });
    }
}
