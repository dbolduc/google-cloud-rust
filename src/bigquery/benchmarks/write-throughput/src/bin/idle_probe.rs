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
use bigquery_write_throughput::table::BenchmarkEnvironment;
use clap::Parser;
use google_cloud_bigquery::client::Write;
use google_cloud_bigquery::model::{ArrowRecordBatch, ArrowSchema};
use humantime::parse_duration;
use std::fs::File;
use std::io::Write as IoWrite;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Parser, Debug, Clone)]
#[command(
    author,
    version,
    about = "BigQuery Storage Write API Idle Connection Prober (2^n seconds)",
    long_about = "Probes the BigQuery Storage Write API with an exponential idle delay schedule (2^n seconds)\n\
                  to detect server idle connection timeouts and verify client retry and reconnect behavior."
)]
pub struct ProbeConfig {
    /// GCP project ID. Defaults to GOOGLE_CLOUD_PROJECT environment variable.
    #[arg(long, default_value = "", env = "GOOGLE_CLOUD_PROJECT")]
    pub project: String,

    /// BigQuery dataset ID. If empty, a temporary dataset will be created and cleaned up.
    #[arg(long, default_value = "")]
    pub dataset_id: String,

    /// BigQuery table ID. Defaults to 'idle_probe_table'.
    #[arg(long, default_value = "idle_probe_table")]
    pub table_id: String,

    /// Starting exponent n (sleep = base^n seconds).
    #[arg(long, default_value_t = 0)]
    pub start_n: u32,

    /// Maximum exponent n (sleep = base^n seconds).
    /// Default is 11 (2^11 = 2048s ≈ 34.1 minutes).
    #[arg(long, default_value_t = 11)]
    pub max_n: u32,

    /// Base for exponential sleep calculation: sleep = base^n seconds.
    #[arg(long, default_value_t = 2.0)]
    pub base: f64,

    /// Number of rows to append per probe batch.
    #[arg(long, default_value_t = 1)]
    pub rows_per_batch: usize,

    /// Payload size in bytes per row.
    #[arg(long, default_value_t = 128)]
    pub row_size: usize,

    /// Number of gRPC channels to configure on the client.
    #[arg(long, default_value_t = 1)]
    pub grpc_channels: usize,

    /// Enable connection multiplexing on the writer.
    #[arg(long, default_value_t = false)]
    pub multiplex: bool,

    /// Multiplex pool size (if multiplexing is enabled).
    #[arg(long, default_value_t = 4)]
    pub multiplex_pool_size: usize,

    /// Timeout for an individual RPC attempt (e.g. 30s, 1m).
    #[arg(long, value_parser = parse_duration)]
    pub attempt_timeout: Option<Duration>,

    /// Keep temporary dataset and table instead of deleting on completion.
    #[arg(long, default_value_t = false)]
    pub keep_dataset: bool,

    /// Optional path to write CSV results to.
    #[arg(long)]
    pub csv_output: Option<String>,
}

#[derive(Debug, Clone)]
struct StepResult {
    step: u32,
    idle_duration: Duration,
    latency: Duration,
    success: bool,
    reconnected: bool,
    error_msg: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ProbeConfig::parse();
    if config.project.is_empty() {
        anyhow::bail!(
            "GOOGLE_CLOUD_PROJECT environment variable or --project argument must be set"
        );
    }
    if config.start_n > config.max_n {
        anyhow::bail!(
            "--start-n ({}) must be less than or equal to --max-n ({})",
            config.start_n,
            config.max_n
        );
    }

    let total_sleep_secs: u64 = (config.start_n..=config.max_n)
        .map(|n| (config.base.powi(n as i32)).round() as u64)
        .sum();

    println!("# ==============================================================================");
    println!("# BigQuery Storage Write API - Exponential Idle Connection Prober");
    println!("# ==============================================================================");
    println!("# Project:            {}", config.project);
    println!(
        "# Dataset:            {}",
        if config.dataset_id.is_empty() {
            "(auto-generated temporary dataset)"
        } else {
            &config.dataset_id
        }
    );
    println!("# Table:              {}", config.table_id);
    println!(
        "# Exponent Range:     n = {}..={} (base: {})",
        config.start_n, config.max_n, config.base
    );
    println!(
        "# Total Idle Sleep:   {}s ({:.1} minutes)",
        total_sleep_secs,
        total_sleep_secs as f64 / 60.0
    );
    println!("# Multiplexing:       {}", config.multiplex);
    if config.multiplex {
        println!("# Multiplex Pool:     {}", config.multiplex_pool_size);
    }
    println!("# gRPC Channels:      {}", config.grpc_channels);
    println!("# Attempt Timeout:    {:?}", config.attempt_timeout);
    println!(
        "# Row Size / Batch:   {} bytes, {} rows",
        config.row_size, config.rows_per_batch
    );
    println!("# ==============================================================================");

    let mut env = BenchmarkEnvironment::setup_with_table(
        &config.project,
        &config.dataset_id,
        &config.table_id,
    )
    .await?;

    if config.keep_dataset {
        env.keep_dataset();
    }

    let table_path = format!(
        "projects/{}/datasets/{}/tables/{}",
        env.project, env.dataset_id, config.table_id
    );

    let res = run_prober(&config, &table_path).await;

    println!("# Prober finished. Cleaning up environment...");
    env.cleanup().await;

    res
}

async fn run_prober(config: &ProbeConfig, table_path: &str) -> anyhow::Result<()> {
    let mut client_builder = Write::builder()
        .with_grpc_subchannel_count(config.grpc_channels)
        .with_multiplex_pool_size(config.multiplex_pool_size);
    if let Some(t) = config.attempt_timeout {
        client_builder = client_builder.with_attempt_timeout(t);
    }
    let client = Arc::new(client_builder.build().await?);

    let schema = Arc::new(Schema::new(vec![Field::new(
        "payload",
        DataType::Utf8,
        false,
    )]));

    let rows: Vec<String> = (0..config.rows_per_batch)
        .map(|r| {
            let mut s = format!("idle-probe-row-{:0width$}", r, width = config.row_size);
            if s.len() > config.row_size {
                s.truncate(config.row_size);
            }
            s
        })
        .collect();
    let row_slices: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
    let payload_array = StringArray::from(row_slices);
    let raw_batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(payload_array)])?;

    let mut ipc_writer = StreamWriter::try_new(Vec::new(), &schema)?;
    let schema_buf = std::mem::take(ipc_writer.get_mut());
    ipc_writer.write(&raw_batch)?;
    let batch_bytes = bytes::Bytes::from(std::mem::take(ipc_writer.get_mut()));

    let arrow_schema = ArrowSchema::new().set_serialized_schema(schema_buf);
    let writer = Arc::new(
        client
            .arrow(arrow_schema)
            .with_multiplexing(config.multiplex)
            .default(table_path.to_string())
            .await?,
    );

    // Warm-up probe: initial write to establish the stream connection
    let now = humantime::format_rfc3339(std::time::SystemTime::now());
    println!("# [{now}] [Warmup] Sending initial write to establish stream connection...");
    let warmup_start = Instant::now();
    let warmup_rows = ArrowRecordBatch::new().set_serialized_record_batch(batch_bytes.clone());
    let warmup_res = writer.append(warmup_rows).send().await;
    let baseline_latency = warmup_start.elapsed();

    match warmup_res {
        Ok(_) => {
            let now = humantime::format_rfc3339(std::time::SystemTime::now());
            println!(
                "# [{now}] [Warmup] Stream established successfully. Latency: {:.1?}.",
                baseline_latency
            );
        }
        Err(e) => {
            let now = humantime::format_rfc3339(std::time::SystemTime::now());
            println!("# [{now}] [Warmup] Initial stream write failed: {e:?}");
            anyhow::bail!("Warmup write failed: {:?}", e);
        }
    }

    let mut results: Vec<StepResult> = Vec::new();
    let mut cumulative_elapsed = Duration::from_secs(0);

    println!("#");
    println!(
        "# Starting exponential idle probe sequence (n = {}..={})...",
        config.start_n, config.max_n
    );
    println!("# Press Ctrl+C at any time to abort early and view the partial summary.");
    println!("#");

    for n in config.start_n..=config.max_n {
        let sleep_secs = (config.base.powi(n as i32)).round() as u64;
        let sleep_duration = Duration::from_secs(sleep_secs);
        cumulative_elapsed += sleep_duration;

        let now = humantime::format_rfc3339(std::time::SystemTime::now());
        println!(
            "# ------------------------------------------------------------------------------"
        );
        println!(
            "# [{now}] Step n={n:<2} | Idle Sleep: {:<6?} ({}) | Cumulative Wait: {:<6?}",
            sleep_duration,
            format_duration_hms(sleep_duration),
            cumulative_elapsed
        );
        println!("# Sleeping for {:?}...", sleep_duration);

        // Sleep with Ctrl+C interruption support
        tokio::select! {
            _ = tokio::time::sleep(sleep_duration) => {}
            _ = tokio::signal::ctrl_c() => {
                println!("\n# Received Ctrl+C interruption! Stopping probe early at step n={n}...");
                break;
            }
        }

        let write_start = Instant::now();
        let rows = ArrowRecordBatch::new().set_serialized_record_batch(batch_bytes.clone());
        let append_res = writer.append(rows).send().await;
        let latency = write_start.elapsed();

        let now = humantime::format_rfc3339(std::time::SystemTime::now());
        // Reconnection typically takes >150ms or >3x the warm baseline
        let reconnected = latency > Duration::from_millis(150) && latency > baseline_latency * 2;

        match append_res {
            Ok(_) => {
                let note = if reconnected {
                    "RECONNECTED (stream re-established)"
                } else {
                    "SUCCESS (stream remained warm)"
                };
                println!(
                    "# [{now}] Step n={n:<2} Result: SUCCESS | Latency: {:<8.1?} | Status: {note}",
                    latency
                );
                results.push(StepResult {
                    step: n,
                    idle_duration: sleep_duration,
                    latency,
                    success: true,
                    reconnected,
                    error_msg: None,
                });
            }
            Err(e) => {
                println!(
                    "# [{now}] Step n={n:<2} Result: FAILED  | Latency: {:<8.1?} | Error: {e:?}",
                    latency
                );
                results.push(StepResult {
                    step: n,
                    idle_duration: sleep_duration,
                    latency,
                    success: false,
                    reconnected: false,
                    error_msg: Some(format!("{e:?}")),
                });
            }
        }
    }

    print_summary_table(&results, baseline_latency);

    if let Some(csv_path) = &config.csv_output {
        write_csv(csv_path, &results)?;
        println!("# Saved CSV results to: {csv_path}");
    }

    Ok(())
}

fn format_duration_hms(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours}h {mins:02}m {secs:02}s")
    } else if mins > 0 {
        format!("{mins}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

fn print_summary_table(results: &[StepResult], baseline_latency: Duration) {
    println!("\n# ==============================================================================");
    println!("# Idle Connection Prober - Results Summary");
    println!("# ==============================================================================");
    println!(
        "# {:>3} | {:>14} | {:>10} | {:>8} | {:>12} | Notes",
        "n", "Idle Sleep", "Human Time", "Status", "Latency"
    );
    println!(
        "# {:-<3}-+-{:-<14}-+-{:-<10}-+-{:-<8}-+-{:-<12}-+----------------------------",
        "", "", "", "", ""
    );

    let mut first_reconnect_n: Option<u32> = None;
    let mut total_reconnects = 0;
    let mut total_failures = 0;

    for r in results {
        let status_str = if !r.success {
            total_failures += 1;
            "FAILED"
        } else if r.reconnected {
            total_reconnects += 1;
            if first_reconnect_n.is_none() {
                first_reconnect_n = Some(r.step);
            }
            "RECONN"
        } else {
            "SUCCESS"
        };

        let notes = if let Some(err) = &r.error_msg {
            err.chars().take(28).collect::<String>()
        } else if r.reconnected {
            "Reconnected after idle drop".to_string()
        } else {
            "Warm stream reused".to_string()
        };

        println!(
            "# {:>3} | {:>13}s | {:>10} | {:>8} | {:>10.1?} | {}",
            r.step,
            r.idle_duration.as_secs(),
            format_duration_hms(r.idle_duration),
            status_str,
            r.latency,
            notes
        );
    }

    println!("# ==============================================================================");
    println!("# Baseline warmup latency:       {:.1?}", baseline_latency);
    println!("# Total probe steps completed:   {}", results.len());
    let success_count = results.iter().filter(|r| r.success).count();
    let success_pct = if !results.is_empty() {
        (success_count as f64 / results.len() as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "# Successful writes:             {} / {} ({:.1}%)",
        success_count,
        results.len(),
        success_pct
    );
    println!("# Total stream reconnects:       {}", total_reconnects);
    println!("# Total write failures:          {}", total_failures);

    if let Some(n) = first_reconnect_n {
        let dur = results
            .iter()
            .find(|r| r.step == n)
            .map(|r| r.idle_duration)
            .unwrap_or_default();
        println!(
            "# First idle drop occurred at:   n = {} (idle sleep: {}s / {})",
            n,
            dur.as_secs(),
            format_duration_hms(dur)
        );
    } else if results.is_empty() {
        println!("# No steps completed.");
    } else {
        println!("# No stream reconnects observed across completed steps.");
    }
    println!("# ==============================================================================\n");
}

fn write_csv(path: &str, results: &[StepResult]) -> anyhow::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "step_n,idle_seconds,human_time,latency_ms,success,reconnected,error"
    )?;
    for r in results {
        writeln!(
            file,
            "{},{},{},{:.2},{},{},\"{}\"",
            r.step,
            r.idle_duration.as_secs(),
            format_duration_hms(r.idle_duration),
            r.latency.as_secs_f64() * 1000.0,
            r.success,
            r.reconnected,
            r.error_msg.as_deref().unwrap_or("")
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_hms() {
        assert_eq!(format_duration_hms(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration_hms(Duration::from_secs(65)), "1m 05s");
        assert_eq!(format_duration_hms(Duration::from_secs(3665)), "1h 01m 05s");
    }

    #[test]
    fn test_parse_args_defaults() -> anyhow::Result<()> {
        let args = ProbeConfig::try_parse_from(["cmd"])?;
        let expected_project = std::env::var("GOOGLE_CLOUD_PROJECT").unwrap_or_default();
        assert_eq!(args.project, expected_project);
        assert_eq!(args.start_n, 0);
        assert_eq!(args.max_n, 11);
        assert_eq!(args.base, 2.0);
        assert_eq!(args.rows_per_batch, 1);
        assert_eq!(args.row_size, 128);
        assert_eq!(args.grpc_channels, 1);
        assert!(!args.multiplex);
        assert_eq!(args.multiplex_pool_size, 4);
        assert_eq!(args.attempt_timeout, None);
        assert!(!args.keep_dataset);
        assert_eq!(args.table_id, "idle_probe_table");
        assert_eq!(args.dataset_id, "");
        assert_eq!(args.csv_output, None);
        Ok(())
    }

    #[test]
    fn test_parse_args_custom() -> anyhow::Result<()> {
        let args = ProbeConfig::try_parse_from([
            "cmd",
            "--project",
            "test-proj",
            "--dataset-id",
            "test_ds",
            "--table-id",
            "test_tbl",
            "--start-n",
            "3",
            "--max-n",
            "8",
            "--base",
            "3.0",
            "--rows-per-batch",
            "10",
            "--row-size",
            "256",
            "--grpc-channels",
            "2",
            "--multiplex",
            "--multiplex-pool-size",
            "8",
            "--attempt-timeout",
            "45s",
            "--keep-dataset",
            "--csv-output",
            "out.csv",
        ])?;
        assert_eq!(args.project, "test-proj");
        assert_eq!(args.dataset_id, "test_ds");
        assert_eq!(args.table_id, "test_tbl");
        assert_eq!(args.start_n, 3);
        assert_eq!(args.max_n, 8);
        assert_eq!(args.base, 3.0);
        assert_eq!(args.rows_per_batch, 10);
        assert_eq!(args.row_size, 256);
        assert_eq!(args.grpc_channels, 2);
        assert!(args.multiplex);
        assert_eq!(args.multiplex_pool_size, 8);
        assert_eq!(args.attempt_timeout, Some(Duration::from_secs(45)));
        assert!(args.keep_dataset);
        assert_eq!(args.csv_output, Some("out.csv".to_string()));
        Ok(())
    }
}
