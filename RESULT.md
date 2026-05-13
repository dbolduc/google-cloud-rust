 I have completed the implementation of the BigQuery Storage Write client prototype. 

  Summary of Changes

   1. ClientBuilder: Created a new builder in src/bigquery-write/src/client_builder.rs to allow
      configurable client initialization (endpoint, credentials, subchannels).
   2. Client: Implemented the main client in src/bigquery-write/src/client.rs. It provides a
      write_stream method to create writers for specific BigQuery streams.
   3. StreamWriter: Implemented the core writing logic in src/bigquery-write/src/stream_writer.rs.
       * It uses a background task to manage a bidirectional gRPC stream.
       * The stream is lazily initialized upon the first append call.
       * It uses a channel-based architecture to handle multiple concurrent appends, correlating
         responses back to callers via oneshot channels.
       * It handles the required state for the first message (e.g., sending the schema and stream
         name).
   4. Crate Integration:
       * Updated src/bigquery-write/src/lib.rs to export the new modules.
       * Modified src/bigquery-write/src/transport.rs to make the underlying gRPC transport
         accessible to the crate.
       * Corrected dependencies in src/bigquery-write/Cargo.toml, moving google-cloud-auth to core
         dependencies and enabling the necessary rustls crypto provider.

  Verification Results

  All unit tests and documentation examples in the google-cloud-bigquery-write crate passed
  successfully:

   1 cargo test -p google-cloud-bigquery-write

  Results:
   * 6 unit tests passed.
   * 5 documentation tests passed.
   * Zero warnings or compilation errors.

  The implementation follows the idiomatic patterns found in other client libraries (like Pub/Sub)
  while adhering to the BigQuery Storage Write API's requirements for stream management and schema
  handling.
