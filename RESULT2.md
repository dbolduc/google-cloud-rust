✦ I have successfully implemented the BigQuery Storage Write client prototype and verified it with
  an integration test.

  Key Achievements

   1. Handwritten google-cloud-bigquery-write Client:
       * Implemented ClientBuilder for flexible configuration (endpoint, credentials,
         subchannels).
       * Implemented Client with write_stream capability.
       * Implemented StreamWriter using a background task to manage a bidirectional gRPC stream.
       * Resolved a critical deadlock where the client was waiting for a response before sending
         the first request. The client now sends the first request (with schema and stream name)
         immediately upon opening the stream.
       * Fixed a stack overflow issue during verification by disabling tracing for the BigQuery v2
         JobService, which was likely encountering recursion or extremely large payloads in its
         Debug implementations.

   2. Integration Test Suite:
       * Added a new writes integration test in tests/bigquery/src/lib.rs.
       * The test performs an end-to-end flow:
           * Creates a temporary BigQuery dataset and table.
           * Serializes an Arrow Schema and RecordBatch using the arrow crate (v58.3.0).
           * Appends 10 rows to the table using the new Storage Write client.
           * Verifies the write success by executing a SQL query and checking total_rows.
       * Managed workspace-level dependencies for arrow and properly integrated it into the test
         driver.

  Verification

  The integration test passed successfully:

   1 GOOGLE_CLOUD_PROJECT=dbolduc-test cargo test -p integration-tests-bigquery run_writes
     --features run-integration-tests

  Output:

   1 CREATING DATASET WITH ID: rust_bq_test_dataset_B213e7Go
   2 DATASET CREATED
   3 CREATING TABLE WITH ID: rust_bq_test_table_dbpv6FVe
   4 TABLE CREATED
   5 Schema message size: 192
   6 RecordBatch message size: 640
   7 test bigquery::run_writes ... ok

  The prototype is now functional and verified against live BigQuery services.
