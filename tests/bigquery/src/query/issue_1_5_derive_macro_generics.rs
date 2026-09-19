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

//! Issue 1.5: `#[derive(FromRow)]` and `#[derive(FromSql)]` Fail to Compile on Generic Structs.
//!
//! Neither `derive_from_row` (`src/bigquery-derive/src/lib.rs:66`) nor `derive_from_sql`
//! (`src/bigquery-derive/src/lib.rs:137`) calls `input.generics.split_for_impl()`, so deriving
//! either macro on a struct with type parameters fails to compile with `missing generics for struct`.

use anyhow::Result;
use google_cloud_bigquery::query::{FromRow, FromSql};

#[derive(FromRow, FromSql, Debug, PartialEq)]
pub struct ConcreteWrapper {
    pub value: i64,
}

// UNCOMMENT TO SEE COMPILATION ERROR
/*
#[derive(FromRow, FromSql, Debug, PartialEq)]
pub struct GenericWrapper<T: FromSql> {
    // Fails with: `error[E0107]: missing generics for struct 'GenericWrapper'`
    pub value: T,
}
*/

pub async fn reproduce() -> Result<()> {
    let parsed: ConcreteWrapper =
        FromSql::from_value(wkt::Value::Array(vec![wkt::Value::Number(42.into())]))?;
    assert_eq!(parsed.value, 42);
    Ok(())
}
