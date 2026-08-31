# BigQuery Storage Write API - Connection Pool & Stream Architecture Design

This document details the architecture and design of the stream connection pooling and write routing strategies implemented for the Rust BigQuery Storage Write client.

---

## 1. Architectural Goals & Challenges

The BigQuery Storage Write API is a high-throughput gRPC streaming ingest service. To achieve maximum throughput while minimizing bandwidth, connection handshakes, and resource utilization, the client must manage physical stream connections efficiently.

To optimize network payloads, BQ Storage Write benefits significantly from **stream stickiness**; consecutive requests sent on the same connection can omit table names, schemas, and other metadata, saving significant serialization and wire overhead.

### Key Challenges
* **Hot-Path Overhead:** Traditional multiplexing designs introduce a central router task or actor, requiring two channel hops (Client $\rightarrow$ Router $\rightarrow$ Connection) and a thread context switch per request. This introduces significant queue allocations, locks, and latency.
* **Reshuffling Churn:** If a stream connection dies under a simple modulo mapping (`hash(id) % N`), the divisor changes. This re-indexes 100% of writers to different connections, instantly destroying compression and schema caching locality across the entire client.
* **Lock Contention:** Storing stream handles in a shared `Mutex` or `RwLock` creates thread contention under high write concurrency, severely limiting multi-core scaling.
* **Stream Semantic Duality:** The client must seamlessly support both high-throughput, unordered ingestion (on the `default` stream) and strict, offset-based ordered ingestion (on `exclusive` streams like `Pending`, `Committed`, and `Buffered`).

---

## 2. Lock-Free Direct Routing Architecture (Default Stream)

To resolve the challenges for unordered ingestion, we replace any central coordinator actor with a **Read-Copy-Update (RCU)** shared table paired with **Direct-to-Channel client dispatch** using `arc_swap`.

```
                           [ ConnectionPool ]
                                  │
                          (ArcSwap snapshot)
                                  ▼
[ Writer 1 ] ──(load-based lookup / ~1ns)──> [ Stream Connection A ]
[ Writer 2 ] ──(load-based lookup / ~1ns)──> [ Stream Connection B ]
[ Writer 3 ] ──(load-based lookup / ~1ns)──> [ Stream Connection A ]
```

### Hot Path (Zero Locks, Zero Context Switches)
1. When `DefaultWriter::append(&self, rows)` is called, it dispatches the write request via its `Dispatcher`. The dispatcher loads the currently assigned `StreamEntry` from its atomic, lock-free local cache (`cached_stream.load_full()`). This operation takes less than **1 nanosecond** and executes without any mutexes or thread blockages.
2. The returned `Append` builder clones the lightweight connection sender and dispatches the request **directly** into the channel of the background connection `Runner` task.
3. There are no middleman tasks, no double-queuing, and zero mutex locks on the hot path.

---

## 3. Power of Two Choices (Po2C) Least Loaded Routing

To guarantee optimal workload balancing and prevent stream saturation, the `StreamPool` implements a dynamic **hybrid Least Loaded Stream selection** algorithm utilizing the **Power of Two Choices (Po2C)** model, rather than static hash-based mappings or full linear scans at scale.

```
                  [ StreamPool::get_least_loaded_stream() ]
                                      │
                         Is Pool Size <= 3 streams?
                                     / \
                                    /   \
                             (Yes) /     \ (No)
                                  /       \
         [ Scan Entire Pool ]                  [ Power of Two Choices (Po2C) ]
                  │                                            │
       Find global absolute min                         Select 2 random indices
                  │                                     (rand::thread_rng)
                  │                                            │
                  │                                     Compare load metrics
                  │                                            │
                  ▼                                            ▼
           [ Selected Stream ]                          [ Selected Stream ]
```

### Hybrid Selection Algorithm
When selecting a stream connection for a newly created writer or upon rebinding after a connection failure, the pool evaluates current pool capacity and delegates load-balancing:

1. **Deterministic Small-Pool Fallback (Size $\le 3$):** 
   If the pool has 3 or fewer streams, it scans all of them to find the absolute global minimum load. Since $N$ is tiny, this $O(N)$ lookup executes in nanoseconds with perfect balancing correctness.
   $$\text{Selected Stream} = \arg\min_{S_i} \text{Outstanding Requests}(S_i)$$

2. **Power of Two Choices (Po2C) (Size $> 3$):**
   If the pool is larger than 3 streams, doing a full scan introduces unnecessary cache line transfers and CPU cycles. Instead, the pool chooses **two distinct stream indices** pseudorandomly, compares their outstanding loads, and picks the lower of the two:
   $$\text{Selected Stream} = \arg\min_{S \in \{S_a, S_b\}} \text{Outstanding Requests}(S)$$
   
   This $O(1)$ strategy mathematically breaks load polarization (avoiding the thundering herd effect that occurs when multiple concurrent reconnects grab the exact same global minimum).

3. **High-Performance Randomness using `rand`:**
   To pick indices efficiently and leverage established project conventions, the pool utilizes the standard `rand` crate's high-performance generator:
   ```rust
   let mut rng = rand::rng();
   let idx1 = rng.gen_range(0..len);
   let mut idx2 = rng.gen_range(0..len);
   ```
   This is extremely fast, standard, and perfectly matches the random generation patterns established across handwritten crates in the workspace.

### Implementation Snippet

```rust
/// Selects a stream using the Power of Two Choices (Po2C) algorithm.
pub(crate) fn select_stream_po2c(
    streams: &[StreamEntry],
    mut get_indices: impl FnMut(usize) -> (usize, usize),
) -> Option<StreamEntry> {
    if streams.is_empty() {
        return None;
    }
    if streams.len() <= 3 {
        // Fallback to absolute minimum for very small pools.
        return streams
            .iter()
            .min_by_key(|entry| entry.outstanding_requests.load(Ordering::Relaxed))
            .cloned();
    }

    let (idx1, idx2) = get_indices(streams.len());
    let s1 = &streams[idx1 % streams.len()];
    let s2 = &streams[idx2 % streams.len()];

    let load1 = s1.outstanding_requests.load(Ordering::Relaxed);
    let load2 = s2.outstanding_requests.load(Ordering::Relaxed);

    if load1 < load2 {
        Some(s1.clone())
    } else {
        Some(s2.clone())
    }
}
```

This guarantees:
- **Scalable $O(1)$ Load Distribution:** Bypasses full pool searches as the pool grows, scaling linearly with thread count.
- **Thundering Herd Suppression:** Concurrently failing writers random-sample different pairs of connections, naturally fanning traffic out over the pool.
- **Microsecond Freshness:** Avoids periodic background sorting, querying real-time load atomic counters at the exact microsecond of write dispatch.

---

## 4. Zero-Contention Telemetry & Flow Control

To safely support high-concurrency workloads, the system tracks and manages two key metrics for each physical stream connection:
1. **Outstanding Requests**
2. **Outstanding Bytes**

### Zero-Contention Atomic Tracking via RAII
To prevent metrics drift under task cancellation (e.g., if an `Append::send()` future is canceled at an `.await` boundary by a caller-level timeout), we avoid manual decrements. Instead, we utilize an RAII-based safety guard (`LoadGuard`) that automatically decrements the respective counters upon dropping, even if the future is terminated mid-execution.

- **On Write Dispatch:** The `Append::send()` method atomically increments the connection's `outstanding_requests` and `outstanding_bytes`, then binds them to a `LoadGuard`.
- **On Response / Error / Cancellation:** The `LoadGuard` is dropped when the future returns or is cancelled, ensuring that counters are accurately decremented without any manual state cleanup.

```rust
pub(crate) struct LoadGuard {
    outstanding_requests: Arc<std::sync::atomic::AtomicU64>,
    outstanding_bytes: Arc<std::sync::atomic::AtomicU64>,
    bytes: u64,
}

impl Drop for LoadGuard {
    fn drop(&mut self) {
        self.outstanding_requests
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        self.outstanding_bytes
            .fetch_sub(self.bytes, std::sync::atomic::Ordering::Relaxed);
    }
}
```

### Flow Control & Scale-Up
- **Transport Metrics:** Before dispatching, the pool compares the target stream's `outstanding_bytes` and `outstanding_requests` against configured high watermarks (`MAX_REQUESTS_THRESHOLD`, `MAX_BYTES_THRESHOLD`) to trigger dynamic scaling.
- **Leak-Free Out-of-Band Watchdog Task:** Spawns a background worker (`watchdog::spawn_watchdog`) that runs completely out-of-band. To prevent memory and connection leaks, the task holds a `Weak<StreamPool>` pointer. If the parent `StreamPool` and all writers are dropped, the task automatically terminates. Every tick, it inspects these metrics to prune closed connections, trigger rebalancing, or dynamically scale up/down the number of physical stream connections inside the `ArcSwap` table without interrupting ongoing writes.

---

## 5. Reactive Fault Tolerance & Eviction

The client detects and recovers from connection failures reactively at the call site, avoiding central polling loops on the write path.

```
[ BQ Write Connection Dropped ]
             │
             ▼
[ Append::send() fails or channel closed ]
             │
             ├──> 1. pool.evict_and_replace(stream_id) ──> Atomically removes dead stream,
             │                                              provisions a replacement runner (cached).
             │
             └──> 2. cached_stream.store(new_stream)   ──> Updates DefaultWriter's local cache
                                                            so subsequent writes route smoothly.
```

1. **Detection:** When a gRPC stream breaks, calling `sender.send()` or waiting on the `oneshot` channel fails with `SendError` or `RecvError`.
2. **Atomic Eviction:** The first failing writer atomically evicts the failed stream and spawns a replacement via `pool.evict_and_replace()`. 
3. **RCU Closure Caching:** To prevent spawning multiple redundant `Runner` tasks if the RCU transaction retries under high contention, the newly created replacement stream entry is cached locally within the closure context.
4. **Local Cache Invalidation:** The writer updates its local `cached_stream` with a new, healthy connection from the pool. Healthy writers mapped to other streams are completely unaffected, experiencing zero thundering herd or pointer lookups.

---

## 6. Structural Design & Lifetimes

By housing the writer's cache inside a thread-safe `Dispatcher`, we ensure clean, type-safe ownership sharing and an elegant `'static` lifetime structure:

```rust
pub struct DefaultWriter {
    pub(crate) inner: Arc<Dispatcher>,
    pub(crate) write_stream: String,
    pub(crate) schema: ArrowSchema,
}

pub(crate) struct Dispatcher {
    pub(crate) pool: ConnectionPool,
    pub(crate) cached_stream: ArcSwap<StreamEntry>,
    dispatcher: LoadGuardedDispatcher,
}

pub(crate) enum ConnectionPool {
    Multiplexed(Arc<StreamPool>),
    Exclusive(ExclusivePool),
}
```

* **Zero-Lock Hot Path:** Cloning `Dispatcher` clones a cheap `Arc` pointer. `Append` request builders hold a `'static` copy of this dispatcher, meaning they can be freely spawned, moved across thread pools, or retried asynchronously.
* **Agnostic Pooling:** The `Dispatcher` is completely strategy-agnostic. It routes writes and handles eviction symmetrically, regardless of whether the backend `ConnectionPool` is multiplexed or exclusive.
* **Double-Eviction Prevention:** Even in `ExclusivePool`, the `u64` connection ID prevents double-eviction. If multiple concurrent `.append()` futures fail simultaneously, only the first future actually triggers a reconnect. Racing futures recognize that `current.id != failed_id` and safely treat the replacement as a no-op, avoiding connection churn.

---

## 7. Ordered Exclusive Streams: Architectural Duality

For stateful streams (`Pending`, `Committed`, `Buffered`), writes must be strictly sequenced by an explicit `offset`. A connection pool strategy (multiplexed or exclusive) is inappropriate here because gRPC streams enforce strict sequencing: if the server receives request $K+1$ before $K$ due to network or scheduling reordering, the stream terminates.

To achieve sequential guarantees and clean retries, we decouple ordered streams from the `ConnectionPool` abstraction entirely, introducing a **Stateful Connection Manager / Coordinator** model.

### Stateful Event-Loop Coordinator

Instead of dispatching writes directly to a shared channel from multiple caller threads, we serialize access through a single-threaded background coordinator loop.

```
[ Caller Thread 1 ] ──┐
[ Caller Thread 2 ] ──┼─(mpsc::send)─> [ Event Loop (Coordinator) ]
[ Caller Thread 3 ] ──┘                │  ├─ VecDeque<InFlightWrite>
                                       │  └─ Retry / Classification
                                       ▼
                             [ Active gRPC Stream ]
```

1. **Ordering Serialization:** Callers send their requests to the coordinator via an `mpsc` channel. The coordinator processes them sequentially, assigns offsets, and forwards them to the active stream.
2. **In-Flight Queue Ownership:** The coordinator retains ownership of sent-but-unacknowledged requests inside a local `VecDeque<InFlightWrite>`:
   ```rust
   struct InFlightWrite {
       req: AppendRowsRequest,
       offset: i64,
       resp_tx: oneshot::Sender<AppendResult<AppendRowsResponse>>,
   }
   ```
3. **Sequence Rewind & Replay:**
   If the gRPC connection breaks, the coordinator catches the transient failure (using shared classification utilities), establishes a new stream connection, and replays all unacknowledged requests in the `VecDeque` sequentially starting from the failed offset.
4. **Caller Transparency:**
   The caller's `oneshot` response channels remain open and unaffected during reconnects. The failover and sequence replay occur entirely behind the scenes, and the caller's future is resolved only when the replayed requests are successfully acknowledged by the new stream.

---

## 8. Application-Level Flow Control

While transport-level load metrics (`outstanding_requests`, `outstanding_bytes`) are excellent for load balancing and autoscale decisions inside the `StreamPool`, they are not designed for user-facing backpressure.

To prevent client-side Out-Of-Memory (OOM) situations under extreme write speeds, the client implements **Application-Level Flow Control** using a `tokio::sync::Semaphore` (matching the pattern used in `google-cloud-pubsub`):

1. **Token Acquisition:** Before serializing or copying Arrow record batches, the client acquires permits from a bounded semaphore based on row or byte size.
2. **Resource Bound:** This suspends the calling thread/future *before* heavy memory allocation occurs, protecting system stability.
3. **Token Release:** The permits are packed into the write request context and automatically released when the write is acknowledged or failed.

---

## 9. Architectural Comparison Matrix

| Architectural Dimension | Default Stream (`ArcSwap` Model) | Exclusive Ordered Streams (Coordinator Model) |
| :--- | :--- | :--- |
| **Primary Use Case** | High-throughput, out-of-order, multiplexed ingestion. | Strict, offset-based, sequential transactional pipelines. |
| **Hot-Path Latency** | **Sub-nanosecond** (Lock-free atomic pointer load). | **Context Switch + Channel Latency** (Serialized execution). |
| **Channel Hops** | **1 channel hop** (Client $\rightarrow$ Connection). | **2 channel hops** (Client $\rightarrow$ Coordinator $\rightarrow$ Connection). |
| **Concurrency Scaling**| **Linear.** Multi-core concurrent dispatch. | **Serialized.** Sequenced through a single event-loop. |
| **Retry & Failover** | Reactive atomic eviction & swap at the call site. | Stateful sequence rewind & replay of unacknowledged queue. |
| **In-Flight Tracking** | Minimal. Handled via individual gRPC streams. | Full. Tracked via a coordinator-owned `VecDeque`. |
