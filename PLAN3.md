# BigQuery Write Client Refactor Plan

This document outlines the plan to refactor the BigQuery Write client to align with the proposed internals design. The architecture follows a task-based actor model using composable layers.

## Core Architecture

The implementation will be built around a series of tasks communicating via asynchronous channels (`tokio::sync::mpsc`).

### 1. Internal Message Types (`pool.rs`)

We will define the unit of work and the command interface for stream tasks:

```rust
pub(super) struct AppendRequest {
    pub request: AppendRowsRequest,
    pub resp_tx: oneshot::Sender<Result<AppendRowsResponse>>,
}

pub(super) enum StreamCommand {
    Append(AppendRequest),
    // Future: Finalize(oneshot::Sender<Result<()>>),
}

#[derive(Clone)]
pub(super) struct StreamHandle {
    pub tx: mpsc::Sender<StreamCommand>,
}
```

### 2. Stream Task (`runner.rs`)

The `StreamWriterRunner` will be refactored into a generic `StreamTask`.

- **Responsibility**: Owns and manages a single bidirectional gRPC stream (`AppendRows`).
- **Input**: Receives `StreamCommand` from an `mpsc::Receiver`.
- **Logic**:
    - `tokio::select!` over incoming commands and gRPC responses.
    - Maintains a `VecDeque<oneshot::Sender>` (FIFO) to match responses to requests.
    - Forwards `AppendRowsRequest` to the gRPC stream.
    - On stream failure, it terminates and fails all pending `oneshot` senders.

### 3. Connection Pool (`pool.rs`)

The `ConnectionPool` manages regional resources.

- **Responsibility**: Routing and lifecycle management of `StreamTask`s.
- **State**: Holds `StreamHandle`s. Initially, it will lazily initialize and cache a single handle for the `_default` stream.
- **Future**: Will implement auto-scaling (spinning up new tasks) and multiplexing routing logic.

### 4. Stream Writers (`stream_writer.rs`)

`ArrowStreamWriter` and `ProtoStreamWriter` will be refactored into thin, format-specific wrappers.

- **State**: Holds a `StreamHandle`.
- **Logic**:
    - `append()` constructs the `AppendRowsRequest` (setting `write_stream`, `rows`, and `writer_schema`).
    - Sends a `StreamCommand::Append` to the `StreamHandle`.
    - Awaits the response via a `oneshot` channel.

### 5. Client Integration (`client.rs`)

The `Client` will maintain a `HashMap<String, Arc<ConnectionPool>>` to scope connections by region.

## Execution Steps

1.  **Define Types**: Update `pool.rs` with `AppendRequest`, `StreamCommand`, and `StreamHandle`.
2.  **Generic Stream Task**: Refactor `runner.rs` to remove specific `stream_name`/`schema` dependencies and handle `StreamCommand`.
3.  **Regional Pool**: Update `pool.rs` to handle lazy initialization of the default stream task.
4.  **Writer Refactor**: Update `stream_writer.rs` to use the `StreamHandle`.
5.  **Integration**: Link the `Client` to the `ConnectionPool`.

## Testing and Validation

Verification will be performed using the existing integration test suite:

```shell
GOOGLE_CLOUD_PROJECT=dbolduc-test cargo test -p integration-tests-bigquery --features run-integration-tests writes
```

The refactor is considered successful when the existing "writes" integration test passes, confirming structural integrity without behavioral regression.
