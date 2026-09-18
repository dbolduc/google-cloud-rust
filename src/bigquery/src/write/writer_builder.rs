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

use super::error::{WriterBuilderError, WriterBuilderResult};
use super::format::{Arrow, Proto};
use super::generated::gapic_storage::client::BigQueryWrite;
use super::pool::{StreamPool, StreamPoolOptions};
use super::transport::Transport;
use super::validate::{validate_stream, validate_table};
use super::{CreatedStreamType, DefaultStream, StreamType};
use crate::model::{ArrowSchema, ProtoSchema, WriteStream};
use std::marker::PhantomData;
use std::sync::Arc;

#[derive(Clone, Debug)]
enum StreamAction {
    OpenDefault { table: String },
    Create { table: String },
    Attach { write_stream: String },
}

/// A builder for configuring and constructing a stream writer.
///
/// Created via [`Write::open_default_stream`][crate::client::Write::open_default_stream],
/// [`Write::create_stream`][crate::client::Write::create_stream], or
/// [`Write::attach_to_stream`][crate::client::Write::attach_to_stream].
#[derive(Clone, Debug)]
pub struct WriterBuilder<S> {
    pub(crate) inner: Arc<Transport>,
    pool: Arc<StreamPool>,
    action: StreamAction,
    multiplexing: bool,
    _stream_type: PhantomData<S>,
}

impl WriterBuilder<DefaultStream> {
    pub(crate) fn open_default(
        inner: Arc<Transport>,
        pool: Arc<StreamPool>,
        table: String,
    ) -> Self {
        Self {
            inner,
            pool,
            action: StreamAction::OpenDefault { table },
            multiplexing: false,
            _stream_type: PhantomData,
        }
    }

    /// Sets whether this writer shares the client's multiplexed stream pool.
    ///
    /// When `false` (the default), this writer creates its own isolated stream
    /// with a pool size limit of 1. When `true`, it shares the `Write` client's
    /// stream pool with other multiplexed writers.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # use google_cloud_bigquery::model::ArrowSchema;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .open_default_stream("projects/my-project/datasets/my_dataset/tables/my_table")
    ///     .with_multiplexing(true)
    ///     .build_arrow(ArrowSchema::new())
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_multiplexing(mut self, enabled: bool) -> Self {
        self.multiplexing = enabled;
        self
    }
}

impl<S: CreatedStreamType> WriterBuilder<S> {
    pub(crate) fn create(inner: Arc<Transport>, pool: Arc<StreamPool>, table: String) -> Self {
        Self {
            inner,
            pool,
            action: StreamAction::Create { table },
            multiplexing: false,
            _stream_type: PhantomData,
        }
    }

    pub(crate) fn attach(
        inner: Arc<Transport>,
        pool: Arc<StreamPool>,
        write_stream: String,
    ) -> Self {
        Self {
            inner,
            pool,
            action: StreamAction::Attach { write_stream },
            multiplexing: false,
            _stream_type: PhantomData,
        }
    }
}

impl<S: StreamType> WriterBuilder<S> {
    pub(crate) fn stream_pool(&self) -> Arc<StreamPool> {
        if self.multiplexing {
            self.pool.clone()
        } else {
            let options = StreamPoolOptions {
                max_streams: 1,
                ..Default::default()
            };
            Arc::new(StreamPool::new(self.inner.clone(), options))
        }
    }

    async fn build_with_format<F>(self, format: F) -> WriterBuilderResult<S::Writer<F>> {
        let write_stream = match &self.action {
            StreamAction::OpenDefault { table } => {
                validate_table(table)?;
                format!("{table}/streams/_default")
            }
            StreamAction::Create { table } => {
                validate_table(table)?;
                let stream_type =
                    S::STREAM_TYPE.expect("Create only used with CreatedStreamType modes");
                let client = BigQueryWrite::from_stub::<Transport>(self.inner.clone());
                let ws = client
                    .create_write_stream()
                    .set_parent(table)
                    .set_write_stream(WriteStream::new().set_type(stream_type))
                    .send()
                    .await?;
                ws.name
            }
            StreamAction::Attach { write_stream } => {
                validate_stream(write_stream)?;
                let expected =
                    S::STREAM_TYPE.expect("Attach only used with CreatedStreamType modes");
                let client = BigQueryWrite::from_stub::<Transport>(self.inner.clone());
                let stream = client
                    .get_write_stream()
                    .set_name(write_stream)
                    .send()
                    .await?;
                let actual = stream.r#type.clone();
                if actual != expected {
                    return Err(WriterBuilderError::TypeMismatch { expected, actual });
                }
                stream.name
            }
        };

        Ok(S::construct(self, write_stream, format))
    }

    /// Consumes the builder and creates a writer using [Arrow] as the data format.
    ///
    /// Returns the writer corresponding to the stream type `S`:
    /// - [`DefaultStream`] -> [`DefaultWriter<Arrow>`][crate::write::DefaultWriter]
    /// - [`PendingStream`][crate::write::PendingStream] -> [`PendingWriter<Arrow>`][crate::write::PendingWriter]
    /// - [`CommittedStream`][crate::write::CommittedStream] -> [`CommittedWriter<Arrow>`][crate::write::CommittedWriter]
    /// - [`BufferedStream`][crate::write::BufferedStream] -> [`BufferedWriter<Arrow>`][crate::write::BufferedWriter]
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # use google_cloud_bigquery::model::ArrowSchema;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .open_default_stream("projects/my-project/datasets/my_dataset/tables/my_table")
    ///     .build_arrow(ArrowSchema::new())
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [Arrow]: https://arrow.apache.org/
    pub async fn build_arrow(self, schema: ArrowSchema) -> WriterBuilderResult<S::Writer<Arrow>> {
        self.build_with_format(Arrow::new(schema)).await
    }

    /// Consumes the builder and creates a writer using Protobuf as the data format.
    #[allow(dead_code)]
    pub(crate) async fn build_proto(
        self,
        schema: ProtoSchema,
    ) -> WriterBuilderResult<S::Writer<Proto>> {
        self.build_with_format(Proto::new(schema)).await
    }
}

#[cfg(test)]
mod tests {
    use crate::client::Write;
    use crate::model::write_stream::Type;
    use crate::write::error::WriterBuilderError;
    use crate::write::test::*;
    use crate::write::{BufferedStream, CommittedStream, PendingStream};
    use bigquery_grpc_mock::google::cloud::bigquery::storage::v1::WriteStream as MockWriteStream;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
    use test_case::test_case;
    use tokio::task::JoinHandle;

    async fn test_client(endpoint: String) -> anyhow::Result<Write> {
        Ok(Write::builder()
            .with_endpoint(endpoint)
            .with_credentials(Anonymous::new().build())
            .build()
            .await?)
    }

    #[tokio::test]
    async fn pending_success() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        mock.expect_create_write_stream().return_once(|req| {
            let req = req.into_inner();
            assert_eq!(req.parent, "projects/p/datasets/d/tables/t");
            let ws = req.write_stream.expect("write_stream populated");
            assert_eq!(Type::from(ws.r#type), Type::Pending);
            Ok(gaxi::grpc::tonic::Response::new(MockWriteStream {
                name: "projects/p/datasets/d/tables/t/streams/s".to_string(),
                ..Default::default()
            }))
        });
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let client = test_client(endpoint).await?;
        let writer = client
            .create_stream::<PendingStream>("projects/p/datasets/d/tables/t")
            .build_arrow(schema())
            .await?;
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, schema());
        Ok(())
    }

    #[tokio::test]
    async fn committed_success() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        mock.expect_create_write_stream().return_once(|req| {
            let req = req.into_inner();
            assert_eq!(req.parent, "projects/p/datasets/d/tables/t");
            let ws = req.write_stream.expect("write_stream populated");
            assert_eq!(Type::from(ws.r#type), Type::Committed);
            Ok(gaxi::grpc::tonic::Response::new(MockWriteStream {
                name: "projects/p/datasets/d/tables/t/streams/s".to_string(),
                ..Default::default()
            }))
        });
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let client = test_client(endpoint).await?;
        let writer = client
            .create_stream::<CommittedStream>("projects/p/datasets/d/tables/t")
            .build_arrow(schema())
            .await?;
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, schema());
        Ok(())
    }

    #[tokio::test]
    async fn buffered_success() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        mock.expect_create_write_stream().return_once(|req| {
            let req = req.into_inner();
            assert_eq!(req.parent, "projects/p/datasets/d/tables/t");
            let ws = req.write_stream.expect("write_stream populated");
            assert_eq!(Type::from(ws.r#type), Type::Buffered);
            Ok(gaxi::grpc::tonic::Response::new(MockWriteStream {
                name: "projects/p/datasets/d/tables/t/streams/s".to_string(),
                ..Default::default()
            }))
        });
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let client = test_client(endpoint).await?;
        let writer = client
            .create_stream::<BufferedStream>("projects/p/datasets/d/tables/t")
            .build_arrow(schema())
            .await?;
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, schema());
        Ok(())
    }

    #[test_case("projects/p")]
    #[test_case("projects/p/tables/t")]
    #[test_case("projects/p/datasets/d/tables/")]
    #[tokio::test]
    async fn create_stream_bad_table_format(table: &str) -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let err = client
            .create_stream::<PendingStream>(table)
            .build_arrow(schema())
            .await
            .expect_err("should fail locally on bad format");
        assert!(matches!(err, WriterBuilderError::Rpc { source: e } if e.is_binding()));
        Ok(())
    }

    #[tokio::test]
    async fn default() -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let writer = client
            .open_default_stream("projects/p/datasets/d/tables/t")
            .build_arrow(schema())
            .await?;
        assert_eq!(
            writer.write_stream,
            "projects/p/datasets/d/tables/t/streams/_default"
        );
        assert_eq!(writer.format.schema, schema());
        Ok(())
    }

    #[test_case("projects/p")]
    #[test_case("projects/p/tables/t")]
    #[test_case("projects/p/datasets/d/tables/")]
    #[test_case("projects/p/instances/i/tables/t")]
    #[test_case("projects/p/datasets/d/tables/t/streams")]
    #[test_case("projects/p/datasets/d/tables/t/streams/_default")]
    #[tokio::test]
    async fn bad_table_format(table: &str) -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let err = client
            .open_default_stream(table)
            .build_arrow(schema())
            .await
            .expect_err("should fail locally on bad format");
        assert!(matches!(err, WriterBuilderError::Rpc { source: e } if e.is_binding()));
        Ok(())
    }

    async fn attach_mock(stream_type: Type) -> anyhow::Result<(Write, JoinHandle<()>)> {
        let mut mock = MockBigQueryWrite::new();
        mock.expect_get_write_stream().return_once(move |req| {
            let req = req.into_inner();
            assert_eq!(req.name, "projects/p/datasets/d/tables/t/streams/s");
            Ok(gaxi::grpc::tonic::Response::new(MockWriteStream {
                name: "projects/p/datasets/d/tables/t/streams/s".to_string(),
                r#type: stream_type.value().expect("known enum value"),
                ..Default::default()
            }))
        });
        let (endpoint, server) = start("0.0.0.0:0", mock).await?;
        let client = test_client(endpoint).await?;
        Ok((client, server))
    }

    #[tokio::test]
    async fn attach_committed_success() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Committed).await?;
        let writer = client
            .attach_to_stream::<CommittedStream>("projects/p/datasets/d/tables/t/streams/s")
            .build_arrow(schema())
            .await?;
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, schema());
        Ok(())
    }

    #[tokio::test]
    async fn attach_pending_success() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Pending).await?;
        let writer = client
            .attach_to_stream::<PendingStream>("projects/p/datasets/d/tables/t/streams/s")
            .build_arrow(schema())
            .await?;
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, schema());
        Ok(())
    }

    #[tokio::test]
    async fn attach_buffered_success() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Buffered).await?;
        let writer = client
            .attach_to_stream::<BufferedStream>("projects/p/datasets/d/tables/t/streams/s")
            .build_arrow(schema())
            .await?;
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, schema());
        Ok(())
    }

    #[test_case("projects/p")]
    #[test_case("projects/p/tables/t")]
    #[test_case("projects/p/datasets/d/tables/t")]
    #[test_case("projects/p/datasets/d/tables/t/streams/")]
    #[tokio::test]
    async fn attach_bad_stream_format(stream: &str) -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let err = client
            .attach_to_stream::<CommittedStream>(stream)
            .build_arrow(schema())
            .await
            .expect_err("should fail locally on bad format");
        assert!(matches!(err, WriterBuilderError::Rpc { source: e } if e.is_binding()));
        Ok(())
    }

    #[tokio::test]
    async fn attach_stream_type_mismatch() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Buffered).await?;
        let err = client
            .attach_to_stream::<CommittedStream>("projects/p/datasets/d/tables/t/streams/s")
            .build_arrow(schema())
            .await
            .expect_err("should return type mismatch error");
        assert!(matches!(err, WriterBuilderError::TypeMismatch { .. }));
        assert!(err.to_string().contains("stream type mismatch: requested"));
        Ok(())
    }

    #[tokio::test]
    async fn build_proto() -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let writer = client
            .open_default_stream("projects/p/datasets/d/tables/t")
            .build_proto(proto_schema())
            .await?;
        assert_eq!(
            writer.write_stream,
            "projects/p/datasets/d/tables/t/streams/_default"
        );
        assert_eq!(writer.format.schema, proto_schema());
        Ok(())
    }
}
