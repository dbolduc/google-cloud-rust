Hey, I need you to write a benchmark of the BigQuery Write client for me.

## Prior Art

Note that there is prior art to writing a benchmark in:
- src/pubsub/benchmarks/throughput/...
- src/storage/benchmarks/...

There is also an integration test using the `Write` client in:
- tests/bigquery/src/writes.rs
- tests/bigquery/src/writes/arrow.rs

Please read all of those files before starting.

## Requirements

### Configuration

Please write a benchmark that has variables for:

- test duration
- size of each row
- rows per batch
- number of tables
- number of writers (>= number of tables) number of gRPC channels for the client

### Design

#### Setup

- Read in the configuration using `clap`
- Pick a table schema (only need one).
- Create the relevant dataset and tables.
- Create a `Write` client.
- Create N writers for the M tables. (N >= M)
- Launch a task per writer

#### Event loop

- In a loop in each task...
  - Generate some sort of test data
  - Send a write to the server 
  - We should keep sending writes to the server without awaiting any previous writes... I think... I am pretty sure?
  - Record the result of the write. Probaby with an atomic counter. 
  - Log any errors. For now, you can stop the benchmark if one happens.

#### Finish

- Timeout the tasks after `test_duration`
- Join the tasks, and report the results.

### Results

The benchmark should report all of the initial configuration as well as:
- the total throughput across all tables
- all errors should be logged
- the error rate / error count for write operations

## Code

The benchmark code will live under:
- src/bigquery-write/benchmarks/throughput/...

It depends on the client in:
- src/bigquery-write/src/client.rs

You can run it with:

```shell
GOOGLE_CLOUD_PROJECT=dbolduc-test \
  cargo run \
  --release \
  -p bigquery-write-throughput
```
