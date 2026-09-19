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

//! Issue 2.3: `CompleteQueryMetadata::from(GetQueryResultsResponse)` Wipes Out 11 Metadata Fields
//! on All Polled and `jobs.insert` Queries.
//!
//! Whenever `Query::until_done()` calls `poll_query_results` (which happens for 100% of `jobs.insert`
//! queries such as `.set_priority("INTERACTIVE")` or `.set_allow_large_results(true)`),
//! `CompleteQuery::from_get_query_results_response` discards the initial `QueryMetadata` and
//! builds `CompleteQueryMetadata` solely from `GetQueryResultsResponse` — wiping out `creation_time`,
//! `start_time`, `end_time`, `statement_type`, `location`, `total_slot_ms`, `total_bytes_billed`, etc.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_test_utils::runtime_config::project_id;

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    // Force the `jobs.insert` path via `set_priority("INTERACTIVE")`.
    let complete = bq
        .query("SELECT 1 AS one")
        .set_priority("INTERACTIVE")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .until_done()
        .await?;

    let meta = complete.metadata();
    if meta.creation_time.is_none()
        || meta.end_time.is_none()
        || meta.statement_type.is_empty()
        || meta.location.is_empty()
    {
        bail!(
            "Issue 2.3 reproduced: CompleteQueryMetadata lost fields after poll_query_results: creation_time={:?}, end_time={:?}, statement_type={:?}, location={:?}",
            meta.creation_time,
            meta.end_time,
            meta.statement_type,
            meta.location
        );
    }

    Ok(())
}
