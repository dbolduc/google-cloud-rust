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

//! Issue 2.4: `poll_query_results` Omits `DataFormatOptions { use_int64_timestamp: true }` &
//! `DateTime::from_value` Rejects Space-Separated Datetime Strings.
//!
//! - `poll_query_results` (`src/bigquery/src/query/query_handle.rs:467-473`) does not set
//!   `format_options.use_int64_timestamp = true`. When `getQueryResults` returns rows without
//!   `use_int64_timestamp = true`, BigQuery formats `TIMESTAMP` cells as floating-point seconds
//!   (`"1.7799822E9"`) and `RANGE<TIMESTAMP>` bounds as ISO strings (`"[2026-05-28 15:30:00+00, UNBOUNDED)"`),
//!   both of which fail `wkt::Timestamp::from_value` (`s.parse::<i64>()`).
//! - `DateTime::from_value` (`src/bigquery/src/query/from_sql.rs:31`) strictly requires `'T'`
//!   and fails on standard SQL space-separated datetimes (`"2026-05-28 15:30:00"`).

use anyhow::{Result, bail};
use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
use google_cloud_bigquery::client::BigQuery;
use google_cloud_bigquery::query::FromSql;
use httptest::matchers::request;
use httptest::responders::status_code;
use httptest::{Expectation, Server};
use serde_json::json;

pub async fn reproduce() -> Result<()> {
    // Part 1: Verify `poll_query_results` omits `formatOptions.useInt64Timestamp = true`.
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path(
            "POST",
            "/bigquery/v2/projects/test-proj/jobs",
        ))
        .respond_with(
            status_code(200)
                .append_header("Content-Type", "application/json")
                .body(
                    json!({
                        "jobReference": {
                            "projectId": "test-proj",
                            "jobId": "job_ts_1"
                        },
                        "status": { "state": "DONE" }
                    })
                    .to_string(),
                ),
        ),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/bigquery/v2/projects/test-proj/queries/job_ts_1",
        ))
        .respond_with(
            status_code(200)
                .append_header("Content-Type", "application/json")
                .body(
                    // Because `poll_query_results` omitted `formatOptions.useInt64Timestamp=true`,
                    // BigQuery formats TIMESTAMP as float seconds (`"1.7799822E9"`).
                    json!({
                        "jobComplete": true,
                        "schema": {
                            "fields": [{ "name": "ts", "type": "TIMESTAMP", "mode": "NULLABLE" }]
                        },
                        "rows": [
                            { "f": [{ "v": "1.7799822E9" }] }
                        ]
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

    let complete = client
        .query("SELECT CURRENT_TIMESTAMP() AS ts")
        .set_priority("INTERACTIVE") // Forces jobs.insert -> poll_query_results
        .until_done()
        .await?;

    let mut iter = complete.read();
    let row = iter.next().await.expect("expected 1 row")?;
    let ts_res = row.get::<wkt::Timestamp, _>("ts");

    // Part 2: Space-separated DATETIME string ("2026-05-28 15:30:00").
    let dt_res = google_cloud_type::model::DateTime::from_value(wkt::Value::String(
        "2026-05-28 15:30:00".to_string(),
    ));

    if ts_res.is_err() || dt_res.is_err() {
        bail!(
            "Issue 2.4 reproduced: timestamp from poll_query_results={:?}, space-separated DateTime={:?}",
            ts_res,
            dt_res
        );
    }

    Ok(())
}
