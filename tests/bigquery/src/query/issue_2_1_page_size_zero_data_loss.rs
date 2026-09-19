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

//! Issue 2.1: `set_page_size(0)` Silently Drops 100% of Query Result Rows.
//!
//! When `.set_page_size(0_u32)` is configured on `Query` or `RowIterator`, `jobs.query` returns
//! `totalRows: 2`, `rows: []`, and a non-empty `pageToken`. On the first `RowIterator::next().await`,
//! `fetch_page` sends `GetQueryResultsRequest` with `pageToken` and `maxResults=0`. Live BigQuery
//! responds with `rows: []` and `pageToken: ""` (empty string), causing `RowIterator::next()`
//! to immediately return `None` — silently dropping 100% of the result rows without an error.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_test_utils::runtime_config::project_id;

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    let complete = bq
        .query("SELECT 1 AS x UNION ALL SELECT 2 AS x")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .set_page_size(0_u32)
        .until_done()
        .await?;

    assert_eq!(complete.metadata().total_rows, Some(2));

    let mut iter = complete.read();
    let mut rows = Vec::new();
    while let Some(row) = iter.next().await.transpose()? {
        rows.push(row.get::<i64, _>("x")?);
    }

    if rows.len() != 2 {
        bail!(
            "Issue 2.1 reproduced: silent data loss! total_rows was Some(2), but RowIterator yielded {} rows",
            rows.len()
        );
    }

    Ok(())
}
