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

use super::pool::ConnectionPool;
use crate::ClientBuilderResult as BuilderResult;
use crate::client_builder::ClientBuilder;
use crate::google::cloud::bigquery::storage::v1::ArrowSchema;
use crate::proto_schema::ProtoSchema;
use crate::stream_writer::{ProtoStreamWriter, StreamWriter};
use crate::transport::Transport;
use crate::{Error, Result};
use gaxi::prost::ToProto;
use std::collections::HashMap;
use std::sync::Arc;

/// A client for BigQuery Storage Write API.
pub struct Client {
    inner: Arc<Transport>,
    /// Map of region -> connection pool.
    pool: HashMap<String, Arc<ConnectionPool>>,
}

impl Client {
    /// Creates a new [ClientBuilder].
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub(crate) async fn new(builder: ClientBuilder) -> BuilderResult<Self> {
        let transport = Transport::new(builder.config).await?;
        Ok(Self {
            inner: Arc::new(transport),
            pool: HashMap::new(),
        })
    }

    /// Create a [StreamWriter] for a specific stream using Arrow format.
    ///
    /// The schema must be provided and will be sent in the first AppendRows request.
    pub fn write_stream(&self, stream_name: String, schema: ArrowSchema) -> StreamWriter {
        StreamWriter::new(self.inner.clone(), stream_name, schema)
    }

    /// Create a [ProtoStreamWriter] for a specific stream using Proto format.
    ///
    /// The schema must be provided and will be sent in the first AppendRows request.
    pub fn write_stream_proto(
        &self,
        stream_name: String,
        schema: ProtoSchema,
    ) -> Result<ProtoStreamWriter> {
        let v1_schema: crate::google::cloud::bigquery::storage::v1::ProtoSchema =
            ToProto::<crate::google::cloud::bigquery::storage::v1::ProtoSchema>::to_proto(schema)
                .map_err(Error::ser)?;
        Ok(ProtoStreamWriter::new(
            self.inner.clone(),
            stream_name,
            v1_schema,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_cloud_auth::credentials::anonymous::Builder as Anonymous;

    #[tokio::test]
    async fn test_client_builder() -> anyhow::Result<()> {
        let client = Client::builder()
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        assert!(Arc::strong_count(&client.inner) >= 1);
        Ok(())
    }
}
