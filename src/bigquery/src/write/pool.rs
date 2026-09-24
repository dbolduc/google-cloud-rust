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

use super::entry::StreamEntry;
use super::runner::Runner;
use super::transport::Transport;
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

const WATCHDOG_INTERVAL: Duration = Duration::from_millis(500);
const LOAD_DELTA_THRESHOLD: f64 = 1.2;

/// Configuration options for the stream pool.
#[derive(Debug)]
pub(crate) struct StreamPoolOptions {
    pub(crate) max_streams: usize,
    pub(crate) max_outstanding_requests: Option<u64>,
    pub(crate) max_outstanding_bytes: Option<u64>,
    pub(crate) load_threshold: f64,
}

impl Default for StreamPoolOptions {
    fn default() -> Self {
        Self {
            max_streams: 8,
            max_outstanding_requests: Some(1000),
            max_outstanding_bytes: None,
            load_threshold: 0.2,
        }
    }
}

/// A pool of open streams that supports multiplexing, load balancing.
#[derive(Debug)]
pub(crate) struct StreamPool {
    inner: Arc<Transport>,
    next_stream_id: AtomicU64,
    stream_count: AtomicUsize,
    writer_count: AtomicUsize,
    // We hold the streams in a `std::sync::Mutex` because we only want a single
    // caller to be able to scale up the pool, or remove a failed stream.
    //
    // Note that we do not acquire the lock on the hot path during writes once
    // `stream_count == max_streams`.
    streams: Mutex<Vec<StreamEntry>>,
    writers: Mutex<Vec<Weak<ArcSwap<StreamEntry>>>>,
    watchdog_started: AtomicBool,
    options: StreamPoolOptions,
}

impl StreamPool {
    /// Initializes a new [StreamPool].
    pub(crate) fn new(inner: Arc<Transport>, options: StreamPoolOptions) -> Self {
        Self {
            inner,
            next_stream_id: AtomicU64::new(1),
            stream_count: AtomicUsize::new(0),
            writer_count: AtomicUsize::new(0),
            streams: Mutex::new(Vec::new()),
            writers: Mutex::new(Vec::new()),
            watchdog_started: AtomicBool::new(false),
            options,
        }
    }

    /// Registers a writer's stream entry handle with the pool and starts the
    /// background watchdog task if multiplexing (`max_streams > 1`) is enabled.
    pub(crate) fn register_writer(self: &Arc<Self>, writer: Weak<ArcSwap<StreamEntry>>) {
        if self.options.max_streams <= 1 {
            return;
        }
        {
            let mut writers = self.writers.lock().unwrap();
            writers.push(writer);
            self.writer_count.store(writers.len(), Ordering::Relaxed);
        }
        if !self.watchdog_started.swap(true, Ordering::Relaxed) {
            let weak_pool = Arc::downgrade(self);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(WATCHDOG_INTERVAL).await;
                    let Some(pool) = weak_pool.upgrade() else {
                        break;
                    };
                    pool.watchdog_pulse();
                }
            });
        }
    }

    /// Lock-free fast check during writes: if the pool has not yet reached
    /// `max_streams` (and has more writers than open streams) and `current`
    /// exceeds `load_threshold`, attempts a non-blocking `watchdog_pulse` so an
    /// initial startup burst scales the pool immediately rather than queueing
    /// all requests onto Stream #1 before the first 500ms timer tick. Once
    /// `stream_count == min(max_streams, writer_count)`, this check
    /// short-circuits in O(1) without touching any locks.
    #[inline]
    pub(crate) fn maybe_scale_up(&self, current: &StreamEntry) -> bool {
        let stream_count = self.stream_count.load(Ordering::Relaxed);
        if stream_count < self.options.max_streams
            && self.writer_count.load(Ordering::Relaxed) > stream_count
            && self.is_loaded(current)
            && let (Ok(mut streams), Ok(mut writers)) =
                (self.streams.try_lock(), self.writers.try_lock())
        {
            self.watchdog_pulse_locked(&mut streams, &mut writers);
            return true;
        }
        false
    }

    /// Runs a single pulse of the pool watchdog: orders streams by load, grows
    /// the pool if the least-loaded stream exceeds `load_threshold`, and
    /// rebalances writers from busy streams onto idle streams.
    pub(crate) fn watchdog_pulse(&self) {
        let mut streams = self.streams.lock().unwrap();
        let mut writers = self.writers.lock().unwrap();
        self.watchdog_pulse_locked(&mut streams, &mut writers);
    }

    fn watchdog_pulse_locked(
        &self,
        streams: &mut Vec<StreamEntry>,
        writers: &mut Vec<Weak<ArcSwap<StreamEntry>>>,
    ) {
        // Remove writers that have been dropped.
        writers.retain(|w| w.strong_count() > 0);
        self.writer_count.store(writers.len(), Ordering::Relaxed);
        if writers.is_empty() || streams.is_empty() {
            return;
        }

        let live_writers: Vec<_> = writers.iter().filter_map(|w| w.upgrade()).collect();
        if live_writers.is_empty() {
            return;
        }

        // Scale up the pool when total load across open streams exceeds the
        // capacity of the current stream count (`streams.len() * load_threshold`),
        // bisecting writers from the most-populated stream onto each new stream.
        let mut scaled_up = false;
        let total_load: f64 = streams.iter().map(|s| self.normalize_load(s)).sum();
        while streams.len() < self.options.max_streams
            && live_writers.len() > streams.len()
            && total_load > (streams.len() as f64) * self.options.load_threshold
        {
            let Some((_, donor_writers)) = streams
                .iter()
                .map(|s| {
                    let matched: Vec<_> = live_writers
                        .iter()
                        .filter(|entry| entry.load().id == s.id)
                        .collect();
                    (s.id, matched)
                })
                .max_by_key(|(_, matched)| matched.len())
            else {
                break;
            };

            if donor_writers.len() <= 1 {
                break;
            }

            let new_stream = self.new_stream_entry();
            let num_to_move = (donor_writers.len() / 2).max(1);
            for candidate in donor_writers.into_iter().take(num_to_move) {
                candidate.store(Arc::new(new_stream.clone()));
            }
            streams.insert(0, new_stream);
            self.stream_count.store(streams.len(), Ordering::Relaxed);
            scaled_up = true;
        }
        if scaled_up {
            return;
        }

        // Order streams by ascending load for steady-state rebalancing.
        streams.sort_by(|a, b| self.normalize_load(a).total_cmp(&self.normalize_load(b)));

        // Rebalance writers from the busiest stream to the most idle stream if
        // the most idle stream is not loaded and the load difference exceeds
        // `LOAD_DELTA_THRESHOLD` (matching Go's `rebalanceWriters`).
        let most_idle = &streams[0];
        if self.is_loaded(most_idle) {
            return;
        }
        let most_idle_load = self.normalize_load(most_idle);
        let most_idle_writers = live_writers
            .iter()
            .filter(|entry| entry.load().id == most_idle.id)
            .count();

        for least_idle_idx in (1..streams.len()).rev() {
            let target = &streams[least_idle_idx];
            if self.normalize_load(target) < most_idle_load * LOAD_DELTA_THRESHOLD {
                return;
            }

            // Collect writers currently sharing `target`.
            let matching_writers: Vec<_> = live_writers
                .iter()
                .filter(|entry| entry.load().id == target.id)
                .collect();

            if matching_writers.len() <= 1 {
                continue;
            }

            // Avoid ping-ponging writers between equally populated active
            // streams due to transient in-flight request jitter.
            if matching_writers.len() <= most_idle_writers
                && !(self.is_loaded(target) && most_idle_load == 0.0)
            {
                continue;
            }

            if let Some(candidate) = matching_writers.first() {
                candidate.store(Arc::new(most_idle.clone()));
            }
            return;
        }
    }

    /// Returns a stream.
    pub(crate) fn get(&self) -> StreamEntry {
        let mut streams = self.streams.lock().unwrap();
        self.get_impl(&mut streams)
    }

    /// Evicts a failed stream and replaces it in-place.
    ///
    /// If multiple callers report the same stream ID simultaneously,
    /// only the first caller provisions a replacement.
    pub(crate) fn evict_and_replace(&self, failed_id: u64) -> StreamEntry {
        let mut streams = self.streams.lock().unwrap();
        if let Some(pos) = streams.iter().position(|entry| entry.id == failed_id) {
            // If we have not yet replaced the failed stream, do so.
            let stream = self.new_stream_entry();
            streams[pos] = stream.clone();
            return stream;
        }
        self.get_impl(&mut streams)
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
        if streams.len() < self.options.max_streams && should_grow {
            // If we can and should scale up, do so.
            let stream = self.new_stream_entry();
            streams.push(stream.clone());
            self.stream_count.store(streams.len(), Ordering::Relaxed);
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
            .options
            .max_outstanding_requests
            .map(|m| entry.outstanding_requests.load(Ordering::Relaxed) as f64 / m as f64)
            .unwrap_or_default();
        let b = self
            .options
            .max_outstanding_bytes
            .map(|m| entry.outstanding_bytes.load(Ordering::Relaxed) as f64 / m as f64)
            .unwrap_or_default();
        f64::max(r, b)
    }

    /// Determine if the stream is approaching load
    fn is_loaded(&self, entry: &StreamEntry) -> bool {
        self.normalize_load(entry) > self.options.load_threshold
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
        let options = StreamPoolOptions {
            max_streams: 10,
            max_outstanding_requests,
            max_outstanding_bytes,
            load_threshold: 0.2,
        };
        let pool = StreamPool::new(transport, options);

        let s = pool.new_stream_entry();
        s.outstanding_requests.store(requests, Ordering::Relaxed);
        s.outstanding_bytes.store(bytes, Ordering::Relaxed);

        assert_eq!(pool.normalize_load(&s), expected_load);
        assert_eq!(pool.is_loaded(&s), expected_is_loaded);
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
        let pool = StreamPool::new(transport, StreamPoolOptions::default());

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
        let options = StreamPoolOptions {
            max_streams: 10,
            // Disable load tracking. We should never scale past a single stream.
            max_outstanding_requests: None,
            max_outstanding_bytes: None,
            load_threshold: 0.2,
        };
        let pool = Arc::new(StreamPool::new(transport, options));

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
        let pool = StreamPool::new(transport, StreamPoolOptions::default());

        // Manually seed the pool
        pool.seed([8, 2, 2, 3, 1, 9]);

        let s = pool.get();
        assert_eq!(s.id, 5);

        Ok(())
    }

    #[tokio::test]
    async fn get_should_grow() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let options = StreamPoolOptions {
            max_streams: 10,
            max_outstanding_requests: Some(3),
            max_outstanding_bytes: None,
            load_threshold: 0.5,
        };
        let pool = Arc::new(StreamPool::new(transport, options));

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
        let options = StreamPoolOptions {
            max_streams: 6,
            max_outstanding_requests: Some(10),
            max_outstanding_bytes: None,
            load_threshold: 0.2,
        };
        let pool = Arc::new(StreamPool::new(transport, options));

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
        let pool = StreamPool::new(transport, StreamPoolOptions::default());

        // Manually seed the pool
        pool.seed([1, 2, 3, 4, 5, 6]);
        assert_eq!(pool.stream_ids(), [1, 2, 3, 4, 5, 6]);

        let s = pool.evict_and_replace(3);
        assert_eq!(s.id, 7);
        assert_eq!(pool.normalize_load(&s), 0.0);
        assert_eq!(pool.stream_ids(), [1, 2, 4, 5, 6, 7]);

        let s = pool.evict_and_replace(6);
        assert_eq!(s.id, 8);
        assert_eq!(pool.normalize_load(&s), 0.0);
        assert_eq!(pool.stream_ids(), [1, 2, 4, 5, 7, 8]);

        Ok(())
    }

    #[tokio::test]
    async fn evict_lock_contention() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let pool = Arc::new(StreamPool::new(transport, StreamPoolOptions::default()));

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

    #[tokio::test]
    async fn watchdog_scales_and_rebalances_writers() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let options = StreamPoolOptions {
            max_streams: 4,
            max_outstanding_requests: Some(1000),
            max_outstanding_bytes: None,
            load_threshold: 0.2,
        };
        let pool = Arc::new(StreamPool::new(transport, options));

        // Create 8 writers up front when load is 0. All 8 bind to stream 1.
        let mut writers = Vec::new();
        for _ in 0..8 {
            let entry = Arc::new(ArcSwap::from_pointee(pool.get()));
            pool.register_writer(Arc::downgrade(&entry));
            writers.push(entry);
        }
        assert_eq!(pool.stream_ids(), [1]);
        for w in &writers {
            assert_eq!(w.load().id, 1);
        }

        // Simulate 125 requests per writer across 6 pulses (matching 8 writers sharing 1000 permits).
        for _ in 0..6 {
            // Update each stream's outstanding_requests to reflect the number of writers assigned to it.
            for s in pool.streams.lock().unwrap().iter() {
                let assigned_count = writers.iter().filter(|w| w.load().id == s.id).count() as u64;
                s.outstanding_requests
                    .store(assigned_count * 125, Ordering::Relaxed);
            }
            pool.watchdog_pulse();
        }

        // Pool should have scaled to all 4 streams, with 2 writers on each stream.
        assert_eq!(pool.stream_ids(), [1, 2, 3, 4]);
        for id in 1..=4 {
            let assigned = writers.iter().filter(|w| w.load().id == id).count();
            assert_eq!(assigned, 2, "expected 2 writers on stream {id}");
        }

        Ok(())
    }

    #[tokio::test]
    async fn watchdog_respects_proportional_stream_capacity() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("ignored").await?);
        let options = StreamPoolOptions {
            max_streams: 4,
            max_outstanding_requests: Some(1000),
            max_outstanding_bytes: None,
            load_threshold: 0.2, // 200 requests per stream capacity
        };
        let pool = Arc::new(StreamPool::new(transport, options));

        let mut writers = Vec::new();
        for _ in 0..8 {
            let entry = Arc::new(ArcSwap::from_pointee(pool.get()));
            pool.register_writer(Arc::downgrade(&entry));
            writers.push(entry);
        }

        // 100 requests (<= 200 threshold for 1 stream): stays on 1 stream, no bisection.
        pool.streams.lock().unwrap()[0]
            .outstanding_requests
            .store(100, Ordering::Relaxed);
        pool.watchdog_pulse();
        assert_eq!(pool.stream_ids(), [1]);
        assert_eq!(writers.iter().filter(|w| w.load().id == 1).count(), 8);

        // 300 requests (> 200 for 1 stream, but <= 400 for 2 streams): scales only to 2 streams (4 writers each), NOT 4.
        pool.streams.lock().unwrap()[0]
            .outstanding_requests
            .store(300, Ordering::Relaxed);
        pool.watchdog_pulse();
        assert_eq!(pool.stream_ids(), [1, 2]);
        assert_eq!(writers.iter().filter(|w| w.load().id == 1).count(), 4);
        assert_eq!(writers.iter().filter(|w| w.load().id == 2).count(), 4);

        Ok(())
    }

    impl StreamPool {
        // Seed the pool with loaded streams to simplify testing.
        pub(crate) fn seed(&self, loads: impl IntoIterator<Item = u64>) {
            for load in loads.into_iter() {
                let s = self.new_stream_entry();
                s.outstanding_requests.store(load, Ordering::Relaxed);
                self.streams.lock().unwrap().push(s);
            }
        }

        // Returns the stream IDs in the pool, in order.
        pub(crate) fn stream_ids(&self) -> Vec<u64> {
            let mut ids: Vec<_> = self.streams.lock().unwrap().iter().map(|s| s.id).collect();
            ids.sort();
            ids
        }

        // Acquire the stream lock
        pub(crate) fn lock(&self) -> MutexGuard<'_, Vec<StreamEntry>> {
            self.streams.lock().unwrap()
        }
    }
}
