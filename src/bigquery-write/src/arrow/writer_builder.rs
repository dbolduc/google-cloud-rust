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

use crate::arrow::DefaultWriter;
use crate::model::ArrowSchema;
use crate::pool::{ConnectionPool, StreamPool};
use crate::transport::Transport;
use crate::{Error, Result};
use gaxi::path_parameter::{PathMismatchBuilder, try_match};
use gaxi::routing_parameter::Segment;
use google_cloud_gax::error::binding::BindingError;
use std::sync::Arc;

/// A builder to create a stream writer
#[derive(Clone, Debug)]
pub struct WriterBuilder {
    inner: Arc<Transport>,
    pool: Arc<StreamPool>,
    schema: ArrowSchema,
    multiplexing: bool,
}

impl WriterBuilder {
    pub(crate) fn new(inner: Arc<Transport>, pool: Arc<StreamPool>, schema: ArrowSchema) -> Self {
        Self {
            inner,
            pool,
            schema,
            multiplexing: true,
        }
    }

    /// Disable multiplexed stream pooling for this writer (each writer gets its own exclusive connection).
    pub fn with_multiplexing(mut self, enabled: bool) -> Self {
        self.multiplexing = enabled;
        self
    }

    /// Create a writer for the [default stream] for the given table.
    ///
    /// [default stream]: https://docs.cloud.google.com/bigquery/docs/write-api#default_stream
    pub fn default<T: Into<String>>(self, table: T) -> Result<DefaultWriter> {
        let table = table.into();
        validate_table(table.as_str())?;
        let mut write_stream = table;
        write_stream.push_str("/streams/_default");

        let pool = if self.multiplexing {
            ConnectionPool::Multiplexed(self.pool)
        } else {
            ConnectionPool::Exclusive(crate::pool::ExclusivePool::new(self.inner.clone()))
        };

        Ok(DefaultWriter::new(
            self.inner,
            pool,
            write_stream,
            self.schema,
        ))
    }
}

fn validate_table(table: &str) -> Result<()> {
    let segments = &[
        Segment::Literal("projects/"),
        Segment::SingleWildcard,
        Segment::Literal("/datasets/"),
        Segment::SingleWildcard,
        Segment::Literal("/tables/"),
        Segment::SingleWildcard,
    ];
    try_match(Some(table), segments)
        .ok_or_else(|| {
            let builder = PathMismatchBuilder::default().maybe_add(
                Some(table),
                segments,
                "table",
                "projects/*/datasets/*/tables/*",
            );
            Error::binding(BindingError {
                paths: vec![builder.build()],
            })
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::StreamPool;
    use crate::transport::tests::test_transport;
    use test_case::test_case;

    fn test_builder(
        transport: Arc<Transport>,
        pool: Arc<StreamPool>,
        schema: ArrowSchema,
    ) -> WriterBuilder {
        WriterBuilder::new(transport, pool, schema)
    }

    #[tokio::test]
    async fn default() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1".to_string()).await?);
        let pool = Arc::new(StreamPool::new(transport.clone()));
        let schema = ArrowSchema::new().set_serialized_schema("test");
        let builder = test_builder(transport, pool, schema.clone());
        let writer = builder.default("projects/p/datasets/d/tables/t")?;
        assert_eq!(
            writer.write_stream,
            "projects/p/datasets/d/tables/t/streams/_default"
        );
        assert_eq!(writer.schema, schema);
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
        let transport = Arc::new(test_transport("http://ignored:1".to_string()).await?);
        let pool = Arc::new(StreamPool::new(transport.clone()));
        let schema = ArrowSchema::new().set_serialized_schema("test");
        let builder = test_builder(transport, pool, schema.clone());
        let err = builder
            .default(table)
            .expect_err("should fail locally on bad format");
        assert!(err.is_binding(), "{err:?}");
        Ok(())
    }
}
