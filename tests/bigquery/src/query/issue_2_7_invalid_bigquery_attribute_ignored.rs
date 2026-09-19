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

//! Issue 2.7: Invalid `#[bigquery(...)]` Attributes Are Silently Ignored in `google-cloud-bigquery-derive`.
//!
//! In `get_field_name` (`src/bigquery-derive/src/lib.rs:169-178`), the `Result` of
//! `attr.parse_nested_meta(...)` is discarded with `let _ = ...`.
//! Consequently, a typo like `#[bigquery(renam = "custom_col")]` or `#[bigquery(rename = 123)]`
//! compiles without any error or warning and silently falls back to the Rust field name.

use anyhow::{Result, bail};
use google_cloud_bigquery::query::FromSql;

#[derive(FromSql, Debug, PartialEq)]
pub struct TypoAttributeStruct {
    // Notice the typo: `renam` instead of `rename`!
    // This should be a compile-time error in `bigquery-derive`, NOT compile silently!
    #[bigquery(renam = "custom_col")]
    pub rust_field_name: i64,
}

pub async fn reproduce() -> Result<()> {
    let input = wkt::Value::Object(wkt::Struct::from_iter([(
        "custom_col".to_string(),
        wkt::Value::Number(99.into()),
    )]));

    match TypoAttributeStruct::from_value(input) {
        Ok(_) => Ok(()),
        Err(err) => {
            bail!(
                "Issue 2.7 reproduced: #[bigquery(renam = \"custom_col\")] silently compiled despite invalid attribute syntax and failed at runtime with: {err}"
            );
        }
    }
}
