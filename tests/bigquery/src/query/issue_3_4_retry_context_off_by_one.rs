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

//! Issue 3.4: `RetryContext` & `RetryableJobErrors` Off-By-One Attempt Count (4 Attempts Instead of 3).
//!
//! `RetryableJobErrors::default()` sets `attempt_limit: 3` (`src/bigquery/src/query/retry_policy.rs:145`).
//! However, `RetryContext::execute` (`src/bigquery/src/query/execution.rs:136-140`) calls
//! `self.on_error(err)` with `state.attempt_count == 0` BEFORE incrementing `self.state.attempt_count += 1`.
//! Consequently, `state.attempt_count >= 3` allows `attempt_count = 0, 1, 2` to continue,
//! executing 4 total attempts instead of 3.

use anyhow::{Result, bail};
use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
use google_cloud_bigquery::client::BigQuery;
use httptest::matchers::request;
use httptest::responders::status_code;
use httptest::{Expectation, Server, cycle};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub async fn reproduce() -> Result<()> {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();

    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path(
            "POST",
            "/bigquery/v2/projects/test-proj/queries",
        ))
        .times(..)
        .respond_with(cycle![{
            let counter = attempts_clone.clone();
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                status_code(200)
                    .append_header("Content-Type", "application/json")
                    .body(
                        json!({
                            "errors": [{
                                "reason": "backendError",
                                "message": "Temporary backend failure"
                            }]
                        })
                        .to_string(),
                    )
            }
        }]),
    );

    let client = BigQuery::builder()
        .with_endpoint(format!("http://{}", server.addr()))
        .with_credentials(Anonymous::new().build())
        .with_project_id("test-proj")
        .build()
        .await?;

    let _ = client.query("SELECT 1").send().await;
    let total_attempts = attempts.load(Ordering::SeqCst);

    if total_attempts != 3 {
        bail!(
            "Issue 3.4 reproduced: RetryableJobErrors has attempt_limit = 3, but RetryContext executed {total_attempts} attempts due to off-by-one attempt_count increment"
        );
    }

    Ok(())
}
