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

//! Issue 2.2: `QueryMetadata::from(Job)` Drops `total_bytes_processed`, `schema`, and `job_complete`
//! on All `dry_run` and `jobs.insert` Queries.
//!
//! `RetryContext::force_job_path()` routes all `dry_run = true` queries to `jobs.insert`, which
//! builds `QueryMetadata::from(Job)` (`src/bigquery/src/generated/query_metadata.rs:733-750`).
//! Because `From<Job>` only copies top-level `Job` fields and ignores `job.statistics` and
//! `job.status`, `query.metadata().total_bytes_processed`, `schema`, and `job_complete` are
//! always `None` on dry-run queries.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_test_utils::runtime_config::project_id;

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    let query = bq
        .query("SELECT name, number FROM `bigquery-public-data.usa_names.usa_1910_2013` WHERE state = 'TX' LIMIT 10")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .set_dry_run(true)
        .set_location("US")
        .send()
        .await?;

    let meta = query.metadata();
    let raw_stats_bytes = meta
        .statistics
        .as_ref()
        .and_then(|s| s.total_bytes_processed);

    if meta.total_bytes_processed.is_none() || meta.schema.is_none() {
        bail!(
            "Issue 2.2 reproduced: dry_run QueryMetadata has total_bytes_processed={:?} and schema={:?} (even though statistics.total_bytes_processed={:?})",
            meta.total_bytes_processed,
            meta.schema,
            raw_stats_bytes
        );
    }

    Ok(())
}
