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

//! Issue 3.3: `RowIterator::fetch_page` Ignores `res.errors`.
//!
//! Unlike `PostQueryExecutor::execute` (`execution.rs:56`) and `poll_query_results`
//! (`query_handle.rs:481`), `RowIterator::fetch_page` (`src/bigquery/src/query/iterator.rs:158-171`)
//! never checks `if !res.errors.is_empty()` on `GetQueryResultsResponse`.

use anyhow::{Result, bail};
use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
use google_cloud_bigquery::client::BigQuery;
use httptest::matchers::request;
use httptest::responders::status_code;
use httptest::{Expectation, Server};
use serde_json::json;

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
                .body(
                    json!({
                        "jobComplete": true,
                        "jobReference": {
                            "projectId": "test-proj",
                            "jobId": "job_page_err_1"
                        },
                        "schema": {
                            "fields": [{ "name": "x", "type": "INTEGER", "mode": "NULLABLE" }]
                        },
                        "rows": [{ "f": [{ "v": "1" }] }],
                        "pageToken": "page_2_token"
                    })
                    .to_string(),
                ),
        ),
    );

    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/bigquery/v2/projects/test-proj/queries/job_page_err_1",
        ))
        .respond_with(
            status_code(200)
                .append_header("Content-Type", "application/json")
                .body(
                    json!({
                        "jobComplete": true,
                        "errors": [{
                            "reason": "backendError",
                            "message": "Error reading result table page"
                        }]
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

    let complete = client.query("SELECT 1 AS x").until_done().await?;
    let mut iter = complete.read();

    // First row from cached page 1:
    let _first_row = iter.next().await.expect("expected first row")?;

    // Second call fetches page 2, which returned `errors: [{ reason: "backendError", ... }]`:
    match iter.next().await {
        Some(Err(_)) => Ok(()),
        None => bail!(
            "Issue 3.3 reproduced: RowIterator::fetch_page silently ignored res.errors and returned None (end of stream) instead of Some(Err(...))"
        ),
        Some(Ok(_)) => bail!("unexpected Ok row"),
    }
}
