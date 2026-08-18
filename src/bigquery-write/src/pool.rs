// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::runner::{Runner, WriteRequest};
use crate::transport::Transport;
use arc_swap::ArcSwap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

/// An entry representing an active, healthy stream connection (Runner).
#[derive(Clone, Debug)]
pub(crate) struct StreamEntry {
    /// Unique identifier for this stream connection.
    pub(crate) id: u64,
    /// Channel to send requests to the stream's background runner task.
    pub(crate) sender: mpsc::Sender<WriteRequest>,
    /// Track the number of outstanding requests on this stream.
    pub(crate) outstanding_requests: Arc<AtomicU64>,
    /// Track the total outstanding bytes on this stream.
    pub(crate) outstanding_bytes: Arc<AtomicU64>,
}

/// A concurrent, lock-free pool of healthy stream connections to BigQuery Write.
///
/// It coordinates the active stream connection pool, tracking outstanding load
/// and atomically evicting / replacing dead streams.
#[derive(Debug)]
pub(crate) struct StreamPool {
    inner: Arc<Transport>,
    next_stream_id: AtomicU64,
    streams: ArcSwap<Vec<StreamEntry>>,
}

const MAX_POOL_SIZE: usize = 8;
const MAX_REQUESTS_THRESHOLD: u64 = 100;
const MAX_BYTES_THRESHOLD: u64 = 10 * 1024 * 1024; // 10 MB

impl StreamPool {
    /// Initializes a new [StreamPool].
    pub(crate) fn new(inner: Arc<Transport>) -> Self {
        Self {
            inner,
            next_stream_id: AtomicU64::new(1),
            streams: ArcSwap::from_pointee(Vec::new()),
        }
    }

    /// Selects the stream connection with the least load, dynamically scaling up if needed.
    pub(crate) fn get_least_loaded_stream(&self) -> StreamEntry {
        let streams = self.streams.load();

        // 1. If empty, lazily spawn the first stream immediately.
        if streams.is_empty() {
            return self
                .spawn_new_stream_atomic()
                .expect("StreamPool invariant: lazy spawn of first stream must succeed");
        }

        // Reload the streams list in case it was modified.
        let streams = self.streams.load();
        let least_loaded = streams
            .iter()
            .min_by_key(|entry| entry.outstanding_requests.load(Ordering::Relaxed))
            .cloned()
            .unwrap();

        let reqs = least_loaded.outstanding_requests.load(Ordering::Relaxed);
        let bytes = least_loaded.outstanding_bytes.load(Ordering::Relaxed);

        // 3. Scale up if the stream is congested and we are under the limit.
        if (reqs >= MAX_REQUESTS_THRESHOLD || bytes >= MAX_BYTES_THRESHOLD)
            && streams.len() < MAX_POOL_SIZE
        {
            let maybe_stream = self.spawn_new_stream_atomic();
            if let Some(stream) = maybe_stream {
                return stream;
            }
        }

        least_loaded
    }

    /// Atomically and thread-safely provisions a new stream connection to scale up.
    fn spawn_new_stream_atomic(&self) -> Option<StreamEntry> {
        let mut newly_created: Option<StreamEntry> = None;
        let mut success = false;

        self.streams.rcu(|current| {
            // If another racing thread scaled us up to limit already, do nothing.
            if !current.is_empty() && current.len() >= MAX_POOL_SIZE {
                success = false;
                return Arc::clone(current);
            }

            // Capture/cache the new entry during retries to avoid duplicating Runner tasks.
            let entry = if let Some(ref entry) = newly_created {
                entry.clone()
            } else {
                let new_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
                let runner = Runner::new(self.inner.clone());
                let entry = StreamEntry {
                    id: new_id,
                    sender: runner.req_tx,
                    outstanding_requests: Arc::new(AtomicU64::new(0)),
                    outstanding_bytes: Arc::new(AtomicU64::new(0)),
                };
                newly_created = Some(entry.clone());
                entry
            };

            success = true;
            let mut updated = (**current).clone();
            updated.push(entry);
            Arc::new(updated)
        });

        if success { newly_created } else { None }
    }

    /// Atomically evicts the failed stream and provisions a new one in place.
    pub(crate) fn evict_and_replace(&self, failed_id: u64) {
        let mut newly_created: Option<StreamEntry> = None;

        self.streams.rcu(|current| {
            // If already evicted by a racing writer, do nothing
            if !current.iter().any(|entry| entry.id == failed_id) {
                return Arc::clone(current);
            }

            let mut updated = (**current).clone();
            updated.retain(|entry| entry.id != failed_id);

            // Provision a replacement stream (Runner), caching during retries
            let entry = if let Some(ref entry) = newly_created {
                entry.clone()
            } else {
                let new_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
                let runner = Runner::new(self.inner.clone());
                let entry = StreamEntry {
                    id: new_id,
                    sender: runner.req_tx,
                    outstanding_requests: Arc::new(AtomicU64::new(0)),
                    outstanding_bytes: Arc::new(AtomicU64::new(0)),
                };
                newly_created = Some(entry.clone());
                entry
            };

            updated.push(entry);
            Arc::new(updated)
        });
    }

    /// Prunes closed channels from the active pool.
    pub(crate) fn prune_dead_streams(&self) {
        self.streams.rcu(|current| {
            if current.is_empty() {
                return Arc::clone(current);
            }
            if current.iter().all(|e| !e.sender.is_closed()) {
                return Arc::clone(current);
            }

            let mut updated = (**current).clone();
            updated.retain(|e| !e.sender.is_closed());

            Arc::new(updated)
        });
    }

    /// Stub for the rebalance algorithm.
    ///
    /// This demonstrates how load rebalancing can update the `ArcSwap` routing table
    /// off the hot path, entirely without locks or disruptions to active writers.
    pub(crate) fn rebalance(&self) {
        // A stub rebalance operation: simply clone and store to prove
        // it can be done lock-free and concurrently.
        self.streams.rcu(|current| {
            let updated = (**current).clone();
            Arc::new(updated)
        });
    }
}

/// A single, exclusive connection manager (no multiplexing).
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct ExclusivePool {
    inner: Arc<Transport>,
    next_stream_id: AtomicU64,
    stream: ArcSwap<StreamEntry>,
}

#[allow(dead_code)]
impl ExclusivePool {
    /// Initializes a new [ExclusivePool].
    pub(crate) fn new(inner: Arc<Transport>) -> Self {
        let runner = Runner::new(inner.clone());
        let initial_stream = StreamEntry {
            id: 1,
            sender: runner.req_tx,
            outstanding_requests: Arc::new(AtomicU64::new(0)),
            outstanding_bytes: Arc::new(AtomicU64::new(0)),
        };
        Self {
            inner,
            next_stream_id: AtomicU64::new(2),
            stream: ArcSwap::from_pointee(initial_stream),
        }
    }

    /// Acquires the current exclusive stream connection.
    pub(crate) fn get_exclusive_stream(&self) -> StreamEntry {
        (*self.stream.load_full()).clone()
    }

    /// Atomically replaces the dead exclusive stream connection.
    pub(crate) fn evict_and_replace_exclusive(&self, failed_id: u64) {
        let mut newly_created: Option<StreamEntry> = None;

        self.stream.rcu(|current| {
            // If already replaced by another writer, do nothing.
            if current.id != failed_id {
                return Arc::clone(current);
            }

            let entry = if let Some(ref entry) = newly_created {
                entry.clone()
            } else {
                let new_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
                let runner = Runner::new(self.inner.clone());
                let entry = StreamEntry {
                    id: new_id,
                    sender: runner.req_tx,
                    outstanding_requests: Arc::new(AtomicU64::new(0)),
                    outstanding_bytes: Arc::new(AtomicU64::new(0)),
                };
                newly_created = Some(entry.clone());
                entry
            };

            Arc::new(entry)
        });
    }
}

/// Connection pool strategy enum (Pattern A).
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum ConnectionPool {
    Multiplexed(Arc<StreamPool>),
    Exclusive(ExclusivePool),
}

impl ConnectionPool {
    /// Loads a stream connection, delegating based on the pool strategy.
    pub(crate) fn get_stream(&self) -> StreamEntry {
        match self {
            ConnectionPool::Multiplexed(pool) => pool.get_least_loaded_stream(),
            ConnectionPool::Exclusive(pool) => pool.get_exclusive_stream(),
        }
    }

    /// Atomically handles stream failure eviction and replacement.
    pub(crate) fn evict_and_replace(&self, failed_id: u64) {
        match self {
            ConnectionPool::Multiplexed(pool) => pool.evict_and_replace(failed_id),
            ConnectionPool::Exclusive(pool) => pool.evict_and_replace_exclusive(failed_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::tests::test_transport;

    #[tokio::test]
    async fn least_loaded_routing() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1".to_string()).await?);
        let pool = StreamPool::new(transport);
        for _ in 0..3 {
            pool.spawn_new_stream_atomic();
        }

        // Initially, all have 0 load.
        let first = pool.get_least_loaded_stream();
        assert!([1, 2, 3].contains(&first.id));

        // Set outstanding request load on streams 1 and 2.
        {
            let streams = pool.streams.load();
            streams
                .iter()
                .find(|e| e.id == 1)
                .unwrap()
                .outstanding_requests
                .store(100, Ordering::Relaxed);
            streams
                .iter()
                .find(|e| e.id == 2)
                .unwrap()
                .outstanding_requests
                .store(50, Ordering::Relaxed);
        }

        // Should return stream 3 (0 load).
        let picked = pool.get_least_loaded_stream();
        assert_eq!(picked.id, 3);

        // Set higher load on stream 3.
        {
            let streams = pool.streams.load();
            streams
                .iter()
                .find(|e| e.id == 3)
                .unwrap()
                .outstanding_requests
                .store(200, Ordering::Relaxed);
        }

        // Now stream 2 has 50 load, stream 1 has 100 load, and stream 3 has 200 load.
        // It should pick stream 2.
        let picked = pool.get_least_loaded_stream();
        assert_eq!(picked.id, 2);

        Ok(())
    }

    #[tokio::test]
    async fn evict_and_replace() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1".to_string()).await?);
        let pool = StreamPool::new(transport);
        for _ in 0..3 {
            pool.spawn_new_stream_atomic();
        }

        // Fetch initial snapshot IDs
        let initial_ids: Vec<u64> = pool.streams.load().iter().map(|e| e.id).collect();
        assert_eq!(initial_ids, vec![1, 2, 3]);

        // Evict stream 2
        pool.evict_and_replace(2);

        let final_ids: Vec<u64> = pool.streams.load().iter().map(|e| e.id).collect();
        // Stream 2 should be gone, replaced by stream 4.
        assert_eq!(final_ids, vec![1, 3, 4]);

        Ok(())
    }

    #[tokio::test]
    async fn prune_dead_streams() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1".to_string()).await?);
        let pool = StreamPool::new(transport);
        for _ in 0..3 {
            pool.spawn_new_stream_atomic();
        }

        // Initially all are open/healthy
        pool.prune_dead_streams();
        assert_eq!(pool.streams.load().len(), 3);

        // Create a stream with an explicitly closed sender
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // drop receiver to close channel
        pool.streams.rcu(|current| {
            let mut updated = (**current).clone();
            updated.push(StreamEntry {
                id: 99,
                sender: tx.clone(),
                outstanding_requests: Arc::new(AtomicU64::new(0)),
                outstanding_bytes: Arc::new(AtomicU64::new(0)),
            });
            Arc::new(updated)
        });

        // Pruning should find the closed sender for ID 99 and remove it.
        pool.prune_dead_streams();
        let remaining_ids: Vec<u64> = pool.streams.load().iter().map(|e| e.id).collect();
        assert!(!remaining_ids.contains(&99));
        assert_eq!(remaining_ids.len(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn dynamic_scale_up() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1".to_string()).await?);
        let pool = StreamPool::new(transport); // Start with 0 streams!

        // Initially empty
        assert_eq!(pool.streams.load().len(), 0);

        // 1. First lookup should lazily spawn the first stream!
        let s1 = pool.get_least_loaded_stream();
        assert_eq!(s1.id, 1);
        assert_eq!(pool.streams.load().len(), 1);

        // 2. Lookup again under low load -> should return the same stream without scaling up
        let s2 = pool.get_least_loaded_stream();
        assert_eq!(s2.id, 1);
        assert_eq!(pool.streams.load().len(), 1);

        // 3. Set load on stream 1 to trigger scale up
        s1.outstanding_requests.store(100, Ordering::Relaxed);

        // 4. Lookup again under high load -> should scale up and spawn a new stream!
        let s3 = pool.get_least_loaded_stream();
        assert_eq!(s3.id, 2);
        assert_eq!(pool.streams.load().len(), 2);

        // 5. Under low load, both are in the pool, but stream 2 has 0 load and stream 1 has 100 load.
        // It should return stream 2.
        let s4 = pool.get_least_loaded_stream();
        assert_eq!(s4.id, 2);
        assert_eq!(pool.streams.load().len(), 2);

        Ok(())
    }
}
