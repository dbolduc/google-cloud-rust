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

    /// Registers a `Dispatcher` with the pool so the background watchdog can
    /// rebalance its stream assignment without keeping dropped dispatchers alive.
    pub(crate) fn register_dispatcher(&self, dispatcher: Weak<Dispatcher>) {
        self.inner.register_dispatcher(dispatcher);
    }

    /// Returns a stream.
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
    fn register_dispatcher(&self, dispatcher: Weak<Dispatcher>) {
        let mut state = self.state.lock().unwrap();
        state.dispatchers.push(dispatcher);
    }

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
                // Reassign dispatchers pointing to an evicted stream to the least-loaded active stream.
                let min_tracker = tracked_streams
                    .iter_mut()
                    .min_by(|a, b| a.effective_load.total_cmp(&b.effective_load))
                    .expect("tracked_streams is non-empty");
                dispatcher.entry.store(Arc::new(min_tracker.entry.clone()));
                min_tracker.dispatchers.push(dispatcher.clone());
            }
        }

        let max_iterations = live_dispatchers.len().saturating_add(self.max_streams);
        for _ in 0..max_iterations {
            // Find the most-loaded stream that has > 1 dispatcher (splittable).
            let Some((max_idx, _)) = tracked_streams
                .iter()
                .enumerate()
                .filter(|(_, s)| s.dispatchers.len() > 1)
                .max_by(|(_, a), (_, b)| a.effective_load.total_cmp(&b.effective_load))
            else {
                break;
            };

            // Find the least-loaded stream overall.
            let (mut min_idx, _) = tracked_streams
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.effective_load.total_cmp(&b.effective_load))
                .expect("tracked_streams is non-empty");

            // If the least-loaded stream is loaded and we can grow, spawn a new stream.
            if tracked_streams[min_idx].effective_load > self.load_threshold
                && tracked_streams.len() < self.max_streams
                && tracked_streams[max_idx].effective_load > self.load_threshold
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
            let delta = max_load / (max_dispatchers_count as f64);
            let new_max_load = max_load - delta;
            let new_min_load = min_load + delta;
            let strictly_reduces_peak =
                f64::max(new_max_load, new_min_load) < max_load * (1.0 - 1e-9);

            if max_load >= min_load * LOAD_DELTA_THRESHOLD && strictly_reduces_peak {
                let dispatcher = tracked_streams[max_idx]
                    .dispatchers
                    .pop()
                    .expect("max_idx has > 1 dispatcher");
                let target_entry = tracked_streams[min_idx].entry.clone();
                dispatcher.entry.store(Arc::new(target_entry));
                tracked_streams[min_idx].dispatchers.push(dispatcher);
                tracked_streams[max_idx].effective_load = new_max_load;
                tracked_streams[min_idx].effective_load = new_min_load;
            } else {
                break;
            }
        }
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
