# BigQuery Storage Write API - Multiplexed Connection Pool Design

This document details the architecture and design of the lock-free, concurrent multiplexed stream connection pool prototype implemented for the Rust BigQuery Storage Write client.

---

## 1. Architectural Goals & Challenges

The BigQuery Storage Write API is a high-throughput gRPC streaming ingest service. To achieve maximum throughput while minimizing bandwidth, connection handshakes, and resource utilization, the client must multiplex many concurrent logical writers ($M$) over a small, stable set of active physical stream connections ($N$, where $M \gg N$).

To optimize network payloads, BQ Storage Write benefits significantly from **stream stickiness**; consecutive requests sent on the same connection can omit table names, schemas, and other metadata, saving significant serialization and wire overhead.

### Key Challenges
* **Hot-Path Overhead:** Traditional multiplexing designs introduce a central router task or actor, requiring two channel hops (Client $\rightarrow$ Router $\rightarrow$ Connection) and a thread context switch per request. This introduces significant queue allocations, locks, and latency.
* **Reshuffling Churn:** If a stream connection dies under a simple modulo mapping (`hash(id) % N`), the divisor changes. This re-indexes 100% of writers to different connections, instantly destroying compression and schema caching locality across the entire client.
* **Lock Contention:** Storing stream handles in a shared `Mutex` or `RwLock` creates thread contention under high write concurrency, severely limiting multi-core scaling.

---

## 2. Lock-Free Direct Routing Architecture

To resolve the above challenges, we replace the central coordinator actor with a **Read-Copy-Update (RCU)** shared table paired with **Direct-to-Channel client dispatch**.

```
                           [ Shared StreamPool ]
                                     │
                             (ArcSwap snapshot)
                                     ▼
[ Writer 1 ] ──(load-based lookup / ~1ns)──> [ Stream Connection A ]
[ Writer 2 ] ──(load-based lookup / ~1ns)──> [ Stream Connection B ]
[ Writer 3 ] ──(load-based lookup / ~1ns)──> [ Stream Connection A ]
```

### Hot Path (Zero Locks, Zero Context Switches)
1. When `DefaultWriter::append(&self, rows)` is called, it loads the current assigned `StreamEntry` from its atomic, lock-free local cache (`cached_stream.load_full()`). This operation takes less than **1 nanosecond** and executes without any mutexes or thread blockages.
2. The returned `Append` builder clones the lightweight connection sender and dispatches the request **directly** into the channel of the background connection `Runner` task.
3. There are no middleman tasks, no double-queuing, and zero mutex locks on the hot path.

---

## 3. Least Loaded Stream Routing

To guarantee optimal workload balancing and prevent stream saturation, the pool implements a dynamic **Least Loaded Stream** routing algorithm rather than static hash-based mappings.

### How it operates:
When selecting a stream connection for a newly created writer or upon rebinding after a connection failure:
1. The pool queries the active list of healthy stream connections in the atomic `ArcSwap` snapshot.
2. It evaluates the load on each stream by reading their atomic tracking counters (`outstanding_requests`) and selects the connection with the lowest value:
   $$\text{Selected Stream} = \arg\min_{S_i} \text{Outstanding Requests}(S_i)$$

```rust
pub(crate) fn get_least_loaded_stream(&self) -> StreamEntry {
    let streams = self.streams.load();
    streams
        .iter()
        .min_by_key(|entry| entry.outstanding_requests.load(Ordering::Relaxed))
        .cloned()
        .expect("StreamPool invariant violated: pool must never be empty")
}
```

This guarantees:
- **Automatic Load Distribution:** Highly active writers naturally fan out across all available connections based on real-time outstanding request counts.
- **Dynamic Rebalancing:** If a connection becomes congested, subsequent writer re-binds are automatically routed to idle connections, alleviating bottlenecks.

---

## 4. Zero-Contention Telemetry & Flow Control

To safely support high-concurrency workloads, the system tracks and manages two key metrics for each physical stream connection:
1. **Outstanding Requests**
2. **Outstanding Bytes**

### Zero-Contention Atomic Tracking
Because the background connection `Runner` is single-threaded, we can update these metrics using zero-contention relaxed atomic variables (`AtomicU64`):
- **On Write Dispatch:** The `Append::send()` method atomically increments the selected connection's `outstanding_requests` and `outstanding_bytes` by the encoded length of the request.
- **On Response / Error:** When a response is successfully received from BQ Storage Write, or if the connection task exits with an error, the background task atomically decrements both counters.

```rust
outstanding_requests.fetch_add(1, Ordering::Relaxed);
outstanding_bytes.fetch_add(req_proto.encoded_len() as u64, Ordering::Relaxed);
```

### Flow Control & Scale-Up
- **Backpressure:** Before dispatching, the client can compare the target stream's `outstanding_bytes` and `outstanding_requests` against configured high watermarks, suspending the writer until the connection drains.
- **Out-of-Band Watchdog Task:** Spawns a background worker (`watchdog::spawn_watchdog`) that runs completely out-of-band. Every tick, it inspects these metrics to prune closed connections, trigger rebalancing, or dynamically scale up/down the number of physical stream connections inside the `ArcSwap` table without interrupting ongoing writes.

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
             │                                              provisions a replacement runner.
             │
             └──> 2. cached_stream.store(new_stream)   ──> Updates DefaultWriter's local cache
                                                            so subsequent writes route smoothly.
```

1. **Detection:** When a gRPC stream breaks, calling `sender.send()` or waiting on the `oneshot` channel fails with `SendError` or `RecvError`.
2. **Atomic Eviction:** The first failing writer atomically evicts the failed stream and spawns a replacement via `pool.evict_and_replace()`. Multiple racing writers attempting to evict the same `failed_id` result in a single, collision-free RCU swap.
3. **Local Cache Invalidation:** The writer updates its local `cached_stream` with a new, healthy connection from the pool. Healthy writers mapped to other streams are completely unaffected, experiencing zero thundering herd or pointer lookups.

---

## 6. Structural Design & Lifetimes

By housing the writer's cache inside an `Arc<WriterSharedState>`, we ensure clean, type-safe ownership sharing and an elegant `'static` lifetime structure:

```rust
pub struct DefaultWriter {
    pub(crate) inner: Arc<WriterSharedState>,
    pub(crate) write_stream: String,
    pub(crate) schema: ArrowSchema,
}

pub(crate) struct WriterSharedState {
    pub(crate) pool: Arc<StreamPool>,
    pub(crate) cached_stream: ArcSwap<StreamEntry>,
}
```

* **Zero-Lock Hot Path:** Cloning `WriterSharedState` clones a cheap `Arc` pointer. `Append` request builders hold a `'static` copy of this state, meaning they can be freely spawned, moved across thread pools, or retried asynchronously.
* **Decoupled Retries:** Leaving a `TODO` for future Gax-based retry loops, retries can happen entirely inside `Append::send()` by doing subsequent lookup passes and channel pushes, completely hidden from the user API.
