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

use super::client_builder::ClientBuilder;
use super::pool::StreamPool;
use super::stream_type::{CreatedStreamType, DefaultStream};
use super::transport::Transport;
use super::writer_builder::WriterBuilder;
use crate::ClientBuilderResult as BuilderResult;
use std::sync::Arc;

/// A client for BigQuery Storage Write API.
#[derive(Debug)]
pub struct Write {
    inner: Arc<Transport>,
    pool: Arc<StreamPool>,
}

impl Write {
    /// Creates a new [ClientBuilder].
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub(crate) async fn new(builder: ClientBuilder) -> BuilderResult<Self> {
        let inner = Arc::new(Transport::new(builder.config).await?);
        let pool = Arc::new(StreamPool::new(inner.clone(), builder.pool_options));
        Ok(Self { inner, pool })
    }

    /// Opens the [default stream] for the given table.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .open_default_stream("projects/my-project/datasets/my-dataset/tables/my-table")
    ///     .build_arrow(schema())
    ///     .await?;
    /// # Ok(()) }
    ///
    /// use google_cloud_bigquery::model::ArrowSchema;
    /// fn schema() -> ArrowSchema {
    ///   todo!("Define your table's schema...")
    /// }
    /// ```
    ///
    /// [default stream]: https://docs.cloud.google.com/bigquery/docs/write-api#default_stream
    pub fn open_default_stream(&self, table: &str) -> WriterBuilder<DefaultStream> {
        WriterBuilder::open_default(self.inner.clone(), self.pool.clone(), table.to_string())
    }

    /// Creates a new [application-created stream] of type `S`
    /// ([`PendingStream`][crate::write::stream_type::PendingStream],
    /// [`CommittedStream`][crate::write::stream_type::CommittedStream], or
    /// [`BufferedStream`][crate::write::stream_type::BufferedStream]) for the given table.
    ///
    /// # Example
    /// ```
    /// use google_cloud_bigquery::write::stream_type::PendingStream;
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .create_stream::<PendingStream>("projects/my-project/datasets/my-dataset/tables/my-table")
    ///     .build_arrow(schema())
    ///     .await?;
    /// # Ok(()) }
    ///
    /// use google_cloud_bigquery::model::ArrowSchema;
    /// fn schema() -> ArrowSchema {
    ///   todo!("Define your table's schema...")
    /// }
    /// ```
    ///
    /// [application-created stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#application-created_streams
    pub fn create_stream<S: CreatedStreamType>(&self, table: &str) -> WriterBuilder<S> {
        WriterBuilder::create(self.inner.clone(), self.pool.clone(), table.to_string())
    }

    /// Attaches to an existing [application-created stream] of type `S`
    /// ([`PendingStream`][crate::write::stream_type::PendingStream],
    /// [`CommittedStream`][crate::write::stream_type::CommittedStream], or
    /// [`BufferedStream`][crate::write::stream_type::BufferedStream]).
    ///
    /// # Example
    /// ```
    /// use google_cloud_bigquery::write::stream_type::CommittedStream;
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .attach_to_stream::<CommittedStream>("projects/my-project/datasets/my_dataset/tables/my_table/streams/my_stream")
    ///     .build_arrow(schema())
    ///     .await?;
    /// # Ok(())
    /// # }
    /// #
    /// # use google_cloud_bigquery::model::ArrowSchema;
    /// # fn schema() -> ArrowSchema {
    /// #   todo!("Define your table's schema...")
    /// # }
    /// ```
    ///
    /// [application-created stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#application-created_streams
    pub fn attach_to_stream<S: CreatedStreamType>(&self, write_stream: &str) -> WriterBuilder<S> {
        WriterBuilder::attach(
            self.inner.clone(),
            self.pool.clone(),
            write_stream.to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::AppendError;
    use super::*;
    use crate::model::{ArrowRecordBatch, ArrowSchema, ProtoRows, ProtoSchema};
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
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
            .open_default_stream("projects/p/datasets/d/tables/t")
            .build_arrow(ArrowSchema::new())
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
            .open_default_stream("projects/p/datasets/d/tables/t")
            .build_proto(ProtoSchema::new())
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
    async fn multiplexing() -> anyhow::Result<()> {
        let client = Write::builder()
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        let multiplexed_writer = client
            .open_default_stream("projects/p/datasets/d/tables/t")
            .with_multiplexing(true)
            .build_arrow(ArrowSchema::new())
            .await?;
        assert!(Arc::ptr_eq(&client.pool, &multiplexed_writer.inner.pool));

        let standalone_writer = client
            .open_default_stream("projects/p/datasets/d/tables/t")
            .with_multiplexing(false)
            .build_arrow(ArrowSchema::new())
            .await?;
        assert!(!Arc::ptr_eq(&client.pool, &standalone_writer.inner.pool));

        Ok(())
    }
}
