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

//! Issue 1.3: Infinite Network Loop in `RowIterator::next()` on Empty Page with `page_token`.
//!
//! In `RowIterator::next()` (`src/bigquery/src/query/iterator.rs:112-137`), if `getQueryResults`
//! returns `rows: []` alongside a non-empty `pageToken` (e.g. when `maxResults=0` or an empty
//! intermediate page is returned), `self.rows.is_empty()` is true and `self.page_token.is_none()`
//! is false, causing `RowIterator::next().await` to spin in an infinite network loop.

use anyhow::{Result, bail};
use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
use google_cloud_bigquery::client::BigQuery;
use httptest::matchers::request;
use httptest::responders::status_code;
use httptest::{Expectation, Server, cycle};
use serde_json::json;
use std::time::Duration;

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
                            "jobId": "job_123"
                        },
                        "schema": {
                            "fields": [{ "name": "x", "type": "INTEGER", "mode": "NULLABLE" }]
                        },
                        "totalRows": "1",
                        "pageToken": "next_page_token_1"
                    })
                    .to_string(),
                ),
        ),
    );

    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/bigquery/v2/projects/test-proj/queries/job_123",
        ))
        .times(..)
        .respond_with(cycle![
            status_code(200)
                .append_header("Content-Type", "application/json")
                .body(
                    json!({
                        "jobComplete": true,
                        "rows": [],
                        "pageToken": "next_page_token_1"
                    })
                    .to_string(),
                )
        ]),
    );

    let client = BigQuery::builder()
        .with_endpoint(format!("http://{}", server.addr()))
        .with_credentials(Anonymous::new().build())
        .with_project_id("test-proj")
        .build()
        .await?;

    let complete = client
        .query("SELECT 1 AS x")
        .set_page_size(0_u32)
        .until_done()
        .await?;

    let mut iter = complete.read();
    match tokio::time::timeout(Duration::from_secs(60), iter.next()).await {
        Ok(_) => Ok(()),
        Err(_) => {
            bail!(
                "Issue 1.3 reproduced: timed out after 60s; infinite recursion/loop in RowIterator::next()"
            );
        }
    }
}
