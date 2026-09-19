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

//! Issue 4.4: Invalid SQL Column Name (`count` vs `number`) in Crate-Level Rustdoc Examples.
//!
//! Both `src/bigquery/src/lib.rs:80` and `src/bigquery/src/query/client.rs:51` contain the example query:
//! `SELECT name, count FROM bigquery-public-data.usa_names.usa_1910_2013 WHERE state = 'WA' LIMIT 5`
//! In `bigquery-public-data.usa_names.usa_1910_2013`, the column is named `number`, not `count`,
//! so copying and running the crate's main example fails on live BigQuery with:
//! `Unrecognized name: count at [1:14]`.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_test_utils::runtime_config::project_id;

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    let res = bq
        .query("SELECT name, count FROM `bigquery-public-data.usa_names.usa_1910_2013` WHERE state = 'WA' LIMIT 5")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .until_done()
        .await;

    if let Err(err) = res {
        bail!(
            "Issue 4.4 reproduced: rustdoc example SQL in src/bigquery/src/lib.rs:80 and client.rs:51 failed on live BigQuery: {err}"
        );
    }

    Ok(())
}
