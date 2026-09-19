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

#[cfg(all(test, feature = "run-integration-tests"))]
mod bigquery {
    use google_cloud_test_utils::errors::anydump;
    use google_cloud_test_utils::tracing::enable_tracing;

    #[tokio::test]
    async fn run_dataset_service() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::dataset_admin()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_job_service() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::job_service()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_query_client() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query_client()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_query_client_datatypes() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query_client_datatypes()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_query_client_numeric_limits() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query_client_numeric_limits()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_query_client_multi_page() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query_client_multi_page()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_query_client_job() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query_client_job()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_query_client_nested_types() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query_client_nested_types()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_reads() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::run_reads()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_writes() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::run_writes()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_job_service_poller_heavy() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::job_service_poller_heavy()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_job_service_poller() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::job_service_poller()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn run_job_service_poller_error() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::job_service_poller_error()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_1_1_until_done_and_fetch_page_panics() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_1_1_until_done_and_fetch_page_panics::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_1_2_unbounded_default_retry_policy() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_1_2_unbounded_default_retry_policy::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_1_3_row_iterator_infinite_loop() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_1_3_row_iterator_infinite_loop::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_1_4_derive_macro_variable_shadowing() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_1_4_derive_macro_variable_shadowing::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_1_5_derive_macro_generics() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_1_5_derive_macro_generics::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_2_1_page_size_zero_data_loss() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_2_1_page_size_zero_data_loss::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_2_2_dry_run_and_job_metadata_loss() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_2_2_dry_run_and_job_metadata_loss::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_2_3_complete_query_metadata_loss() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_2_3_complete_query_metadata_loss::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_2_4_timestamp_and_datetime_formats() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_2_4_timestamp_and_datetime_formats::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_2_5_json_column_deserialization() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_2_5_json_column_deserialization::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_2_6_anonymous_and_duplicate_struct_fields() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_2_6_anonymous_and_duplicate_struct_fields::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_2_7_invalid_bigquery_attribute_ignored() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_2_7_invalid_bigquery_attribute_ignored::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_3_1_poll_query_results_backoff_and_timeout() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_3_1_poll_query_results_backoff_and_timeout::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_3_2_attach_job_ignores_error_result() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_3_2_attach_job_ignores_error_result::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_3_3_fetch_page_ignores_errors() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_3_3_fetch_page_ignores_errors::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_3_4_retry_context_off_by_one() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_3_4_retry_context_off_by_one::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_4_1_raw_identifier_field_names() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_4_1_raw_identifier_field_names::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_4_2_derive_from_sql_null_error_variant() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_4_2_derive_from_sql_null_error_variant::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_4_3_case_sensitive_and_duplicate_columns() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_4_3_case_sensitive_and_duplicate_columns::reproduce()
            .await
            .inspect_err(anydump)
    }

    #[tokio::test]
    async fn repro_issue_4_4_broken_rustdoc_example_sql() -> anyhow::Result<()> {
        let _guard = enable_tracing();
        integration_tests_bigquery::query::issue_4_4_broken_rustdoc_example_sql::reproduce()
            .await
            .inspect_err(anydump)
    }
}
