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

//! Issue 1.4: Variable Shadowing Breaks Compilation in `#[derive(FromRow)]` and `#[derive(FromSql)]`.
//!
//! - In `#[derive(FromRow)]` (`src/bigquery-derive/src/lib.rs:51-77`), `fn try_from(mut row: Row)`
//!   binds each struct field via `let #field_name = row.take(...)?;`. A struct with a field named
//!   `row` before the last field shadows `mut row: Row` and fails to compile.
//! - In `#[derive(FromSql)]` (`src/bigquery-derive/src/lib.rs:116-158`), internal variables
//!   `mut iter` and `mut obj` are shadowed if a struct has a field named `iter` or `obj` before
//!   another field.

use anyhow::Result;
use google_cloud_bigquery::query::{FromRow, FromSql};

// Workaround that compiles today (putting renamed identifiers instead of `row`, `iter`, `obj`):
#[derive(FromRow, Debug, PartialEq)]
pub struct WorkaroundRow {
    #[bigquery(rename = "row")]
    pub row_val: i64,
    pub name: String,
}

#[derive(FromSql, Debug, PartialEq)]
pub struct WorkaroundSqlStruct {
    #[bigquery(rename = "iter")]
    pub iter_val: i64,
    #[bigquery(rename = "obj")]
    pub obj_val: i64,
    pub name: String,
}

// UNCOMMENT TO SEE COMPILATION ERROR
/*
#[derive(FromRow, Debug, PartialEq)]
pub struct BrokenShadowRow {
    // Shadows `mut row: google_cloud_bigquery::query::Row` inside `try_from`!
    // Next line fails with: `error[E0599]: no method named 'take' found for type 'i64'`
    pub row: i64,
    pub name: String,
}

#[derive(FromSql, Debug, PartialEq)]
pub struct BrokenShadowSqlIter {
    // Shadows `let mut iter = arr.into_iter();` inside `FromSql::from_value`!
    // Next line fails with: `error[E0599]: no method named 'next' found for type 'i64'`
    pub iter: i64,
    pub name: String,
}

#[derive(FromSql, Debug, PartialEq)]
pub struct BrokenShadowSqlObj {
    // Shadows `wkt::Value::Object(mut obj)` inside `FromSql::from_value`!
    // Next line fails with: `error[E0599]: no method named 'remove' found for type 'i64'`
    pub obj: i64,
    pub name: String,
}
*/

pub async fn reproduce() -> Result<()> {
    let s: WorkaroundSqlStruct = FromSql::from_value(wkt::Value::Array(vec![
        wkt::Value::Number(1.into()),
        wkt::Value::Number(2.into()),
        wkt::Value::String("ok".to_string()),
    ]))?;
    assert_eq!(s.iter_val, 1);
    assert_eq!(s.obj_val, 2);
    Ok(())
}
