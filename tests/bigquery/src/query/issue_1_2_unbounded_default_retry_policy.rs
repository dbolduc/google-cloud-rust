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

//! Issue 1.2: Unbounded Default RPC Retry Policy (`RetryableErrors`) loops forever.
//!
//! `default_retry_policy()` (`src/bigquery/src/query/retry_policy.rs:86-88`) returns
//! `Arc::new(RetryableErrors)` without decorating it with `.with_attempt_limit(...)`
//! or `.with_time_limit(...)`. Any persistent transient error (e.g. HTTP 503) causes
//! `BigQuery` to retry infinitely forever.

use anyhow::{Result, bail};
use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
use google_cloud_bigquery::client::BigQuery;
use google_cloud_gax::exponential_backoff::ExponentialBackoffBuilder;
use httptest::matchers::request;
use httptest::responders::status_code;
use httptest::{Expectation, Server, cycle};
use std::time::Duration;

pub async fn reproduce() -> Result<()> {
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path(
            "POST",
            "/bigquery/v2/projects/test-proj/queries",
        ))
        .times(..)
        .respond_with(cycle![status_code(503).body("Service Unavailable")]),
    );

    // Use a fast backoff so the infinite retry loop cycles rapidly, while leaving
    // the client's default `retry_policy` (`default_retry_policy()`) untouched.
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

    match tokio::time::timeout(Duration::from_secs(60), client.query("SELECT 1").send()).await {
        Ok(res) => {
            assert!(
                res.is_err(),
                "expected error after exhausting retries, got Ok"
            );
            Ok(())
        }
        Err(_) => {
            bail!(
                "Issue 1.2 reproduced: timed out after 60s; infinite recursion/loop in default_retry_policy()"
            );
        }
    }
}
