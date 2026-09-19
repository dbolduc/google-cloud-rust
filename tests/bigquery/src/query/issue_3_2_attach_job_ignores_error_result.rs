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

//! Issue 3.2: `BigQuery::attach_job` Ignores `job.status.error_result` on Failed Jobs.
//!
//! Unlike `InsertJobExecutor::execute` (which calls `check_job_status(res)`),
//! `BigQuery::attach_job` (`src/bigquery/src/query/client.rs:209-245`) passes the `Job`
//! returned by `jobs.get` directly to `QueryHandle::from_job` without checking
//! `job.status.error_result`. Attaching to a failed `DONE` job returns `Ok(QueryHandle)`
//! with `completed = true` and `metadata().errors = []`.

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
            "GET",
            "/bigquery/v2/projects/test-proj/jobs/failed_job_123",
        ))
        .respond_with(
            status_code(200)
                .append_header("Content-Type", "application/json")
                .body(
                    json!({
                        "jobReference": {
                            "projectId": "test-proj",
                            "jobId": "failed_job_123"
                        },
                        "configuration": {
                            "query": { "query": "SELECT * FROM nonexistent" }
                        },
                        "status": {
                            "state": "DONE",
                            "errorResult": {
                                "reason": "notFound",
                                "message": "Not found: Table test-proj:ds.nonexistent"
                            },
                            "errors": [{
                                "reason": "notFound",
                                "message": "Not found: Table test-proj:ds.nonexistent"
                            }]
                        }
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

    let job_ref = google_cloud_bigquery_v2::model::JobReference::new().set_job_id("failed_job_123");
    let attached = client.attach_job(job_ref).await;
    if let Ok(handle) = attached {
        bail!(
            "Issue 3.2 reproduced: attach_job() returned Ok(Query) for a failed DONE job (and metadata().errors is {:?})",
            handle.metadata().errors
        );
    }

    Ok(())
}
