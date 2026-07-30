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

use super::{DefaultWriter, PendingWriter};
use crate::Result;
use crate::model::CreateWriteStreamRequest;
use crate::model::{AppendRowsRequest, ArrowRecordBatch, ArrowSchema};
use crate::transport::Transport;
use std::sync::Arc;

pub struct WriterBuilder {
    inner: Arc<Transport>,
    schema: ArrowSchema,
}

impl WriterBuilder {
    pub(crate) fn new(inner: Arc<Transport>, schema: ArrowSchema) -> Self {
        Self { inner, schema }
    }

    // TODO : could return an error in case of bad table format? Like a binding error?
    /// Create a writer for the default stream for the given table.
    pub fn default<T: Into<String>>(self, table: T) -> DefaultWriter {
        let mut write_stream = table.into();
        write_stream.push_str("/streams/_default");
        DefaultWriter::new(self.inner, write_stream, self.schema)
    }

    /// Creates a pending writer for the given table.
    ///
    /// The client library creates a `WriteStream` with type `PENDING` on behalf of the application.
    pub async fn pending<T: Into<String>>(self, table: T) -> Result<PendingWriter> {
        pub use crate::generated::gapic_storage::client::BigQueryWrite;
        pub use crate::model::WriteStream;
        pub use crate::model::write_stream::Type;
        let client = BigQueryWrite::from_stub::<Transport>(self.inner.clone());
        let write_stream = client
            .create_write_stream()
            .set_parent(table)
            .set_write_stream(WriteStream::new().set_type(Type::Pending))
            .send()
            .await?;
        Ok(PendingWriter::new(
            self.inner,
            write_stream.name,
            self.schema,
        ))
    }

    /*
    /// Creates a committed writer for the given table.
    ///
    /// The client library creates a `WriteStream` with type `COMMITTED` on behalf of the application.
    pub async fn committed<T: Into<String>>(self, table: T) -> Result<CommittedWriter> {
        todo!()
    }

    /// Creates a buffered writer for the given table.
    ///
    /// The client library creates a `WriteStream` with type `BUFFERED` on behalf of the application.
    pub async fn buffered<T: Into<String>>(self, table: T) -> Result<BufferedWriter> {
        todo!()
    }
    */
}
