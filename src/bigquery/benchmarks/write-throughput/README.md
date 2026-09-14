# BigQuery Write Throughput Benchmark

A throughput benchmark for the BigQuery Storage Write API in the
`google-cloud-bigquery` Rust client library.

This tool measures the streaming ingestion performance of the default stream in
the BigQuery Storage Write API using Arrow data. It reports operation rates in
batches per second and megabytes per second.

## Usage

```bash
cargo run --release -p bigquery-write-throughput -- [OPTIONS]
```

To view all available options, run:

```bash
cargo run -p bigquery-write-throughput -- --help
```

## Options

- `--project <PROJECT>`: Google Cloud project ID (can also be set via
  `GOOGLE_CLOUD_PROJECT`).
- `--duration <DURATION>`: Benchmark runtime duration (e.g. `1m`, `5m`, `300s`).
  Default: `1m`.
- `--report-interval <REPORT_INTERVAL>`: Frequency of progress metrics reporting
  (e.g. `5s`, `10s`). Default: `5s`.
- `--row-size <ROW_SIZE>`: Size of each row payload in bytes. Default: `1024`.
- `--rows-per-batch <ROWS_PER_BATCH>`: Number of rows per serialized Arrow
  RecordBatch. Default: `1000`.
- `--num-tables <NUM_TABLES>`: Number of tables created/written to in the
  dataset. Default: `1`.
- `--num-writers <NUM_WRITERS>`: Number of concurrent writers. Default: `1`.
- `--grpc-channels <GRPC_CHANNELS>`: Number of gRPC subchannels configured on
  the client. Default: `1`.
- `--dataset-id <DATASET_ID>`: Target dataset ID. If not specified, a temporary
  dataset (`rust_bq_bench_dataset_<random>`) is created and automatically
  cleaned up upon completion.
- `--multiplex`: Enable stream multiplexing across writers. Default: `false`.
- `--multiplex-pool-size <MULTIPLEX_POOL_SIZE>`: Maximum number of streams in
  the client's multiplexed stream pool. Default: `4`.
- `--max-outstanding-requests <MAX_OUTSTANDING_REQUESTS>`: Maximum outstanding
  requests per multiplexed stream before load balancing kicks in. Default:
  `1000`.
- `--max-outstanding-bytes <MAX_OUTSTANDING_BYTES>`: Maximum outstanding bytes
  per multiplexed stream before load balancing kicks in. Default: none.
- `--attempt-timeout <ATTEMPT_TIMEOUT>`: Maximum duration for an individual
  write attempt before timing out and retrying (e.g. `20ms`, `500ms`, `5s`).
  Default: none (unbounded).

## Output Format

The benchmark outputs progress data in CSV format:

- `timestamp`: Unix epoch timestamp in milliseconds.
- `elapsed(s)`: Elapsed time for the reported interval in seconds.
- `op`: Operation type (`Send` for dispatched batches, `Recv` for acknowledged
  batches).
- `iteration`: Current report iteration number.
- `count`: Number of batches processed in this interval.
- `batches/s`: Batches processed per second.
- `bytes`: Total bytes processed in this interval.
- `MB/s`: Throughput in megabytes per second.
- `errors`: Number of errors encountered in this interval.
- `errors/s`: Number of errors per second.

## Examples

### Basic Benchmark

```bash
cargo run --release -p bigquery-write-throughput -- \
    --project ${GOOGLE_CLOUD_PROJECT} \
    --duration 1m \
    --report-interval 10s \
    --num-writers 2 \
    --grpc-channels 1
```

### Multiplexed Writers

```bash
cargo run --release -p bigquery-write-throughput -- \
    --project ${GOOGLE_CLOUD_PROJECT} \
    --duration 1m \
    --report-interval 10s \
    --num-writers 8 \
    --multiplex \
    --multiplex-pool-size 4
```

## Automated Sweep & Visual Dashboard

The `sweep.py` script runs the benchmark $N$ times with randomized configurations
(channels, pool sizes, writer counts, batch sizes, etc.), logs time-series data, and
automatically generates an interactive HTML performance dashboard with charts.

```bash
# Run 10 random 5-minute benchmarks and generate visual dashboard
python3 src/bigquery/benchmarks/write-throughput/sweep.py \
    --project ${GOOGLE_CLOUD_PROJECT} \
    -n 10 \
    --duration 5m \
    --report-interval 10s

# Generate an interactive graph for any existing benchmark log file
python3 src/bigquery/benchmarks/write-throughput/sweep.py --plot-log path/to/bm-run.txt
```

The script produces:
- `report.html`: Standalone interactive dashboard with leaderboard, time-series charts, and metrics table.
- `summary.csv` and `summary.json`: Tabular benchmark results for all runs.
- `logs/run_*.txt`: Raw console and CSV outputs for each individual run.

