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

use super::pool::{ConnectionPool, StreamHandle};
use crate::ClientBuilderResult as BuilderResult;
use crate::client_builder::ClientBuilder;
use crate::model::ArrowSchema;
use crate::proto_schema::ProtoSchema;
use crate::runner::StreamTask;
use crate::stream_writer::{ArrowStreamWriter, ProtoStreamWriter};
use crate::transport::Transport;
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

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
        let mut pool = HashMap::new();
        pool.insert("default".to_string(), Arc::new(ConnectionPool::new()));
        Ok(Self {
            inner: Arc::new(transport),
            pool,
        })
    }

    async fn get_default_handle(&self) -> Result<StreamHandle> {
        let pool = self.pool.get("default").ok_or_else(|| {
            Error::service(
                google_cloud_gax::error::rpc::Status::default()
                    .set_code(google_cloud_gax::error::rpc::Code::Internal)
                    .set_message("default pool not found"),
            )
        })?;

        pool.get_or_init_default(|| async {
            let (tx, rx) = mpsc::channel(100);
            let mut runner = StreamTask::new(self.inner.clone());
            tokio::spawn(async move {
                let _ = runner.run(rx).await;
            });
            Ok(StreamHandle { tx })
        })
        .await
    }

    /// Create a [ArrowStreamWriter] for a specific stream using Arrow format.
    ///
    /// The schema must be provided and will be sent in the first AppendRows request.
    pub async fn write_stream_arrow(
        &self,
        stream_name: String,
        schema: ArrowSchema,
    ) -> Result<ArrowStreamWriter> {
        let handle = self.get_default_handle().await?;
        Ok(ArrowStreamWriter::new(handle, stream_name, schema))
    }

    /// Create a [ProtoStreamWriter] for a specific stream using Proto format.
    ///
    /// The schema must be provided and will be sent in the first AppendRows request.
    pub async fn write_stream_proto(
        &self,
        stream_name: String,
        schema: ProtoSchema,
    ) -> Result<ProtoStreamWriter> {
        let handle = self.get_default_handle().await?;
        Ok(ProtoStreamWriter::new(handle, stream_name, schema))
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
