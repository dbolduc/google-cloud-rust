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

//! Issue 1.1: Unconditional `.expect()` panics in `Query::until_done()` and `RowIterator::fetch_page()`.
//!
//! - `Query::until_done()` (`src/bigquery/src/query/query_handle.rs:218-221`) calls
//!   `metadata.job_reference.as_ref().expect("query job should have job reference at this point")`
//!   whenever `completed && cached_rows.is_some()` is false (or if `is_dry_run()` returns false
//!   because `configuration.dry_run` was omitted in the `Job` response).
//! - `RowIterator::fetch_page()` (`src/bigquery/src/query/iterator.rs:140-143`) calls
//!   `self.job_ref.as_ref().expect("only queries with a job reference should have page tokens...")`
//!   whenever a response contains a non-empty `page_token` without a `job_reference`.

use anyhow::{Result, bail};
use futures::FutureExt;
use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
use google_cloud_bigquery::client::BigQuery;
use httptest::matchers::request;
use httptest::responders::status_code;
use httptest::{Expectation, Server};
use serde_json::json;
use std::panic::AssertUnwindSafe;

pub async fn reproduce() -> Result<()> {
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path(
            "POST",
            "/bigquery/v2/projects/test-proj/queries",
        ))
        .respond_with(
            status_code(200)
                .append_header("Content-Type", "application/json")
                // Stateless query response where `jobComplete` is false/omitted and `jobReference` is omitted.
                .body(
                    json!({
                        "queryId": "stateless_q_123",
                        "jobComplete": false
                    })
                    .to_string(),
                ),
        ),
    );

    let client = BigQuery::builder()
        .with_endpoint(format!("http://{}", server.addr()))
        .with_credentials(Anonymous::new().build())
        .with_project_id("test-proj")
        .build()
        .await?;

    let query_handle = client.query("SELECT 1").send().await?;

    // Calling `until_done()` should return a `Result::Err(QueryError)`, NOT panic!
    let outcome = AssertUnwindSafe(query_handle.until_done())
        .catch_unwind()
        .await;

    if let Err(panic_payload) = outcome {
        let msg = panic_payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("unknown panic");
        bail!("Issue 1.1 reproduced: Query::until_done() panicked instead of returning Err: {msg}");
    }

    Ok(())
}
