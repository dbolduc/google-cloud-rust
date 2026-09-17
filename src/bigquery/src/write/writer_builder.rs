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

use super::pool::{StreamPool, StreamPoolOptions};
use super::transport::Transport;
use super::writer::{ArrowFormat, ProtoFormat};
use super::{
    BufferedStream, BufferedWriter, CommittedStream, CommittedWriter, DefaultStream, DefaultWriter,
    PendingStream, PendingWriter,
};
use crate::model::{ArrowSchema, ProtoSchema};
use std::marker::PhantomData;
use std::sync::Arc;

/// A builder to configure and create a stream writer.
#[derive(Clone, Debug)]
pub struct WriterBuilder<Mode> {
    pub(crate) inner: Arc<Transport>,
    pub(crate) pool: Arc<StreamPool>,
    pub(crate) write_stream: String,
    pub(crate) multiplexing: bool,
    _marker: PhantomData<Mode>,
}

impl<Mode> WriterBuilder<Mode> {
    pub(crate) fn new(inner: Arc<Transport>, pool: Arc<StreamPool>, write_stream: String) -> Self {
        Self {
            inner,
            pool,
            write_stream,
            multiplexing: false,
            _marker: PhantomData,
        }
    }

    /// Returns the full resource name of the underlying write stream.
    pub fn write_stream(&self) -> &str {
        &self.write_stream
    }

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
}

impl WriterBuilder<DefaultStream> {
    /// Enable multiplexing.
    ///
    /// Set this option to use the client's shared stream pool.
    ///
    /// This option only applies to the default stream.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # use google_cloud_bigquery::model::ArrowSchema;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .open_default_stream("projects/my-project/datasets/my_dataset/tables/my_table")
    ///     .await?
    ///     .with_multiplexing(true)
    ///     .build_arrow(ArrowSchema::new());
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_multiplexing(mut self, enable: bool) -> Self {
        self.multiplexing = enable;
        self
    }

    /// Consumes the builder and creates a writer using [Arrow] as the data format.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_bigquery::client::Write;
    /// # use google_cloud_bigquery::model::ArrowSchema;
    /// # async fn sample(client: Write) -> anyhow::Result<()> {
    /// let writer = client
    ///     .open_default_stream("projects/my-project/datasets/my-dataset/tables/my-table")
    ///     .await?
    ///     .build_arrow(ArrowSchema::new());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [Arrow]: https://arrow.apache.org/
    pub fn build_arrow(self, schema: ArrowSchema) -> DefaultWriter<ArrowFormat> {
        DefaultWriter::new(
            self.stream_pool(),
            self.write_stream,
            ArrowFormat::new(schema),
        )
    }

    /// Consumes the builder and creates a writer using Protobuf as the data format.
    #[allow(dead_code)]
    pub(crate) fn build_proto(self, schema: ProtoSchema) -> DefaultWriter<ProtoFormat> {
        DefaultWriter::new(
            self.stream_pool(),
            self.write_stream,
            ProtoFormat::new(schema),
        )
    }
}

impl WriterBuilder<PendingStream> {
    /// Consumes the builder and creates a writer using [Arrow] as the data format.
    ///
    /// [Arrow]: https://arrow.apache.org/
    pub fn build_arrow(self, schema: ArrowSchema) -> PendingWriter<ArrowFormat> {
        PendingWriter::new(self.inner, self.write_stream, ArrowFormat::new(schema))
    }

    /// Consumes the builder and creates a writer using Protobuf as the data format.
    #[allow(dead_code)]
    pub(crate) fn build_proto(self, schema: ProtoSchema) -> PendingWriter<ProtoFormat> {
        PendingWriter::new(self.inner, self.write_stream, ProtoFormat::new(schema))
    }
}

impl WriterBuilder<CommittedStream> {
    /// Consumes the builder and creates a writer using [Arrow] as the data format.
    ///
    /// [Arrow]: https://arrow.apache.org/
    pub fn build_arrow(self, schema: ArrowSchema) -> CommittedWriter<ArrowFormat> {
        CommittedWriter::new(self.inner, self.write_stream, ArrowFormat::new(schema))
    }

    /// Consumes the builder and creates a writer using Protobuf as the data format.
    #[allow(dead_code)]
    pub(crate) fn build_proto(self, schema: ProtoSchema) -> CommittedWriter<ProtoFormat> {
        CommittedWriter::new(self.inner, self.write_stream, ProtoFormat::new(schema))
    }
}

impl WriterBuilder<BufferedStream> {
    /// Consumes the builder and creates a writer using [Arrow] as the data format.
    ///
    /// [Arrow]: https://arrow.apache.org/
    pub fn build_arrow(self, schema: ArrowSchema) -> BufferedWriter<ArrowFormat> {
        BufferedWriter::new(self.inner, self.write_stream, ArrowFormat::new(schema))
    }

    /// Consumes the builder and creates a writer using Protobuf as the data format.
    #[allow(dead_code)]
    pub(crate) fn build_proto(self, schema: ProtoSchema) -> BufferedWriter<ProtoFormat> {
        BufferedWriter::new(self.inner, self.write_stream, ProtoFormat::new(schema))
    }
}

#[cfg(test)]
mod arrow_tests {
    use crate::client::Write;
    use crate::model::write_stream::Type;
    use crate::write::error::AttachError;
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
            .create_pending_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_arrow(schema());
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
    async fn pending_bad_table_format(table: &str) -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let err = client
            .create_pending_stream(table)
            .await
            .expect_err("should fail locally on bad format");
        assert!(err.is_binding(), "{err:?}");
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
            .create_committed_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_arrow(schema());
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
    async fn committed_bad_table_format(table: &str) -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let err = client
            .create_committed_stream(table)
            .await
            .expect_err("should fail locally on bad format");
        assert!(err.is_binding(), "{err:?}");
        Ok(())
    }

    #[tokio::test]
    async fn default() -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let writer = client
            .open_default_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_arrow(schema());
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
            .await
            .expect_err("should fail locally on bad format");
        assert!(err.is_binding(), "{err:?}");
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
            .create_buffered_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_arrow(schema());
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
    async fn buffered_bad_table_format(table: &str) -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let err = client
            .create_buffered_stream(table)
            .await
            .expect_err("should fail locally on bad format");
        assert!(err.is_binding(), "{err:?}");
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
            .attach::<CommittedStream>("projects/p/datasets/d/tables/t/streams/s")
            .await?
            .build_arrow(schema());
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
            .attach::<PendingStream>("projects/p/datasets/d/tables/t/streams/s")
            .await?
            .build_arrow(schema());
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
            .attach::<BufferedStream>("projects/p/datasets/d/tables/t/streams/s")
            .await?
            .build_arrow(schema());
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
            .attach::<CommittedStream>(stream)
            .await
            .expect_err("should fail locally on bad format");
        assert!(matches!(err, AttachError::Rpc { source: e } if e.is_binding()));
        Ok(())
    }

    #[tokio::test]
    async fn attach_stream_type_mismatch() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Buffered).await?;
        let err = client
            .attach::<CommittedStream>("projects/p/datasets/d/tables/t/streams/s")
            .await
            .expect_err("should return type mismatch error");
        assert!(matches!(err, AttachError::TypeMismatch { .. }));
        assert!(err.to_string().contains("stream type mismatch: requested"));
        Ok(())
    }
}

#[cfg(test)]
mod proto_tests {
    use crate::client::Write;
    use crate::model::write_stream::Type;
    use crate::write::error::AttachError;
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
            .create_pending_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_proto(proto_schema());
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, proto_schema());
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
            .create_committed_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_proto(proto_schema());
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, proto_schema());
        Ok(())
    }

    #[tokio::test]
    async fn default() -> anyhow::Result<()> {
        let client = test_client("http://ignored:1".to_string()).await?;
        let writer = client
            .open_default_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_proto(proto_schema());
        assert_eq!(
            writer.write_stream,
            "projects/p/datasets/d/tables/t/streams/_default"
        );
        assert_eq!(writer.format.schema, proto_schema());
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
            .create_buffered_stream("projects/p/datasets/d/tables/t")
            .await?
            .build_proto(proto_schema());
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, proto_schema());
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
            .attach::<CommittedStream>("projects/p/datasets/d/tables/t/streams/s")
            .await?
            .build_proto(proto_schema());
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, proto_schema());
        Ok(())
    }

    #[tokio::test]
    async fn attach_pending_success() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Pending).await?;
        let writer = client
            .attach::<PendingStream>("projects/p/datasets/d/tables/t/streams/s")
            .await?
            .build_proto(proto_schema());
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, proto_schema());
        Ok(())
    }

    #[tokio::test]
    async fn attach_buffered_success() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Buffered).await?;
        let writer = client
            .attach::<BufferedStream>("projects/p/datasets/d/tables/t/streams/s")
            .await?
            .build_proto(proto_schema());
        assert_eq!(
            writer.inner.write_stream,
            "projects/p/datasets/d/tables/t/streams/s"
        );
        assert_eq!(writer.inner.format.schema, proto_schema());
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
            .attach::<CommittedStream>(stream)
            .await
            .expect_err("should fail locally on bad format");
        assert!(matches!(err, AttachError::Rpc { source: e } if e.is_binding()));
        Ok(())
    }

    #[tokio::test]
    async fn attach_stream_type_mismatch() -> anyhow::Result<()> {
        let (client, _server) = attach_mock(Type::Buffered).await?;
        let err = client
            .attach::<CommittedStream>("projects/p/datasets/d/tables/t/streams/s")
            .await
            .expect_err("should return type mismatch error");
        assert!(matches!(err, AttachError::TypeMismatch { .. }));
        assert!(err.to_string().contains("stream type mismatch: requested"));
        Ok(())
    }
}
