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

use super::arrow::{
    BufferedWriter, CommittedWriter, DefaultWriter, PendingWriter, Writer, WriterBuilder,
};
use super::client_builder::ClientBuilder;
use super::error::{AttachError, AttachResult};
use super::generated::gapic_storage::client::BigQueryWrite;
use super::pool::StreamPool;
use super::transport::Transport;
use super::validate::{validate_stream, validate_table};
use crate::model::WriteStream;
use crate::model::write_stream::Type;
use crate::{ClientBuilderResult as BuilderResult, Result};
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
    ///     .await?
    ///     .with_arrow_format(schema());
    /// # Ok(()) }
    ///
    /// use google_cloud_bigquery::model::ArrowSchema;
    /// fn schema() -> ArrowSchema {
    ///   todo!("Define your table's schema...")
    /// }
    /// ```
    ///
    /// [default stream]: https://docs.cloud.google.com/bigquery/docs/write-api#default_stream
    pub async fn open_default_stream<T: Into<String>>(
        &self,
        table: T,
    ) -> Result<WriterBuilder<DefaultWriter>> {
        let table = table.into();
        validate_table(table.as_str())?;
        let mut write_stream = table;
        write_stream.push_str("/streams/_default");
        Ok(WriterBuilder::new(
            self.inner.clone(),
            self.pool.clone(),
            write_stream,
        ))
    }

    /// Creates a new [pending stream] for the given table.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .create_pending_stream("projects/my-project/datasets/my-dataset/tables/my-table")
    ///     .await?
    ///     .with_arrow_format(schema());
    /// # Ok(()) }
    ///
    /// use google_cloud_bigquery::model::ArrowSchema;
    /// fn schema() -> ArrowSchema {
    ///   todo!("Define your table's schema...")
    /// }
    /// ```
    ///
    /// [pending stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#pending_type
    pub async fn create_pending_stream<T: Into<String>>(
        &self,
        table: T,
    ) -> Result<WriterBuilder<PendingWriter>> {
        let table = table.into();
        validate_table(table.as_str())?;

        let client = BigQueryWrite::from_stub::<Transport>(self.inner.clone());
        let write_stream = client
            .create_write_stream()
            .set_parent(table)
            .set_write_stream(WriteStream::new().set_type(Type::Pending))
            .send()
            .await?;

        Ok(WriterBuilder::new(
            self.inner.clone(),
            self.pool.clone(),
            write_stream.name,
        ))
    }

    /// Creates a new [committed stream] for the given table.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .create_committed_stream("projects/my-project/datasets/my-dataset/tables/my-table")
    ///     .await?
    ///     .with_arrow_format(schema());
    /// # Ok(()) }
    ///
    /// use google_cloud_bigquery::model::ArrowSchema;
    /// fn schema() -> ArrowSchema {
    ///   todo!("Define your table's schema...")
    /// }
    /// ```
    ///
    /// [committed stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#committed_type
    pub async fn create_committed_stream<T: Into<String>>(
        &self,
        table: T,
    ) -> Result<WriterBuilder<CommittedWriter>> {
        let table = table.into();
        validate_table(table.as_str())?;

        let client = BigQueryWrite::from_stub::<Transport>(self.inner.clone());
        let write_stream = client
            .create_write_stream()
            .set_parent(table)
            .set_write_stream(WriteStream::new().set_type(Type::Committed))
            .send()
            .await?;

        Ok(WriterBuilder::new(
            self.inner.clone(),
            self.pool.clone(),
            write_stream.name,
        ))
    }

    /// Creates a new [buffered stream] for the given table.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .create_buffered_stream("projects/my-project/datasets/my-dataset/tables/my-table")
    ///     .await?
    ///     .with_arrow_format(schema());
    /// # Ok(()) }
    ///
    /// use google_cloud_bigquery::model::ArrowSchema;
    /// fn schema() -> ArrowSchema {
    ///   todo!("Define your table's schema...")
    /// }
    /// ```
    ///
    /// [buffered stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#buffered_type
    pub async fn create_buffered_stream<T: Into<String>>(
        &self,
        table: T,
    ) -> Result<WriterBuilder<BufferedWriter>> {
        let table = table.into();
        validate_table(table.as_str())?;

        let client = BigQueryWrite::from_stub::<Transport>(self.inner.clone());
        let write_stream = client
            .create_write_stream()
            .set_parent(table)
            .set_write_stream(WriteStream::new().set_type(Type::Buffered))
            .send()
            .await?;

        Ok(WriterBuilder::new(
            self.inner.clone(),
            self.pool.clone(),
            write_stream.name,
        ))
    }

    /// Attaches to an existing write stream.
    ///
    /// # Example
    /// ```
    /// use google_cloud_bigquery::write::arrow::CommittedWriter;
    /// # use google_cloud_bigquery::client::Write;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer: CommittedWriter = client
    ///     .attach("projects/my-project/datasets/my_dataset/tables/my_table/streams/my_stream")
    ///     .await?
    ///     .with_arrow_format(schema());
    /// # Ok(())
    /// # }
    /// #
    /// # use google_cloud_bigquery::model::ArrowSchema;
    /// # fn schema() -> ArrowSchema {
    /// #   todo!("Define your table's schema...")
    /// # }
    /// ```
    pub async fn attach<U: Writer, S: Into<String>>(
        &self,
        write_stream: S,
    ) -> AttachResult<WriterBuilder<U>> {
        let write_stream = write_stream.into();
        validate_stream(write_stream.as_str())?;

        let client = BigQueryWrite::from_stub::<Transport>(self.inner.clone());
        let stream = client
            .get_write_stream()
            .set_name(&write_stream)
            .send()
            .await?;

        let stream_type = stream.r#type.clone();
        if stream_type != U::STREAM_TYPE {
            return Err(AttachError::TypeMismatch {
                expected: U::STREAM_TYPE,
                actual: stream_type,
            });
        }
        Ok(WriterBuilder::new(
            self.inner.clone(),
            self.pool.clone(),
            write_stream,
        ))
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
            .await?
            .with_arrow_format(ArrowSchema::new());
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
            .await?
            .with_proto_format(ProtoSchema::new());
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
            .await?
            .with_multiplexing(true)
            .with_arrow_format(ArrowSchema::new());
        assert!(Arc::ptr_eq(&client.pool, &multiplexed_writer.inner.pool));

        let standalone_writer = client
            .open_default_stream("projects/p/datasets/d/tables/t")
            .await?
            .with_multiplexing(false)
            .with_arrow_format(ArrowSchema::new());
        assert!(!Arc::ptr_eq(&client.pool, &standalone_writer.inner.pool));

        Ok(())
    }
}
