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

use super::dispatcher::Dispatcher;
use super::entry::StreamEntry;
use super::runner::Runner;
use super::transport::Transport;
use arc_swap::ArcSwap;
use google_cloud_gax::backoff_policy::BackoffPolicy;
use google_cloud_gax::retry_policy::RetryPolicy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::task::JoinHandle;

const WATCHDOG_INTERVAL: Duration = Duration::from_millis(500);
const LOAD_DELTA_THRESHOLD: f64 = 1.2;

#[derive(Debug)]
pub(crate) struct PoolState {
    pub(crate) streams: Vec<StreamEntry>,
    pub(crate) dispatchers: Vec<Weak<Dispatcher>>,
}

impl std::ops::Deref for PoolState {
    type Target = Vec<StreamEntry>;

    fn deref(&self) -> &Self::Target {
        &self.streams
    }
}

impl std::ops::DerefMut for PoolState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.streams
    }
}

#[derive(Debug)]
struct StreamTracker {
    entry: StreamEntry,
    effective_load: f64,
    dispatchers: Vec<Arc<Dispatcher>>,
}

impl StreamTracker {
    fn distinct_tables(&self) -> usize {
        let mut tables: Vec<&str> = self
            .dispatchers
            .iter()
            .map(|d| d.write_stream.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        tables.sort_unstable();
        tables.dedup();
        tables.len()
    }

    fn has_table(&self, write_stream: &str) -> bool {
        !write_stream.is_empty()
            && self
                .dispatchers
                .iter()
                .any(|d| d.write_stream == write_stream)
    }
}

#[derive(Debug)]
pub(crate) struct StreamPoolInner {
    inner: Arc<Transport>,
    next_stream_id: AtomicU64,
    // We hold the streams and registered dispatchers in a `std::sync::Mutex`
    // because we only want a single caller to be able to scale up the pool,
    // rebalance dispatchers, or remove a failed stream.
    //
    // Note that we do not acquire the lock on the hot path during writes. We
    // only acquire the lock when adding a new dispatcher, recovering from a
    // stream error, or during background watchdog pulses.
    state: Mutex<PoolState>,
    max_streams: usize,
    max_outstanding_requests: Option<u64>,
    max_outstanding_bytes: Option<u64>,
    load_threshold: f64,
}

/// A pool of open streams that supports multiplexing, load balancing, and
/// periodic background dispatcher rebalancing.
#[derive(Debug)]
pub(crate) struct StreamPool {
    inner: Arc<StreamPoolInner>,
    watchdog_task: Option<JoinHandle<()>>,
}

impl Drop for StreamPool {
    fn drop(&mut self) {
        if let Some(handle) = &self.watchdog_task {
            handle.abort();
        }
    }
}

impl StreamPool {
    /// Initializes a new [StreamPool].
    pub(crate) fn new(inner: Arc<Transport>, max_streams: usize) -> Self {
        Self::with_limits(inner, max_streams, Some(1000), None)
    }

    pub(crate) fn with_limits(
        inner: Arc<Transport>,
        max_streams: usize,
        max_outstanding_requests: Option<u64>,
        max_outstanding_bytes: Option<u64>,
    ) -> Self {
        Self::with_limits_and_threshold(
            inner,
            max_streams,
            max_outstanding_requests,
            max_outstanding_bytes,
            0.2,
        )
    }

    pub(crate) fn with_limits_and_threshold(
        inner: Arc<Transport>,
        max_streams: usize,
        max_outstanding_requests: Option<u64>,
        max_outstanding_bytes: Option<u64>,
        load_threshold: f64,
    ) -> Self {
        let max_streams = max_streams.max(1);
        let inner = Arc::new(StreamPoolInner {
            inner,
            next_stream_id: AtomicU64::new(1),
            state: Mutex::new(PoolState {
                streams: Vec::new(),
                dispatchers: Vec::new(),
            }),
            max_streams,
            max_outstanding_requests,
            max_outstanding_bytes,
            load_threshold,
        });

        let watchdog_task = if max_streams > 1 {
            Self::spawn_watchdog(Arc::downgrade(&inner))
        } else {
            None
        };

        Self {
            inner,
            watchdog_task,
        }
    }

    fn spawn_watchdog(weak_inner: Weak<StreamPoolInner>) -> Option<JoinHandle<()>> {
        let handle = tokio::runtime::Handle::try_current().ok()?;
        let start = tokio::time::Instant::now() + WATCHDOG_INTERVAL;
        let mut interval = tokio::time::interval_at(start, WATCHDOG_INTERVAL);
        Some(handle.spawn(async move {
            loop {
                interval.tick().await;
                let Some(inner) = weak_inner.upgrade() else {
                    break;
                };
                inner.watchdog_pulse();
            }
        }))
    }

    /// Atomically selects a stream for `write_stream`, creates a `Dispatcher`,
    /// and registers it with the pool.
    pub(crate) fn create_dispatcher(
        self: &Arc<Self>,
        write_stream: String,
        retry_policy: Arc<dyn RetryPolicy>,
        backoff_policy: Arc<dyn BackoffPolicy>,
        attempt_timeout: Option<Duration>,
    ) -> Arc<Dispatcher> {
        let mut state = self.inner.state.lock().unwrap();
        let stream = self.inner.get_for_stream_impl(&mut state, &write_stream);
        let dispatcher = Arc::new(Dispatcher {
            pool: self.clone(),
            entry: ArcSwap::from_pointee(stream),
            write_stream,
            retry_policy,
            backoff_policy,
            attempt_timeout,
        });
        state.dispatchers.push(Arc::downgrade(&dispatcher));
        dispatcher
    }

    /// Returns a stream.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn get(&self) -> StreamEntry {
        self.inner.get()
    }

    /// Evicts a failed stream and replaces it in-place.
    ///
    /// If multiple callers report the same stream ID simultaneously,
    /// only the first caller provisions a replacement.
    pub(crate) fn evict_and_replace(&self, failed_id: u64) -> StreamEntry {
        self.inner.evict_and_replace(failed_id)
    }

    /// Executes a single watchdog pulse to scale up the pool and rebalance dispatchers.
    #[cfg(test)]
    pub(crate) fn watchdog_pulse(&self) {
        self.inner.watchdog_pulse();
    }
}

impl StreamPoolInner {
    fn get(&self) -> StreamEntry {
        let mut state = self.state.lock().unwrap();
        self.get_impl(&mut state.streams)
    }

    fn evict_and_replace(&self, failed_id: u64) -> StreamEntry {
        let mut state = self.state.lock().unwrap();
        if let Some(pos) = state.streams.iter().position(|entry| entry.id == failed_id) {
            // If we have not yet replaced the failed stream, do so.
            let stream = self.new_stream_entry();
            state.streams[pos] = stream.clone();
            let replacement = Arc::new(stream.clone());
            state.dispatchers.retain(|weak| {
                if let Some(dispatcher) = weak.upgrade() {
                    if dispatcher.entry.load().id == failed_id {
                        dispatcher.entry.store(replacement.clone());
                    }
                    true
                } else {
                    false
                }
            });
            return stream;
        }
        self.get_impl(&mut state.streams)
    }

    pub(crate) fn watchdog_pulse(&self) {
        let mut state = self.state.lock().unwrap();
        self.watchdog_pulse_impl(&mut state);
    }

    fn watchdog_pulse_impl(&self, state: &mut PoolState) {
        let mut live_dispatchers = Vec::with_capacity(state.dispatchers.len());
        state.dispatchers.retain(|weak| {
            if let Some(strong) = weak.upgrade() {
                live_dispatchers.push(strong);
                true
            } else {
                false
            }
        });

        if live_dispatchers.is_empty() {
            return;
        }

        if state.streams.is_empty() {
            state.streams.push(self.new_stream_entry());
        }

        let mut tracked_streams: Vec<StreamTracker> = state
            .streams
            .iter()
            .map(|entry| StreamTracker {
                entry: entry.clone(),
                effective_load: self.normalize_load(entry),
                dispatchers: Vec::new(),
            })
            .collect();

        for dispatcher in &live_dispatchers {
            let current_id = dispatcher.entry.load().id;
            if let Some(tracker) = tracked_streams
                .iter_mut()
                .find(|t| t.entry.id == current_id)
            {
                tracker.dispatchers.push(dispatcher.clone());
            } else {
                // Reassign dispatchers pointing to an evicted stream, preferring a non-loaded
                // stream that already serves the same table, or the least-loaded active stream.
                let same_table_idx = tracked_streams
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| {
                        t.has_table(&dispatcher.write_stream)
                            && t.effective_load <= self.load_threshold
                    })
                    .min_by(|(_, a), (_, b)| {
                        a.effective_load
                            .total_cmp(&b.effective_load)
                            .then_with(|| a.dispatchers.len().cmp(&b.dispatchers.len()))
                    })
                    .map(|(idx, _)| idx);

                let target_idx = same_table_idx.unwrap_or_else(|| {
                    tracked_streams
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            a.effective_load
                                .total_cmp(&b.effective_load)
                                .then_with(|| a.distinct_tables().cmp(&b.distinct_tables()))
                                .then_with(|| a.dispatchers.len().cmp(&b.dispatchers.len()))
                        })
                        .map(|(idx, _)| idx)
                        .expect("tracked_streams is non-empty")
                });

                let target_tracker = &mut tracked_streams[target_idx];
                dispatcher
                    .entry
                    .store(Arc::new(target_tracker.entry.clone()));
                target_tracker.dispatchers.push(dispatcher.clone());
            }
        }

        let max_iterations = live_dispatchers.len().saturating_add(self.max_streams);
        for _ in 0..max_iterations {
            // Find the most-loaded stream that has > 1 dispatcher (splittable).
            let Some((max_idx, _)) = tracked_streams
                .iter()
                .enumerate()
                .filter(|(_, s)| s.dispatchers.len() > 1)
                .max_by(|(_, a), (_, b)| {
                    a.effective_load
                        .total_cmp(&b.effective_load)
                        .then_with(|| a.distinct_tables().cmp(&b.distinct_tables()))
                        .then_with(|| a.dispatchers.len().cmp(&b.dispatchers.len()))
                })
            else {
                break;
            };

            let max_distinct_tables = tracked_streams[max_idx].distinct_tables();
            let candidate_stream = tracked_streams[max_idx]
                .dispatchers
                .last()
                .map(|d| d.write_stream.clone())
                .unwrap_or_default();

            let group_len = if max_distinct_tables > 1 && !candidate_stream.is_empty() {
                tracked_streams[max_idx]
                    .dispatchers
                    .iter()
                    .filter(|d| d.write_stream == candidate_stream)
                    .count()
            } else {
                1
            };

            // Find the best target stream to receive the moved dispatcher(s).
            let (mut min_idx, _) = tracked_streams
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.effective_load
                        .total_cmp(&b.effective_load)
                        .then_with(|| a.distinct_tables().cmp(&b.distinct_tables()))
                        .then_with(|| a.dispatchers.len().cmp(&b.dispatchers.len()))
                })
                .expect("tracked_streams is non-empty");

            let min_has_different_table = !candidate_stream.is_empty()
                && tracked_streams[min_idx].distinct_tables() > 0
                && !tracked_streams[min_idx].has_table(&candidate_stream);

            // If the target stream is loaded (or already serves a different table) and we can grow,
            // spawn a new stream.
            if tracked_streams[max_idx].effective_load > self.load_threshold
                && tracked_streams.len() < self.max_streams
                && (tracked_streams[min_idx].effective_load > self.load_threshold
                    || min_idx == max_idx
                    || min_has_different_table)
            {
                let new_entry = self.new_stream_entry();
                state.streams.push(new_entry.clone());
                tracked_streams.push(StreamTracker {
                    entry: new_entry,
                    effective_load: 0.0,
                    dispatchers: Vec::new(),
                });
                min_idx = tracked_streams.len() - 1;
            }

            if max_idx == min_idx {
                break;
            }

            let max_load = tracked_streams[max_idx].effective_load;
            let min_load = tracked_streams[min_idx].effective_load;
            let max_dispatchers_count = tracked_streams[max_idx].dispatchers.len();
            let delta = max_load * (group_len as f64) / (max_dispatchers_count as f64);
            let new_max_load = max_load - delta;
            let new_min_load = min_load + delta;
            let strictly_reduces_peak =
                f64::max(new_max_load, new_min_load) < max_load * (1.0 - 1e-9);

            if max_load >= min_load * LOAD_DELTA_THRESHOLD && strictly_reduces_peak {
                let target_entry = Arc::new(tracked_streams[min_idx].entry.clone());
                let moved: Vec<Arc<Dispatcher>> = if group_len > 1 {
                    let mut extracted = Vec::with_capacity(group_len);
                    tracked_streams[max_idx].dispatchers.retain(|d| {
                        if d.write_stream == candidate_stream {
                            extracted.push(d.clone());
                            false
                        } else {
                            true
                        }
                    });
                    extracted
                } else {
                    vec![
                        tracked_streams[max_idx]
                            .dispatchers
                            .pop()
                            .expect("max_idx has > 1 dispatcher"),
                    ]
                };

                for dispatcher in moved {
                    dispatcher.entry.store(target_entry.clone());
                    tracked_streams[min_idx].dispatchers.push(dispatcher);
                }
                tracked_streams[max_idx].effective_load = new_max_load;
                tracked_streams[min_idx].effective_load = new_min_load;
            } else {
                break;
            }
        }
    }

    /// Selects a stream for a new `Dispatcher` targeting `write_stream`.
    ///
    /// When `write_stream` is non-empty:
    /// 1. **Table Affinity**: Reuses an existing non-overloaded stream already serving
    ///    `write_stream`, keeping all writers for the same table colocated on one stream.
    /// 2. **Unclaimed Idle Stream**: Reuses an existing non-overloaded stream that has no
    ///    assigned table yet.
    /// 3. **Scale Out**: If the pool has not reached `max_streams`, provisions a new stream
    ///    so distinct tables get dedicated streams rather than thrashing on a shared stream.
    /// 4. **Balanced Sharding**: When `max_streams` is reached, assigns the table to the
    ///    stream with the fewest distinct tables and lowest load.
    fn get_for_stream_impl(&self, state: &mut PoolState, write_stream: &str) -> StreamEntry {
        if write_stream.is_empty() {
            return self.get_impl(&mut state.streams);
        }

        let mut live_dispatchers = Vec::with_capacity(state.dispatchers.len());
        state.dispatchers.retain(|weak| {
            if let Some(strong) = weak.upgrade() {
                live_dispatchers.push(strong);
                true
            } else {
                false
            }
        });

        struct StreamCandidate {
            index: usize,
            load: f64,
            is_loaded: bool,
            has_same_table: bool,
            distinct_tables: usize,
            dispatcher_count: usize,
        }

        let candidates: Vec<StreamCandidate> = state
            .streams
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let load = self.normalize_load(entry);
                let is_loaded = self.is_loaded(entry);
                let mut tables: Vec<&str> = Vec::new();
                let mut dispatcher_count = 0usize;
                let mut has_same_table = false;
                for d in &live_dispatchers {
                    if d.entry.load().id == entry.id {
                        dispatcher_count += 1;
                        if !d.write_stream.is_empty() {
                            if d.write_stream == write_stream {
                                has_same_table = true;
                            }
                            tables.push(d.write_stream.as_str());
                        }
                    }
                }
                tables.sort_unstable();
                tables.dedup();
                StreamCandidate {
                    index,
                    load,
                    is_loaded,
                    has_same_table,
                    distinct_tables: tables.len(),
                    dispatcher_count,
                }
            })
            .collect();

        // 1. Prefer an existing non-overloaded stream that already serves `write_stream`.
        if let Some(best) = candidates
            .iter()
            .filter(|c| c.has_same_table && !c.is_loaded)
            .min_by(|a, b| {
                a.load
                    .total_cmp(&b.load)
                    .then_with(|| a.dispatcher_count.cmp(&b.dispatcher_count))
            })
        {
            return state.streams[best.index].clone();
        }

        // 2. Prefer an existing non-overloaded stream that is not yet serving any table.
        if let Some(best) = candidates
            .iter()
            .filter(|c| c.distinct_tables == 0 && !c.is_loaded)
            .min_by(|a, b| {
                a.load
                    .total_cmp(&b.load)
                    .then_with(|| a.dispatcher_count.cmp(&b.dispatcher_count))
            })
        {
            return state.streams[best.index].clone();
        }

        // 3. Grow the pool if capacity permits so this table gets a dedicated stream.
        if state.streams.len() < self.max_streams {
            let stream = self.new_stream_entry();
            state.streams.push(stream.clone());
            return stream;
        }

        // 4. Pool is at `max_streams`; shard evenly across streams by table count and load.
        let best = candidates
            .iter()
            .min_by(|a, b| match (a.is_loaded, b.is_loaded) {
                (false, false) => a
                    .distinct_tables
                    .cmp(&b.distinct_tables)
                    .then_with(|| a.load.total_cmp(&b.load))
                    .then_with(|| a.dispatcher_count.cmp(&b.dispatcher_count)),
                (true, true) => a
                    .load
                    .total_cmp(&b.load)
                    .then_with(|| a.distinct_tables.cmp(&b.distinct_tables))
                    .then_with(|| a.dispatcher_count.cmp(&b.dispatcher_count)),
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
            })
            .expect("streams is non-empty when max_streams >= 1");

        state.streams[best.index].clone()
    }

    /// Selects the stream connection with the least load.
    ///
    /// If necessary, the pool will dynamically scale up.
    ///
    /// This is only called when...
    /// - a new writer is added
    /// - a stream fails with a transient error
    fn get_impl(&self, streams: &mut Vec<StreamEntry>) -> StreamEntry {
        let least_loaded = streams.iter().min_by(|a, b| {
            let load_a = self.normalize_load(a);
            let load_b = self.normalize_load(b);
            load_a.total_cmp(&load_b)
        });
        let should_grow = least_loaded.is_none_or(|s| self.is_loaded(s));
        if streams.len() < self.max_streams && should_grow {
            // If we can and should scale up, do so.
            let stream = self.new_stream_entry();
            streams.push(stream.clone());
            return stream;
        }
        match least_loaded {
            Some(s) => s.clone(),
            None => unreachable!("this can only happen when `max_streams == 0`"),
        }
    }

    fn new_stream_entry(&self) -> StreamEntry {
        let id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
        let runner = Runner::new(self.inner.clone());

        StreamEntry {
            id,
            req_tx: runner.req_tx,
            outstanding_requests: Arc::new(AtomicU64::new(0)),
            outstanding_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Estimate the current load on the stream.
    ///
    /// Our best proxy is to use outstanding requests, outstanding bytes.
    fn normalize_load(&self, entry: &StreamEntry) -> f64 {
        let r = self
            .max_outstanding_requests
            .filter(|&m| m > 0)
            .map(|m| entry.outstanding_requests.load(Ordering::Relaxed) as f64 / m as f64)
            .unwrap_or_default();
        let b = self
            .max_outstanding_bytes
            .filter(|&m| m > 0)
            .map(|m| entry.outstanding_bytes.load(Ordering::Relaxed) as f64 / m as f64)
            .unwrap_or_default();
        f64::max(r, b)
    }

    /// Determine if the stream is approaching load
    fn is_loaded(&self, entry: &StreamEntry) -> bool {
        self.normalize_load(entry) > self.load_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::super::runner::WriteRequest;
    use super::*;
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::Response as TonicResponse;
    use std::sync::MutexGuard;
    use test_case::test_case;
    use tokio::sync::{mpsc, oneshot};
    use tokio::task::JoinSet;

    #[test_case(10, Some(100), 10_000, Some(100_000), 0.1, false)]
    #[test_case(90, Some(100), 10_000, Some(100_000), 0.9, true)]
    #[test_case(10, Some(100), 90_000, Some(100_000), 0.9, true)]
    #[test_case(90, Some(100), 90_000, Some(100_000), 0.9, true)]
    #[test_case(10, None, 10_000, Some(100_000), 0.1, false)]
    #[test_case(10, Some(100), 10_000, None, 0.1, false)]
    #[test_case(10, None, 10_000, None, 0.0, false)]
    #[tokio::test]
    async fn load_math(
        requests: u64,
        max_outstanding_requests: Option<u64>,
        bytes: u64,
        max_outstanding_bytes: Option<u64>,
        expected_load: f64,
        expected_is_loaded: bool,
    ) -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = StreamPool::with_limits(
            transport,
            10,
            max_outstanding_requests,
            max_outstanding_bytes,
        );

        let s = pool.inner.new_stream_entry();
        s.outstanding_requests.store(requests, Ordering::Relaxed);
        s.outstanding_bytes.store(bytes, Ordering::Relaxed);

        assert_eq!(pool.inner.normalize_load(&s), expected_load);
        assert_eq!(pool.inner.is_loaded(&s), expected_is_loaded);
        Ok(())
    }

    #[tokio::test]
    async fn empty_pool_get_basic() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(|_| Ok(TonicResponse::from(response_rx)));
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = StreamPool::new(transport, 10);

        let s1 = pool.get();
        assert_eq!(s1.id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        let s2 = pool.get();
        assert_eq!(s2.id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        // Use the stream handles to send requests on the same underlying gRPC
        // stream.

        // write 1, from stream 1
        let (resp_tx1, resp_rx1) = oneshot::channel();
        let write1 = WriteRequest {
            req: test_request(1),
            resp_tx: resp_tx1,
        };
        s1.req_tx.send(write1)?;

        // write 2, from stream 2
        let (resp_tx2, resp_rx2) = oneshot::channel();
        let write2 = WriteRequest {
            req: test_request(2),
            resp_tx: resp_tx2,
        };
        s2.req_tx.send(write2)?;

        // resp 1
        response_tx.send(Ok(convert(&test_response(1)))).await?;
        let resp1 = resp_rx1.await??;
        assert_eq!(resp1, test_response(1));

        // resp 2
        response_tx.send(Ok(convert(&test_response(2)))).await?;
        let resp2 = resp_rx2.await??;
        assert_eq!(resp2, test_response(2));

        Ok(())
    }

    #[tokio::test]
    async fn empty_pool_get_lock_contention() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        // Disable load tracking. We should never scale past a single stream.
        let pool = Arc::new(StreamPool::with_limits(transport, 10, None, None));

        let mut streams = JoinSet::new();
        for _ in 0..1000 {
            let p = pool.clone();
            streams.spawn(async move { p.get() });
        }
        // Verify each stream handle is for ID 1.
        while let Some(s) = streams.join_next().await {
            assert_eq!(s?.id, 1);
        }
        // Verify only one stream is created total.
        assert_eq!(pool.stream_ids(), [1]);

        Ok(())
    }

    #[tokio::test]
    async fn get_least_loaded() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = StreamPool::new(transport, 10);

        // Manually seed the pool
        pool.seed([8, 2, 2, 3, 1, 9]);

        let s = pool.get();
        assert_eq!(s.id, 5);

        Ok(())
    }

    #[tokio::test]
    async fn get_should_grow() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::with_limits_and_threshold(
            transport,
            10,
            Some(3),
            None,
            0.5,
        ));

        let s = pool.get();
        assert_eq!(s.id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        // Simulate an in-flight request on the stream. It should not be loaded
        // yet.
        s.outstanding_requests.fetch_add(1, Ordering::Relaxed);
        let s = pool.get();
        assert_eq!(s.id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        // Simulate another in-flight request on the stream. Now it should be at
        // load. We should grow the pool.
        s.outstanding_requests.fetch_add(1, Ordering::Relaxed);
        let s = pool.get();
        assert_eq!(s.id, 2);
        assert_eq!(pool.stream_ids(), [1, 2]);

        // Simulate an in-flight request on the second stream. It should not be
        // loaded yet.
        s.outstanding_requests.fetch_add(1, Ordering::Relaxed);
        let s = pool.get();
        assert_eq!(s.id, 2);
        assert_eq!(pool.stream_ids(), [1, 2]);

        // Simulate another in-flight request on the second stream. Now it should be at
        // load. We should grow the pool again.
        s.outstanding_requests.fetch_add(1, Ordering::Relaxed);
        let s = pool.get();
        assert_eq!(s.id, 3);
        assert_eq!(pool.stream_ids(), [1, 2, 3]);

        Ok(())
    }

    #[tokio::test]
    async fn fully_loaded_get() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::with_limits(transport, 6, Some(10), None));

        // Manually seed the pool to its limit (`max_streams`). Note that all
        // streams are already at load.
        pool.seed([8, 5, 3, 8, 9, 11]);
        assert_eq!(pool.stream_ids().len(), 6);

        // We should return the least loaded stream without growing the pool.
        let s = pool.get();
        assert_eq!(s.id, 3);
        assert_eq!(pool.stream_ids().len(), 6);

        Ok(())
    }

    #[tokio::test]
    async fn evict_basic() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = StreamPool::new(transport, 10);

        // Manually seed the pool
        pool.seed([1, 2, 3, 4, 5, 6]);
        assert_eq!(pool.stream_ids(), [1, 2, 3, 4, 5, 6]);

        let s = pool.evict_and_replace(3);
        assert_eq!(s.id, 7);
        assert_eq!(pool.inner.normalize_load(&s), 0.0);
        assert_eq!(pool.stream_ids(), [1, 2, 4, 5, 6, 7]);

        let s = pool.evict_and_replace(6);
        assert_eq!(s.id, 8);
        assert_eq!(pool.inner.normalize_load(&s), 0.0);
        assert_eq!(pool.stream_ids(), [1, 2, 4, 5, 7, 8]);

        Ok(())
    }

    #[tokio::test]
    async fn evict_lock_contention() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::new(transport, 10));

        // Manually seed the pool
        pool.seed([1]);

        let mut streams = JoinSet::new();
        for _ in 0..1000 {
            let p = pool.clone();
            streams.spawn(async move { p.evict_and_replace(1) });
        }
        // Verify each stream handle is for ID 2.
        while let Some(s) = streams.join_next().await {
            assert_eq!(s?.id, 2);
        }
        // Verify the pool stays at one stream total.
        assert_eq!(pool.stream_ids(), [2]);

        Ok(())
    }

    fn test_pool_dispatcher(pool: &Arc<StreamPool>) -> Arc<Dispatcher> {
        Dispatcher::new(
            pool.clone(),
            test_retry_policy(),
            test_backoff_policy(),
            None,
        )
    }

    #[tokio::test]
    async fn watchdog_scales_up_and_rebalances_writers() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::with_limits(transport, 4, Some(100), None));

        // Attach 8 writers before any writes are in flight (load == 0).
        // All 8 writers get assigned to stream ID 1 initially.
        let writers: Vec<_> = (0..8).map(|_| test_pool_dispatcher(&pool)).collect();
        assert_eq!(pool.stream_ids(), [1]);
        for w in &writers {
            assert_eq!(w.entry.load().id, 1);
        }

        // Simulate heavy load on stream 1 (80 outstanding requests -> 0.8 load > 0.2 threshold).
        writers[0]
            .entry
            .load()
            .outstanding_requests
            .store(80, Ordering::Relaxed);

        // Trigger a watchdog pulse.
        pool.watchdog_pulse();

        // Pool should scale up to 4 streams (0.8 / 4 = 0.2 per stream).
        assert_eq!(pool.stream_ids(), [1, 2, 3, 4]);

        // Writers should be evenly balanced: 2 writers per stream.
        for stream_id in 1..=4 {
            let count = writers
                .iter()
                .filter(|w| w.entry.load().id == stream_id)
                .count();
            assert_eq!(count, 2, "stream {stream_id} should have 2 writers");
        }

        Ok(())
    }

    #[tokio::test]
    async fn watchdog_respects_max_streams_and_rebalances_skewed_load() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::with_limits(transport, 2, Some(100), None));

        let writers: Vec<_> = (0..6).map(|_| test_pool_dispatcher(&pool)).collect();
        assert_eq!(pool.stream_ids(), [1]);

        // Set load to 1.0 (100 outstanding requests). Even though 1.0 / 0.2 = 5 streams,
        // max_streams is capped at 2.
        writers[0]
            .entry
            .load()
            .outstanding_requests
            .store(100, Ordering::Relaxed);

        pool.watchdog_pulse();

        assert_eq!(pool.stream_ids(), [1, 2]);
        let count_1 = writers.iter().filter(|w| w.entry.load().id == 1).count();
        let count_2 = writers.iter().filter(|w| w.entry.load().id == 2).count();
        assert_eq!(count_1, 3);
        assert_eq!(count_2, 3);

        Ok(())
    }

    #[tokio::test]
    async fn watchdog_prunes_dropped_writers() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::new(transport, 4));

        let w1 = test_pool_dispatcher(&pool);
        let w2 = test_pool_dispatcher(&pool);
        let _w3 = test_pool_dispatcher(&pool);
        assert_eq!(pool.dispatcher_count(), 3);

        drop(w1);
        drop(w2);

        pool.watchdog_pulse();
        assert_eq!(pool.dispatcher_count(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn watchdog_does_not_split_single_writer() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::with_limits(transport, 4, Some(100), None));

        let w1 = test_pool_dispatcher(&pool);
        w1.entry
            .load()
            .outstanding_requests
            .store(90, Ordering::Relaxed);

        // Even though load is 0.9 > 0.2, there is only 1 writer, which cannot be split.
        pool.watchdog_pulse();
        assert_eq!(pool.stream_ids(), [1]);
        assert_eq!(w1.entry.load().id, 1);

        Ok(())
    }

    #[tokio::test]
    async fn watchdog_reassigns_writers_on_evicted_stream() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::new(transport, 4));

        let w1 = test_pool_dispatcher(&pool);
        let w2 = test_pool_dispatcher(&pool);
        assert_eq!(w1.entry.load().id, 1);
        assert_eq!(w2.entry.load().id, 1);

        // Evicting stream 1 replaces it with stream 2 and updates registered writers.
        let s2 = pool.evict_and_replace(1);
        assert_eq!(s2.id, 2);
        assert_eq!(w1.entry.load().id, 2);
        assert_eq!(w2.entry.load().id, 2);

        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn watchdog_background_task_ticks_automatically() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::with_limits(transport, 4, Some(100), None));

        let writers: Vec<_> = (0..4).map(|_| test_pool_dispatcher(&pool)).collect();
        assert_eq!(pool.stream_ids(), [1]);

        writers[0]
            .entry
            .load()
            .outstanding_requests
            .store(100, Ordering::Relaxed);

        // Advance time past WATCHDOG_INTERVAL (500ms) and yield to let the spawned task run.
        tokio::time::advance(WATCHDOG_INTERVAL + Duration::from_millis(10)).await;
        tokio::task::yield_now().await;

        assert_eq!(pool.stream_ids(), [1, 2, 3, 4]);
        for stream_id in 1..=4 {
            let count = writers
                .iter()
                .filter(|w| w.entry.load().id == stream_id)
                .count();
            assert_eq!(count, 1);
        }

        Ok(())
    }

    fn test_pool_table_dispatcher(pool: &Arc<StreamPool>, table_idx: usize) -> Arc<Dispatcher> {
        let write_stream =
            format!("projects/p/datasets/d/tables/table_{table_idx}/streams/_default");
        Dispatcher::new_with_stream(
            pool.clone(),
            write_stream,
            test_retry_policy(),
            test_backoff_policy(),
            None,
        )
    }

    #[tokio::test]
    async fn table_affinity_scales_to_dedicated_streams_per_table() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        // Simulate b/561726629: 256 writers across 64 tables with multiplex_pool_size = 64.
        let num_tables = 64;
        let num_writers = 256;
        let pool = Arc::new(StreamPool::with_limits(
            transport,
            num_tables,
            Some(200),
            None,
        ));

        let writers: Vec<_> = (0..num_writers)
            .map(|i| test_pool_table_dispatcher(&pool, i % num_tables))
            .collect();

        // Pool must immediately scale out to all 64 streams (1 dedicated stream per table).
        assert_eq!(pool.stream_ids().len(), num_tables);

        // All 4 writers for any given table must be colocated on the exact same stream.
        for t in 0..num_tables {
            let expected_stream = writers[t].entry.load().id;
            assert_eq!(expected_stream, (t + 1) as u64);
            for w_idx in (t..num_writers).step_by(num_tables) {
                assert_eq!(
                    writers[w_idx].entry.load().id,
                    expected_stream,
                    "writer {w_idx} for table_{t} should share stream {expected_stream}"
                );
            }
        }

        // Each stream must serve exclusively 1 table (4 writers).
        for stream_id in 1..=(num_tables as u64) {
            let mut tables: Vec<_> = writers
                .iter()
                .filter(|w| w.entry.load().id == stream_id)
                .map(|w| w.write_stream.as_str())
                .collect();
            assert_eq!(tables.len(), 4);
            tables.sort_unstable();
            tables.dedup();
            assert_eq!(
                tables.len(),
                1,
                "stream {stream_id} should serve exactly 1 table"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn table_affinity_shards_evenly_when_tables_exceed_max_streams() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let max_streams = 16;
        let num_tables = 64;
        let num_writers = 256;
        let pool = Arc::new(StreamPool::with_limits(
            transport,
            max_streams,
            Some(200),
            None,
        ));

        let writers: Vec<_> = (0..num_writers)
            .map(|i| test_pool_table_dispatcher(&pool, i % num_tables))
            .collect();

        assert_eq!(pool.stream_ids().len(), max_streams);

        // Every table's 4 writers must still be colocated on the same stream.
        for t in 0..num_tables {
            let expected_stream = writers[t].entry.load().id;
            for w_idx in (t..num_writers).step_by(num_tables) {
                assert_eq!(writers[w_idx].entry.load().id, expected_stream);
            }
        }

        // The 64 tables must be evenly sharded across the 16 streams (4 tables / 16 writers per stream).
        for stream_id in 1..=(max_streams as u64) {
            let mut tables: Vec<_> = writers
                .iter()
                .filter(|w| w.entry.load().id == stream_id)
                .map(|w| w.write_stream.as_str())
                .collect();
            assert_eq!(
                tables.len(),
                16,
                "stream {stream_id} should have 16 writers"
            );
            tables.sort_unstable();
            tables.dedup();
            assert_eq!(
                tables.len(),
                4,
                "stream {stream_id} should have 4 distinct tables"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn watchdog_preserves_table_group_cohesion_when_splitting_multi_table_stream()
    -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::with_limits(transport, 4, Some(100), None));

        // Create 8 writers across 4 tables (2 writers per table), and force them all onto
        // stream 1 initially (simulating post-eviction or pre-existing colocated state).
        let writers: Vec<_> = (0..8)
            .map(|i| test_pool_table_dispatcher(&pool, i % 4))
            .collect();
        let stream_1 = writers[0].entry.load_full();
        for w in &writers {
            w.entry.store(stream_1.clone());
        }
        pool.inner.state.lock().unwrap().streams.truncate(1);
        assert_eq!(pool.stream_ids(), [1]);

        // Simulate heavy load on stream 1 (80 outstanding requests -> 0.8 load > 0.2 threshold).
        stream_1.outstanding_requests.store(80, Ordering::Relaxed);

        pool.watchdog_pulse();

        // Watchdog should scale out to 4 streams and move whole table groups together
        // so that each table's 2 writers remain on the same stream.
        assert_eq!(pool.stream_ids().len(), 4);
        for t in 0..4 {
            assert_eq!(
                writers[t].entry.load().id,
                writers[t + 4].entry.load().id,
                "both writers for table_{t} should stay colocated on the same stream"
            );
        }
        for stream_id in pool.stream_ids() {
            let mut tables: Vec<_> = writers
                .iter()
                .filter(|w| w.entry.load().id == stream_id)
                .map(|w| w.write_stream.as_str())
                .collect();
            assert_eq!(tables.len(), 2);
            tables.sort_unstable();
            tables.dedup();
            assert_eq!(tables.len(), 1);
        }

        Ok(())
    }

    impl StreamPool {
        // Seed the pool with loaded streams to simplify testing.
        pub(crate) fn seed(&self, loads: impl IntoIterator<Item = u64>) {
            for load in loads.into_iter() {
                let s = self.inner.new_stream_entry();
                s.outstanding_requests.store(load, Ordering::Relaxed);
                self.inner.state.lock().unwrap().streams.push(s);
            }
        }

        // Returns the stream IDs in the pool, in order.
        pub(crate) fn stream_ids(&self) -> Vec<u64> {
            let mut ids: Vec<_> = self
                .inner
                .state
                .lock()
                .unwrap()
                .streams
                .iter()
                .map(|s| s.id)
                .collect();
            ids.sort();
            ids
        }

        // Returns the count of registered dispatcher entries in the pool.
        pub(crate) fn dispatcher_count(&self) -> usize {
            self.inner.state.lock().unwrap().dispatchers.len()
        }

        // Acquire the stream pool state lock
        pub(crate) fn lock(&self) -> MutexGuard<'_, PoolState> {
            self.inner.state.lock().unwrap()
        }
    }
}
