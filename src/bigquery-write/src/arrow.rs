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

use crate::Result;
use crate::model::ArrowSchema;
use crate::append_result::AppendResult;

pub struct WriterBuilder {
    schema: ArrowSchema,
}

impl WriterBuilder {
    // TODO : could return an error in case of bad table format? Like a binding error?
    /// Create a writer for the default stream for the given table.
    pub async fn default<T: Into<String>>(self, table: T) -> DefaultWriter {
        let write_stream = table.into().push_str("/streams/_default");
        DefaultWriter::new(write_stream, schema)
    }

    /*
    /// Creates a pending writer for the given table.
    ///
    /// The client library creates a `WriteStream` with type `PENDING` on behalf of the application.
    pub async fn pending<T: Into<String>>(self, table: T) -> Result<PendingWriter> {
        todo!()
    }

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

    pub(crate) fn new(schema: ArrowSchema) -> Self {
        Self {
            schema
        }
    }
}

pub struct DefaultWriter {
    write_stream: String,
    schema: ArrowSchema,
}

impl DefaultWriter {
    fn new(write_stream: String, schema: ArrowSchema) -> Self {
        Self {
            write_stream,
            schema,
        }
    }

    pub fn append(&self) -> AppendBuilder {
        AppendBuilder::new(self);
    }
}

pub struct AppendBuilder {
    inner: Arc<Transport>,
    // TODO : send optimization. We will want refs and won't want to store this in a req.
    req: AppendRowsRequest,
}

impl AppendBuilder {
    fn new(inner: Arc<Transport>, write_stream: String, schema: ArrowSchema) -> Self {
        Self {
            inner,
            req: AppendRowsRequest::new().set_write_stream(write_stream).set_arrow_rows(ArrowData::new().set_writer_schema(schema)),
        }
    }

    pub async fn send(self) -> crate::error::AppendResult<AppendResult> {
        todo!("unimplemented")
    }
}
