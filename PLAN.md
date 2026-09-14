# BigQuery Storage Write API: Default Stream Retries & Deadlines Plan

## Objective
Implement robust, idiomatic retries and deadlines for the BigQuery Storage Write API default stream (`DefaultWriter` and `Dispatcher`) in `google-cloud-bigquery`.

This work is scoped strictly to the default stream. Exclusive streams (`pending`, `committed`, `buffered`) requiring ordered replay across stream failures are explicitly out of scope.

---

## The 8-PR Implementation Plan

### PR 1: `AppendError::Rpc::source` as `Arc<Error>` & Runner Fan-Out
- **Objective**: Make `AppendError` cloneable for RPC errors and prevent the thundering-herd stream recreation stampede when stream creation fails.
- **Scope**:
  - In `src/bigquery/src/write/error.rs`:
    - Update `AppendError::Rpc` to store `source: Arc<Error>`.
    - Implement `From<Error>` (wrapping in `Arc::new`) and `From<Arc<Error>>`.
  - In `src/bigquery/src/write/runner.rs`:
    - In `run_stream_task`, when `Stream::new` returns `Err(e)`, wrap in `Arc::new(e)`.
    - Drain both `resp_txs` and pending `req_rx`, broadcasting `Err(AppendError::from(Arc::clone(&shared_e)))` to all waiting callers so none drop with ambiguous `UnexpectedEndOfStream`.
- **Tests**:
  - Update any existing tests constructing `AppendError::Rpc` to use `Arc`.
  - Activate and verify `stream_connect_error_fans_out_rpc_error_to_queued_writes`.

---

### PR 2: Retry Policy & Transient Classification (`src/bigquery/src/write/retry_policy.rs`)
- **Objective**: Define GAX-compliant default retry policy and error classifications.
- **Scope**:
  - Create `src/bigquery/src/write/retry_policy.rs`.
  - Implement `RetryableErrors` for `google_cloud_gax::retry_policy::RetryPolicy`:
    - Classify transient RPC status codes: `Aborted`, `Unavailable`, `ResourceExhausted`, `Internal`, `DeadlineExceeded`.
    - Classify HTTP status codes: `429`, `500`, `502`, `503`, `504`.
    - Classify transport & connect errors: `source.is_transport()`, `source.is_io()`, `source.is_connect()`.
    - Classify all other codes, serialization, and deserialization errors as `Permanent`.
  - Provide `default_retry_policy()` (60s time limit) and `default_backoff_policy()` (100ms initial delay, 30s max, 2.0 multiplier).
- **Tests**:
  - Unit tests in `retry_policy.rs` verifying each status code, transport error, and permanent error against the policy.

---

### PR 3: Dispatcher Error Classification (0-Backoff vs. Backoff, Same Stream vs. Evict)
- **Objective**: Refactor `Dispatcher::send` error handling before introducing the retry loop.
- **Scope**:
  - In `src/bigquery/src/write/dispatcher.rs`:
    - Differentiate `stream.send()` errors:
      - Transport drops (`UnexpectedEndOfStream`, `source.is_transport()`, `source.is_io()`, `Code::Aborted` server restart) $\rightarrow$ Evict stream. Categorize as candidate for immediate retry.
      - Connect errors (`source.is_connect()`, etc.) $\rightarrow$ Evict stream. Categorize as backoff required.
    - Differentiate `to_result(resp)` errors:
      - `RowErrors` $\rightarrow$ Permanent, keep stream.
      - `Response::Error(status)` $\rightarrow$ Keep stream! Evaluate against retry policy (e.g. `ResourceExhausted`), categorize as backoff required on the *same* stream.
- **Tests Activated**:
  - `row_error_keeps_stream_open_and_does_not_retry`
  - `permanent_rpc_error_fails_immediately`

---

### PR 4: Dispatcher Retry Loop
- **Objective**: Introduce the retry loop in `Dispatcher::send` without deadlines.
- **Scope**:
  - In `src/bigquery/src/write/dispatcher.rs`:
    - `Dispatcher` takes generic `Arc<dyn RetryPolicy>` and `Arc<dyn BackoffPolicy>` (calling code / `Dispatcher::new` hardcodes internal defaults for now).
    - Loop around the classified send attempts:
      - Track `RetryState` and `consecutive_drops`.
      - Fast path: immediate retry (0 backoff delay) on 1st transport disconnect.
      - Backoff path: apply `backoff_policy.on_failure(&state)` on in-stream pushback (`ResourceExhausted`), connect failure, or 2nd+ consecutive disconnect.
      - Stop on permanent errors or policy exhaustion.
- **Tests Activated**:
  - `unexpected_end_of_stream_retries_immediately_on_new_stream`
  - `consecutive_stream_disconnects_apply_backoff`
  - `resource_exhausted_retries_on_same_stream_with_backoff`
  - `transport_h2_reset_evicts_and_retries_immediately`
  - `server_restart_aborted_evicts_and_retries_immediately`
  - `stream_connect_transient_error_retries_with_backoff`
  - `library_retries_stream_drops_even_with_strict_customer_policy`
  - `customer_retry_policy_can_disable_resource_exhausted`

---

### PR 5: Add Attempt Deadline to `stream.rs` & Plumb via `runner.rs`
- **Objective**: Introduce per-attempt timeout into the low-level stream layer.
- **Scope**:
  - In `src/bigquery/src/write/stream.rs`:
    - Support per-attempt timeout on write requests or timeout wrapper.
  - In `src/bigquery/src/write/runner.rs`:
    - Plumb attempt timeout through `WriteRequest` or runner configuration.
  - In `src/bigquery/src/write/dispatcher.rs`:
    - Plumb attempt timeout into `stream.send()`.

---

### PR 6: Implement Deadlines in Dispatcher Retry Loop
- **Objective**: Enforce both per-attempt timeouts and overall operation deadlines in `Dispatcher::send`.
- **Scope**:
  - In `src/bigquery/src/write/dispatcher.rs`:
    - If an attempt times out: treat as hung stream, evict stream from pool, record attempt in `RetryState`, and retry.
    - Evaluate `retry_policy.remaining_time(&state)` before attempts and backoff sleeps. If budget exhausted, return `AppendError::Rpc(Error::exhausted(...))`.
- **Tests Activated**:
  - `attempt_timeout_evicts_hung_stream_and_retries`
  - `overall_operation_deadline_exhausts_and_stops_retries`

---

### PR 7: Internal Plumbing of Retry & Backoff Policies
- **Objective**: Plumb `retry_policy` and `backoff_policy` from client initialization down through `StreamPool` and `Dispatcher` with hardcoded defaults.
- **Scope**:
  - In `src/bigquery/src/write/client.rs` / `write/pool.rs` / `write/dispatcher.rs`:
    - Pass configured/default policies down to `Dispatcher` instead of hardcoding within `Dispatcher::new`.
    - Ensure writers (`ArrowWriterBuilder`, `ProtoWriterBuilder`, `DefaultWriter`) pass policies cleanly.

---

### PR 8: Public API Exposure on `ClientBuilder`
- **Objective**: Expose `.with_retry_policy(...)` and `.with_backoff_policy(...)` on `ClientBuilder` for customer configuration.
- **Scope**:
  - In `src/bigquery/src/write/client_builder.rs`:
    - Expose `with_retry_policy(P: RetryPolicy + 'static)` and `with_backoff_policy(B: BackoffPolicy + 'static)`.
  - Add doc comments, examples, and verify crate doc tests.
