use crate::gas_price::models::GasTokenPrice;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// In-memory price cache
#[allow(dead_code)]
pub struct PriceCache {
    /// Cache entries by token ID
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// Cache TTL in seconds
    ttl_seconds: u64,
    /// Whether to allow returning expired entries
    fallback_to_cache: bool,
}

/// Individual cache entry
#[derive(Clone)]
struct CacheEntry {
    price: GasTokenPrice,
    inserted_at: u64,
}

#[allow(dead_code)]
impl PriceCache {
    /// Create a new price cache with the given TTL
    pub fn new(ttl_seconds: u64) -> Self {
        Self::with_options(ttl_seconds, false)
    }

    /// Create a new price cache with options
    pub fn with_options(ttl_seconds: u64, fallback_to_cache: bool) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            ttl_seconds,
            fallback_to_cache,
        }
    }

    /// Get a price from the cache
    pub async fn get(&self, token_id: &str) -> Option<GasTokenPrice> {
        self.get_with_options(token_id, self.fallback_to_cache)
            .await
    }

    /// Get a price from the cache with options
    pub async fn get_with_options(
        &self,
        token_id: &str,
        allow_expired: bool,
    ) -> Option<GasTokenPrice> {
        let entries = self.entries.read().await;

        if let Some(entry) = entries.get(token_id) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now - entry.inserted_at <= self.ttl_seconds {
                debug!("Cache hit for token {}", token_id);
                return Some(entry.price.clone());
            } else if allow_expired {
                debug!(
                    "Cache entry expired for token {} but returning stale value (age: {}s)",
                    token_id,
                    now - entry.inserted_at
                );
                return Some(entry.price.clone());
            } else {
                debug!("Cache entry expired for token {}", token_id);
            }
        }

        None
    }

    /// Get multiple prices from the cache
    pub async fn get_many(&self, token_ids: &[String]) -> HashMap<String, GasTokenPrice> {
        let entries = self.entries.read().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut results = HashMap::new();

        for token_id in token_ids {
            if let Some(entry) = entries.get(token_id) {
                if now - entry.inserted_at <= self.ttl_seconds {
                    results.insert(token_id.clone(), entry.price.clone());
                }
            }
        }

        results
    }

    /// Insert a price into the cache
    pub async fn insert(&self, price: GasTokenPrice) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = CacheEntry {
            price: price.clone(),
            inserted_at: now,
        };

        let mut entries = self.entries.write().await;
        entries.insert(price.token_id.clone(), entry);

        debug!(
            "Cached price for token {}: ${:.2}",
            price.token_id, price.price_usd
        );
    }

    /// Insert multiple prices into the cache
    pub async fn insert_many(&self, prices: Vec<GasTokenPrice>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut entries = self.entries.write().await;

        for price in prices {
            let entry = CacheEntry {
                price: price.clone(),
                inserted_at: now,
            };
            entries.insert(price.token_id.clone(), entry);
        }

        info!("Cached {} prices", entries.len());
    }

    /// Clear expired entries from the cache
    pub async fn clear_expired(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut entries = self.entries.write().await;
        let initial_count = entries.len();

        entries.retain(|_token_id, entry| now - entry.inserted_at <= self.ttl_seconds);

        let removed = initial_count - entries.len();
        if removed > 0 {
            debug!("Cleared {} expired cache entries", removed);
        }
    }

    /// Clear all entries from the cache
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        let count = entries.len();
        entries.clear();
        info!("Cleared {} cache entries", count);
    }

    /// Get the number of cached entries
    pub async fn size(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Get all cached prices (including expired ones)
    pub async fn get_all(&self) -> Vec<GasTokenPrice> {
        self.entries
            .read()
            .await
            .values()
            .map(|entry| entry.price.clone())
            .collect()
    }
}
