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

use super::arrow::WriterBuilder as ArrowWriterBuilder;
use super::client_builder::ClientBuilder;
use super::proto::WriterBuilder as ProtoWriterBuilder;
use super::retry_policy::{default_backoff_policy, default_retry_policy};
use super::transport::Transport;
use crate::ClientBuilderResult as BuilderResult;
use crate::model::{ArrowSchema, ProtoSchema};
use google_cloud_gax::backoff_policy::BackoffPolicy;
use google_cloud_gax::retry_policy::RetryPolicy;
use std::sync::Arc;
use std::time::Duration;

/// A client for BigQuery Storage Write API.
#[derive(Debug)]
pub struct Write {
    inner: Arc<Transport>,
    retry_policy: Arc<dyn RetryPolicy>,
    backoff_policy: Arc<dyn BackoffPolicy>,
    attempt_timeout: Option<Duration>,
}

impl Write {
    /// Creates a new [ClientBuilder].
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub(crate) async fn new(builder: ClientBuilder) -> BuilderResult<Self> {
        let retry_policy = builder
            .config
            .retry_policy
            .clone()
            .unwrap_or_else(default_retry_policy);
        let backoff_policy = builder
            .config
            .backoff_policy
            .clone()
            .unwrap_or_else(default_backoff_policy);
        let attempt_timeout = builder.config.attempt_timeout;
        let transport = Transport::new(builder.config).await?;
        Ok(Self {
            inner: Arc::new(transport),
            retry_policy,
            backoff_policy,
            attempt_timeout,
        })
    }

    /// Create a writer using [Arrow] as the data format.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///   .arrow(schema())
    ///   .default("projects/my-project/datasets/my-dataset/tables/my-table")
    ///   .await?;
    /// # Ok(()) }
    ///
    /// use google_cloud_bigquery::model::ArrowSchema;
    /// fn schema() -> ArrowSchema {
    ///   todo!("Define your table's schema...")
    /// }
    /// ```
    ///
    /// [arrow]: https://arrow.apache.org/
    pub fn arrow(&self, schema: ArrowSchema) -> ArrowWriterBuilder {
        ArrowWriterBuilder::new(
            self.inner.clone(),
            schema,
            self.retry_policy.clone(),
            self.backoff_policy.clone(),
            self.attempt_timeout,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn proto(&self, schema: ProtoSchema) -> ProtoWriterBuilder {
        ProtoWriterBuilder::new(
            self.inner.clone(),
            schema,
            self.retry_policy.clone(),
            self.backoff_policy.clone(),
            self.attempt_timeout,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::AppendError;
    use super::*;
    use crate::model::{ArrowRecordBatch, ArrowSchema, ProtoRows, ProtoSchema};
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::Response as TonicResponse;
    use gaxi::grpc::tonic::Status as TonicStatus;
    use google_cloud_auth::credentials::anonymous::Builder as Anonymous;

    #[tokio::test]
    async fn arrow() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(|_| Err(TonicStatus::failed_precondition("fail")));
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let client = Write::builder()
            .with_endpoint(endpoint)
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        let writer = client
            .arrow(ArrowSchema::new())
            .default("projects/p/datasets/d/tables/t")
            .await?;
        let err = writer
            .append(ArrowRecordBatch::new())
            .send()
            .await
            .expect_err("write should fail");
        assert!(matches!(err, AppendError::Rpc { source: _ }));

        Ok(())
    }

    #[tokio::test]
    async fn proto() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(|_| Err(TonicStatus::failed_precondition("fail")));
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let client = Write::builder()
            .with_endpoint(endpoint)
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        let writer = client
            .proto(ProtoSchema::new())
            .default("projects/p/datasets/d/tables/t")
            .await?;
        let err = writer
            .append(ProtoRows::new())
            .send()
            .await
            .expect_err("write should fail");
        assert!(matches!(err, AppendError::Rpc { source: _ }));

        Ok(())
    }

    #[tokio::test]
    async fn defaults() -> anyhow::Result<()> {
        let (endpoint, _server) = start("0.0.0.0:0", MockBigQueryWrite::new()).await?;
        let client = Write::builder()
            .with_endpoint(endpoint)
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        assert!(client.attempt_timeout.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn arrow_retry_propagation() -> anyhow::Result<()> {
        let (response_tx, response_rx) = tokio::sync::mpsc::channel(1);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(|_| Err(TonicStatus::unavailable("try again")));
        mock.expect_append_rows()
            .times(1)
            .return_once(|_| Ok(TonicResponse::from(response_rx)));
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let mut builder = Write::builder();
        builder.config.retry_policy = Some(test_retry_policy());
        builder.config.backoff_policy = Some(test_backoff_policy());
        let client = builder
            .with_endpoint(endpoint)
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        let writer = client
            .arrow(ArrowSchema::new())
            .default("projects/p/datasets/d/tables/t")
            .await?;
        response_tx.send(Ok(convert(&test_response(1)))).await?;
        let resp = writer.append(ArrowRecordBatch::new()).send().await?;
        assert_eq!(resp.offset, Some(1));

        Ok(())
    }

    #[tokio::test]
    async fn proto_retry_propagation() -> anyhow::Result<()> {
        let (response_tx, response_rx) = tokio::sync::mpsc::channel(1);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(|_| Err(TonicStatus::unavailable("try again")));
        mock.expect_append_rows()
            .times(1)
            .return_once(|_| Ok(TonicResponse::from(response_rx)));
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let mut builder = Write::builder();
        builder.config.retry_policy = Some(test_retry_policy());
        builder.config.backoff_policy = Some(test_backoff_policy());
        let client = builder
            .with_endpoint(endpoint)
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        let writer = client
            .proto(ProtoSchema::new())
            .default("projects/p/datasets/d/tables/t")
            .await?;
        response_tx.send(Ok(convert(&test_response(1)))).await?;
        let resp = writer.append(ProtoRows::new()).send().await?;
        assert_eq!(resp.offset, Some(1));

        Ok(())
    }
}
