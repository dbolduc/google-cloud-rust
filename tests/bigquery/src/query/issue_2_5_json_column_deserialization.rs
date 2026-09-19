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

//! Issue 2.5: BigQuery `JSON` Columns Cannot Be Deserialized into `wkt::Struct`, `#[derive(FromSql)]`,
//! or `Vec<T>`, and `wkt::Value` Returns an Unparsed `Value::String`.
//!
//! In `convert_basic_type` (`src/bigquery/src/query/row.rs:287-290`), `"JSON"` is grouped with
//! `"STRING"` and returned as `Value::String(String)` without parsing the JSON payload.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_bigquery::query::FromSql;
use google_cloud_test_utils::runtime_config::project_id;

#[derive(FromSql, Debug, PartialEq)]
struct PersonJson {
    name: String,
    age: i64,
}

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    let complete = bq
        .query("SELECT JSON '{\"name\": \"Alice\", \"age\": 30}' AS json_obj, JSON '[1, 2, 3]' AS json_arr")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .until_done()
        .await?;

    let mut iter = complete.read();
    let row = iter.next().await.expect("expected 1 row")?;

    let as_struct = row.get::<wkt::Struct, _>("json_obj");
    let as_from_sql = row.get::<PersonJson, _>("json_obj");
    let as_vec = row.get::<Vec<i64>, _>("json_arr");
    let as_value = row.get::<wkt::Value, _>("json_obj")?;

    if as_struct.is_err()
        || as_from_sql.is_err()
        || as_vec.is_err()
        || matches!(as_value, wkt::Value::String(_))
    {
        bail!(
            "Issue 2.5 reproduced: JSON column failed deserialization: wkt::Struct={:?}, FromSql={:?}, Vec<i64>={:?}, wkt::Value={:?}",
            as_struct,
            as_from_sql,
            as_vec,
            as_value
        );
    }

    Ok(())
}
