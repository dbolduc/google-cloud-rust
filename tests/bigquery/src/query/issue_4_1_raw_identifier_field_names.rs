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

//! Issue 4.1: Raw Identifiers (`r#type`, `r#match`) Lookup `"r#type"` Instead of `"type"`.
//!
//! In `get_field_name` (`src/bigquery-derive/src/lib.rs:184-188`), `field.ident.to_string()`
//! retains the literal `"r#"` prefix instead of calling `syn::ext::IdentExt::unraw(ident).to_string()`.
//! When a user defines `struct EventRow { r#type: String }` for a BigQuery column named `type`,
//! `FromRow` and `FromSql` look up `"r#type"` and fail with `ColumnNotFound("r#type")` /
//! `MissingField("r#type")`.

use crate::INSTANCE_LABEL;
use anyhow::{Result, bail};
use google_cloud_bigquery::client::BigQuery;
use google_cloud_bigquery::query::{FromRow, FromSql};
use google_cloud_test_utils::runtime_config::project_id;

#[derive(FromRow, FromSql, Debug, PartialEq)]
pub struct EventRow {
    pub r#type: String,
}

pub async fn reproduce() -> Result<()> {
    let project_id = project_id()?;
    let bq = BigQuery::builder().build().await?;

    let complete = bq
        .query("SELECT 'click' AS type")
        .with_project_id(project_id)
        .set_labels(vec![(INSTANCE_LABEL, "true")])
        .until_done()
        .await?;

    let mut iter = complete.read();
    let row = iter.next().await.expect("expected 1 row")?;

    match EventRow::try_from(row) {
        Ok(event) => {
            assert_eq!(event.r#type, "click");
            Ok(())
        }
        Err(err) => {
            bail!(
                "Issue 4.1 reproduced: #[derive(FromRow)] failed on raw identifier `r#type` because it looked up column \"r#type\" instead of \"type\": {err}"
            );
        }
    }
}
