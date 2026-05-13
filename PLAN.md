Implementation Plan - BigQuery Storage Write Client Prototype

  Objective
  Prototype a BigQuery Storage Write client in Rust that abstracts the
  bidirectional AppendRows stream, following the patterns used in other Rust
  clients (Pub/Sub, Storage).

  Key Files & Context
   - src/bigquery-write/src/client.rs: Main client.
   - src/bigquery-write/src/client_builder.rs: Client builder.
   - src/bigquery-write/src/stream_writer.rs: StreamWriter to handle AppendRows.
   - src/bigquery-write/src/transport.rs: Underlying transport logic (already
     partially implemented).

  Proposed API Surface

  Client

    1 pub struct Client {
    2     inner: Arc<Transport>,
    3 }
    4
    5 impl Client {
    6     pub fn builder() -> ClientBuilder { ... }
    7     
    8     /// Create a StreamWriter for a specific stream.
    9     /// The schema must be provided and will be sent in the first
      AppendRows request.
   10     pub fn write_stream(&self, stream_name: String, schema: ArrowSchema) ->
      StreamWriter { ... }
   11 }

  ClientBuilder

   1 pub struct ClientBuilder {
   2     config: ClientConfig,
   3 }
   4
   5 impl ClientBuilder {
   6     pub async fn build(self) -> Result<Client> { ... }
   7     pub fn with_endpoint(mut self, endpoint: String) -> Self { ... }
   8     // ... other standard setters (with_credentials, etc.)
   9 }

  StreamWriter

   1 pub struct StreamWriter {
   2     inner: Arc<StreamWriterImpl>,
   3 }
   4
   5 impl StreamWriter {
   6     /// Append Arrow record batches to the stream.
   7     pub async fn append(&self, rows: ArrowRecordBatch) ->
     Result<AppendRowsResponse> { ... }
   8 }

  Implementation Details

   1. Define ClientBuilder:
       * Implement new, build, and standard setters (endpoint, credentials).
       * Pattern matched from src/pubsub/src/subscriber/client_builder.rs.

   2. Define Client:
       * Implement builder, new, and write_stream.
       * Pattern matched from src/pubsub/src/subscriber/client.rs.

   3. Define StreamWriter:
       * StreamWriter will lazily initialize the AppendRows stream on the first
         append.
       * It will use a background task to manage the gRPC stream.
       * It will use a tokio::sync::mpsc channel to send requests to the
         background task.
       * It will use a queue of oneshot senders to correlate responses back to
         the append calls, leveraging the API's guaranteed ordering.

   4. Verification & Testing:
       * Add unit tests in each file.
       * Use bigquery_write_grpc_mock for testing as seen in transport.rs.
       * Ensure cargo test -p google-cloud-bigquery-write passes.
