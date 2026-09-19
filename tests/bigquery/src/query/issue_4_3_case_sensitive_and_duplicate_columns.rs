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

//! Issue 4.3: Case-Sensitive Column Lookups & Duplicate Column Shadowing in `Schema`.
//!
//! `Schema::get_field_index_by_name` (`src/bigquery/src/query/schema.rs:26-28`) uses
//! `self.0.fields.iter().position(|f| f.name == name)`:
//! 1. It is strictly case-sensitive, whereas BigQuery SQL column names are case-insensitive.
//! 2. When a query returns duplicate column names (`SELECT 10 AS dup, 20 AS dup`),
//!    `position` always returns index `0`, even after `row.take("dup")` has consumed index `0`.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_test_utils::runtime_config::project_id;

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    let complete = bq
        .query("SELECT 42 AS MY_COL, 10 AS dup, 20 AS dup")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .until_done()
        .await?;

    let mut iter = complete.read();
    let mut row = iter.next().await.expect("expected 1 row")?;

    let lowercase_lookup = row.get::<i64, _>("my_col");
    let first_dup = row.take::<i64, _>("dup")?;
    let second_dup = row.take::<i64, _>("dup");

    if lowercase_lookup.is_err() || second_dup.is_err() {
        bail!(
            "Issue 4.3 reproduced: lowercase lookup for MY_COL={:?}; first take(\"dup\")={}, second take(\"dup\")={:?}",
            lowercase_lookup,
            first_dup,
            second_dup
        );
    }

    Ok(())
}
