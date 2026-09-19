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

//! Issue 4.2: `#[derive(FromSql)]` Returns `ConvertError::TypeMismatch` Instead of
//! `ConvertError::NotNull` on `NULL`.
//!
//! All built-in `FromSql` implementations (`String`, `i64`, `f64`, `bool`, `Vec<T>`, `wkt::Struct`,
//! `wkt::Timestamp`, `Date`, `TimeOfDay`, `DateTime`, `Decimal`, `Interval`, `Range<T>`) return
//! `Err(ConvertError::NotNull)` when passed `wkt::Value::Null`.
//! However, `#[derive(FromSql)]` (`src/bigquery-derive/src/lib.rs:153-156`) falls into its wildcard
//! `other` arm and returns `Err(ConvertError::TypeMismatch { expected: "array or object", got: Null })`.

use anyhow::{Result, bail};
use google_cloud_bigquery::error::ConvertError;
use google_cloud_bigquery::query::FromSql;

#[derive(FromSql, Debug, PartialEq)]
pub struct MyNestedStruct {
    pub id: i64,
}

pub async fn reproduce() -> Result<()> {
    let err = MyNestedStruct::from_value(wkt::Value::Null).unwrap_err();
    if !matches!(err, ConvertError::NotNull) {
        bail!(
            "Issue 4.2 reproduced: #[derive(FromSql)] returned {err:?} on Value::Null instead of ConvertError::NotNull"
        );
    }
    Ok(())
}
