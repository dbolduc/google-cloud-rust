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

//! Issue 2.6: Anonymous (`SELECT STRUCT(10, 'hello')`) and Duplicate Struct Fields Lose Data or
//! Fail in `convert_nested` & `#[derive(FromSql)]`.
//!
//! `convert_nested` (`src/bigquery/src/query/row.rs:275-283`) converts every `RECORD` cell into
//! `Value::Object(obj)` keyed by `field.name`.
//! - Because `convert_nested` never returns `Value::Array`, the `Value::Array` positional branch
//!   in `#[derive(FromSql)]` (`src/bigquery-derive/src/lib.rs:140-146`) is dead code when reading
//!   from a `Row`, so `SELECT STRUCT(10, 'hello')` (which BigQuery names `_field_1`, `_field_2`)
//!   fails to deserialize into a `#[derive(FromSql)]` struct.
//! - If `TableFieldSchema` contains duplicate field names, `.collect()` into `serde_json::Map`
//!   silently overwrites earlier fields.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_bigquery::query::FromSql;
use google_cloud_test_utils::runtime_config::project_id;

#[derive(FromSql, Debug, PartialEq)]
struct Pair {
    first: i64,
    second: String,
}

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    let complete = bq
        .query("SELECT STRUCT(10, 'hello') AS anon_pair")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .until_done()
        .await?;

    let mut iter = complete.read();
    let row = iter.next().await.expect("expected 1 row")?;

    let pair_res = row.get::<Pair, _>("anon_pair");
    if let Err(err) = pair_res {
        bail!(
            "Issue 2.6 reproduced: positional STRUCT(10, 'hello') failed to deserialize via #[derive(FromSql)] because convert_nested always returns Value::Object: {err}"
        );
    }

    Ok(())
}
