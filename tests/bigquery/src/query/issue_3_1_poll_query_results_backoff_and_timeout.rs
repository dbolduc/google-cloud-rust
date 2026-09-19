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

//! Issue 3.1: `Query::until_done()` Hardcodes a 10s Initial Backoff Delay (Ignoring
//! `ClientBuilder::with_backoff_policy`) and `poll_query_results` Forces an Extra Round-Trip
//! (`set_max_results(0)`).
//!
//! In `Query::until_done()` (`src/bigquery/src/query/query_handle.rs:223-228`),
//! `ExponentialBackoffBuilder::default().with_initial_delay(Duration::from_secs(10))` is
//! hardcoded instead of using the client's configured backoff policy, causing a mandatory
//! 10-second sleep whenever the first `getQueryResults` call returns `jobComplete: false`.

use anyhow::{Result, bail};
use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
use google_cloud_bigquery::client::BigQuery;
use google_cloud_gax::exponential_backoff::ExponentialBackoffBuilder;
use httptest::matchers::request;
use httptest::responders::status_code;
use httptest::{Expectation, Server, cycle};
use serde_json::json;
use std::time::{Duration, Instant};

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
                        "jobComplete": false,
                        "jobReference": {
                            "projectId": "test-proj",
                            "jobId": "job_poll_1"
                        }
                    })
                    .to_string(),
                ),
        ),
    );

    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/bigquery/v2/projects/test-proj/queries/job_poll_1",
        ))
        .times(2)
        .respond_with(cycle![
            status_code(200)
                .append_header("Content-Type", "application/json")
                .body(json!({ "jobComplete": false }).to_string()),
            status_code(200)
                .append_header("Content-Type", "application/json")
                .body(json!({ "jobComplete": true, "rows": [] }).to_string())
        ]),
    );

    // Configure a 10ms backoff on the client.
    let fast_backoff = ExponentialBackoffBuilder::default()
        .with_initial_delay(Duration::from_millis(10))
        .with_maximum_delay(Duration::from_millis(50))
        .build()?;

    let client = BigQuery::builder()
        .with_endpoint(format!("http://{}", server.addr()))
        .with_credentials(Anonymous::new().build())
        .with_backoff_policy(fast_backoff)
        .with_project_id("test-proj")
        .build()
        .await?;

    let start = Instant::now();
    let _complete = client.query("SELECT 1").until_done().await?;
    let elapsed = start.elapsed();

    if elapsed >= Duration::from_secs(5) {
        bail!(
            "Issue 3.1 reproduced: until_done() ignored client's 10ms backoff policy and slept for {:?} (hardcoded 10s initial delay)",
            elapsed
        );
    }

    Ok(())
}
