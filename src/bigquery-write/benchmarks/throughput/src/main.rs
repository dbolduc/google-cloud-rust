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

use arrow::array::StringArray;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use google_cloud_bigquery_v2::client::{DatasetService, TableService};
use google_cloud_bigquery_v2::model::{
    Dataset, DatasetReference, Table, TableFieldSchema, TableReference, TableSchema,
};
use google_cloud_bigquery_write::client::Write;
use google_cloud_bigquery_write::model::{ArrowRecordBatch, ArrowSchema};
use rand::RngExt;
use rand::distr::Alphanumeric;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

mod args;

#[derive(Default)]
struct Stats {
    send_count: AtomicI64,
    send_bytes: AtomicI64,
    recv_count: AtomicI64,
    recv_bytes: AtomicI64,
    error_count: AtomicI64,
    stop_flag: AtomicBool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = crate::args::parse_args();
    if config.project.is_empty() {
        anyhow::bail!(
            "GOOGLE_CLOUD_PROJECT environment variable or --project argument must be set"
        );
    }

    println!(
        "# Running BigQuery Write throughput benchmark with config: {:?}",
        config
    );
    run_benchmark(config).await?;

    Ok(())
}

async fn run_benchmark(config: crate::args::Config) -> anyhow::Result<()> {
    let dataset_service = DatasetService::builder().build().await?;
    let table_service = TableService::builder().build().await?;

    let dataset_id = if config.dataset_id.is_empty() {
        let rand_suffix: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();
        format!("rust_bq_bench_dataset_{}", rand_suffix.to_lowercase())
    } else {
        config.dataset_id.clone()
    };

    let is_temp_dataset = config.dataset_id.is_empty();

    if is_temp_dataset {
        println!("# Creating temporary dataset: {}", dataset_id);
        dataset_service
            .insert_dataset()
            .set_project_id(&config.project)
            .set_dataset(
                Dataset::new()
                    .set_dataset_reference(DatasetReference::new().set_dataset_id(&dataset_id))
                    .set_labels([("bq_benchmark", "true")]),
            )
            .send()
            .await?;
    }

    let run_res = async {
        let mut table_ids = Vec::new();
        for t in 0..config.num_tables {
            let table_id = format!("table_{}", t);
            println!("# Creating table: {} in dataset: {}", table_id, dataset_id);

            let schema = TableSchema::new().set_fields([
                TableFieldSchema::new().set_name("payload").set_type("STRING"),
            ]);

            table_service
                .insert_table()
                .set_project_id(&config.project)
                .set_dataset_id(&dataset_id)
                .set_table(
                    Table::new()
                        .set_table_reference(
                            TableReference::new()
                                .set_project_id(&config.project)
                                .set_dataset_id(&dataset_id)
                                .set_table_id(&table_id),
                        )
                        .set_schema(schema),
                )
                .send()
                .await?;

            table_ids.push(table_id);
        }

        println!("# Creating BigQuery Write client...");
        let client = Write::builder()
            .with_grpc_subchannel_count(config.grpc_channels)
            .build()
            .await?;

        // Pre-serialize schema and batch
        let arrow_schema = Arc::new(Schema::new(vec![Field::new(
            "payload",
            DataType::Utf8,
            false,
        )]));
        let schema_buf = serialize_schema(&arrow_schema)?;
        let schema_len = schema_buf.len();

        let payload_str = "x".repeat(config.row_size);
        let payloads: Vec<&str> = std::iter::repeat_n(payload_str.as_str(), config.rows_per_batch)
            .collect();
        let payload_array = StringArray::from(payloads);
        let batch = RecordBatch::try_new(arrow_schema.clone(), vec![Arc::new(payload_array)])?;
        let batch_buf = serialize_batch(&batch, schema_len)?;
        let logical_bytes_per_batch = (config.row_size * config.rows_per_batch) as i64;

        println!(
            "# Setup complete. Row size: {} bytes, Rows per batch: {}, Logical batch size: {} bytes, Serialized batch size: {} bytes",
            config.row_size,
            config.rows_per_batch,
            logical_bytes_per_batch,
            batch_buf.len()
        );

        let mut writers = Vec::new();
        for w in 0..config.num_writers {
            let table_id = &table_ids[w % config.num_tables];
            let table_path = format!(
                "projects/{}/datasets/{}/tables/{}",
                config.project, dataset_id, table_id
            );

            let schema = ArrowSchema::new().set_serialized_schema(schema_buf.clone());
            let writer = client.arrow(schema).default(table_path)?;
            writers.push(writer);
        }

        let stats = Arc::new(Stats::default());
        let mut writer_tasks = Vec::new();
        // Limit outstanding requests to 1000 per writer task to prevent memory exhaustion
        let semaphore = Arc::new(tokio::sync::Semaphore::new(1000));

        println!("# Spawning {} writer tasks...", config.num_writers);
        for (w_idx, writer) in writers.into_iter().enumerate() {
            let writer = Arc::new(writer);
            let stats = stats.clone();
            let semaphore = semaphore.clone();
            let batch_buf = batch_buf.clone();

            let task = tokio::spawn(async move {
                loop {
                    if stats.stop_flag.load(Ordering::Relaxed) {
                        break;
                    }

                    let permit = match semaphore.clone().acquire_owned().await {
                        Ok(p) => p,
                        Err(_) => break,
                    };

                    let rows = ArrowRecordBatch::new()
                        .set_serialized_record_batch(batch_buf.clone());
                    let append = writer.append(rows);

                    stats.send_count.fetch_add(1, Ordering::Relaxed);
                    stats
                        .send_bytes
                        .fetch_add(logical_bytes_per_batch, Ordering::Relaxed);

                    let stats = stats.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        match append.send().await {
                            Ok(_) => {
                                stats.recv_count.fetch_add(1, Ordering::Relaxed);
                                stats
                                    .recv_bytes
                                    .fetch_add(logical_bytes_per_batch, Ordering::Relaxed);
                            }
                            Err(e) => {
                                eprintln!("Write error on writer {}: {:?}", w_idx, e);
                                stats.error_count.fetch_add(1, Ordering::Relaxed);
                                stats.stop_flag.store(true, Ordering::Relaxed);
                            }
                        }
                    });
                }
            });
            writer_tasks.push(task);
        }

        const CSV_HEADER: &str =
            "timestamp,elapsed(s),op,iteration,count,batches/s,bytes,MB/s,errors,errors/s";
        println!("{}", CSV_HEADER);

        let start_time = Instant::now();
        let report_interval = config.report_interval;
        let total_duration = config.duration;
        let mut iteration = 0;

        loop {
            let elapsed = start_time.elapsed();
            if elapsed >= total_duration || stats.stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let interval_start = Instant::now();
            let start_send_count = stats.send_count.load(Ordering::Relaxed);
            let start_send_bytes = stats.send_bytes.load(Ordering::Relaxed);
            let start_recv_count = stats.recv_count.load(Ordering::Relaxed);
            let start_recv_bytes = stats.recv_bytes.load(Ordering::Relaxed);
            let start_error_count = stats.error_count.load(Ordering::Relaxed);

            tokio::time::sleep(report_interval).await;

            let elapsed_interval = interval_start.elapsed();
            let send_count_last = stats.send_count.load(Ordering::Relaxed) - start_send_count;
            let send_bytes_last = stats.send_bytes.load(Ordering::Relaxed) - start_send_bytes;
            let recv_count_last = stats.recv_count.load(Ordering::Relaxed) - start_recv_count;
            let recv_bytes_last = stats.recv_bytes.load(Ordering::Relaxed) - start_recv_bytes;
            let error_count_last = stats.error_count.load(Ordering::Relaxed) - start_error_count;

            print_result(
                "Send",
                iteration,
                send_count_last,
                send_bytes_last,
                0,
                elapsed_interval,
            );
            print_result(
                "Recv",
                iteration,
                recv_count_last,
                recv_bytes_last,
                error_count_last,
                elapsed_interval,
            );

            iteration += 1;
        }

        // Set stop flag to ensure all writer loops exit
        stats.stop_flag.store(true, Ordering::Relaxed);

        // Await all writer loops to finish
        for task in writer_tasks {
            let _ = task.await;
        }

        println!("# Benchmark finished.");
        println!("# Configuration: {:?}", config);
        let total_elapsed = start_time.elapsed();
        let total_elapsed_s = total_elapsed.as_secs_f64();

        let total_send_count = stats.send_count.load(Ordering::Relaxed);
        let total_send_bytes = stats.send_bytes.load(Ordering::Relaxed);
        let total_recv_count = stats.recv_count.load(Ordering::Relaxed);
        let total_recv_bytes = stats.recv_bytes.load(Ordering::Relaxed);
        let total_errors = stats.error_count.load(Ordering::Relaxed);

        let send_rate = (total_send_count as f64) / total_elapsed_s;
        let send_mbs = (total_send_bytes as f64) / total_elapsed_s / 1_000_000.0;

        let recv_rate = (total_recv_count as f64) / total_elapsed_s;
        let recv_mbs = (total_recv_bytes as f64) / total_elapsed_s / 1_000_000.0;

        let error_rate = (total_errors as f64) / total_elapsed_s;
        let error_percentage = if total_recv_count + total_errors > 0 {
            (total_errors as f64) / ((total_recv_count + total_errors) as f64) * 100.0
        } else {
            0.0
        };

        println!("# Summary:");
        println!("# Elapsed time: {:.2}s", total_elapsed_s);
        println!("# Total batches sent: {}", total_send_count);
        println!(
            "# Total data sent: {:.2} MB (rate: {:.2} batches/s, {:.2} MB/s)",
            (total_send_bytes as f64) / 1_000_000.0,
            send_rate,
            send_mbs
        );
        println!("# Total batches completed: {}", total_recv_count);
        println!(
            "# Total data completed: {:.2} MB (rate: {:.2} batches/s, {:.2} MB/s)",
            (total_recv_bytes as f64) / 1_000_000.0,
            recv_rate,
            recv_mbs
        );
        println!(
            "# Total errors: {} (rate: {:.2} errors/s, percentage: {:.2}%)",
            total_errors, error_rate, error_percentage
        );

        Ok(())
    }
    .await;

    // Cleanup dataset and tables
    if is_temp_dataset {
        println!("# Cleaning up temporary dataset: {}", dataset_id);
        if let Err(e) = dataset_service
            .delete_dataset()
            .set_project_id(&config.project)
            .set_dataset_id(&dataset_id)
            .set_delete_contents(true)
            .send()
            .await
        {
            eprintln!("Error cleaning up dataset {}: {:?}", dataset_id, e);
        }
    }

    run_res
}

fn serialize_schema(schema: &Schema) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let _ = StreamWriter::try_new(&mut buf, schema)?;
    Ok(buf)
}

fn serialize_batch(batch: &RecordBatch, schema_len: usize) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut writer = StreamWriter::try_new(&mut buf, &batch.schema())?;
    writer.write(batch)?;
    // Note that the schema is encoded in the front of the record batch. We need
    // to strip it.
    Ok(buf[schema_len..].to_vec())
}

fn print_result(
    operation: &str,
    iteration: i64,
    count: i64,
    bytes: i64,
    errors: i64,
    elapsed: Duration,
) {
    let elapsed_s = elapsed.as_secs_f64();
    let mbs = (bytes as f64) / elapsed_s / 1_000_000.0;
    let msgs = (count as f64) / elapsed_s;
    let errs = (errors as f64) / elapsed_s;
    println!(
        "{},{},{},{},{},{:.2},{},{:.2},{},{:.2}",
        timestamp(),
        elapsed_s,
        operation,
        iteration,
        count,
        msgs,
        bytes,
        mbs,
        errors,
        errs
    );
}

fn timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
