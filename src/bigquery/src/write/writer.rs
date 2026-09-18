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

use crate::model::append_rows_request::{ArrowData, ProtoData};
use crate::model::write_stream::Type;
use crate::model::{AppendRowsRequest, ArrowRecordBatch, ArrowSchema, ProtoRows, ProtoSchema};

/// Format marker and schema configuration for [Arrow] streams.
///
/// [Arrow]: https://arrow.apache.org/
#[derive(Clone, Debug, PartialEq)]
pub struct ArrowFormat {
    pub(crate) schema: ArrowSchema,
}

impl ArrowFormat {
    pub(crate) fn new(schema: ArrowSchema) -> Self {
        Self { schema }
    }
}

/// Format marker and schema configuration for Protobuf streams.
#[derive(Clone, Debug, PartialEq)]
pub struct ProtoFormat {
    pub(crate) schema: ProtoSchema,
}

impl ProtoFormat {
    pub(crate) fn new(schema: ProtoSchema) -> Self {
        Self { schema }
    }
}

/// Marker type representing a default write stream.
#[derive(Clone, Copy, Debug)]
pub struct DefaultStream;

/// Marker type representing a pending write stream.
#[derive(Clone, Copy, Debug)]
pub struct PendingStream;

/// Marker type representing a committed write stream.
#[derive(Clone, Copy, Debug)]
pub struct CommittedStream;

/// Marker type representing a buffered write stream.
#[derive(Clone, Copy, Debug)]
pub struct BufferedStream;

pub(crate) mod sealed {
    use super::*;

    /// Sealed trait for stream data formats.
    pub trait Format {
        fn append_request(&self, write_stream: &str, rows: Self::Row) -> AppendRowsRequest
        where
            Self: super::Format;
    }

    /// Sealed trait for stream modes that can be attached to an existing stream.
    pub trait Attachable {
        const STREAM_TYPE: Type;
    }

    impl Attachable for PendingStream {
        const STREAM_TYPE: Type = Type::Pending;
    }

    impl Attachable for CommittedStream {
        const STREAM_TYPE: Type = Type::Committed;
    }

    impl Attachable for BufferedStream {
        const STREAM_TYPE: Type = Type::Buffered;
    }
}

/// Trait implemented by stream data formats ([`ArrowFormat`], [`ProtoFormat`]).
///
/// This trait is sealed and cannot be implemented for types outside this crate.
pub trait Format: sealed::Format {
    /// The row payload type accepted by writers of this format.
    type Row;
}

impl sealed::Format for ArrowFormat {
    fn append_request(&self, write_stream: &str, rows: ArrowRecordBatch) -> AppendRowsRequest {
        AppendRowsRequest::new()
            .set_write_stream(write_stream)
            .set_arrow_rows(
                ArrowData::new()
                    .set_writer_schema(self.schema.clone())
                    .set_rows(rows),
            )
    }
}

impl Format for ArrowFormat {
    type Row = ArrowRecordBatch;
}

impl sealed::Format for ProtoFormat {
    fn append_request(&self, write_stream: &str, rows: ProtoRows) -> AppendRowsRequest {
        AppendRowsRequest::new()
            .set_write_stream(write_stream)
            .set_proto_rows(
                ProtoData::new()
                    .set_writer_schema(self.schema.clone())
                    .set_rows(rows),
            )
    }
}

impl Format for ProtoFormat {
    type Row = ProtoRows;
}

/// Marker trait for stream modes that can be attached to an existing write stream.
///
/// This trait is sealed and cannot be implemented for types outside this crate.
pub trait Writer: sealed::Attachable {}

impl Writer for PendingStream {}
impl Writer for CommittedStream {}
impl Writer for BufferedStream {}

#[cfg(test)]
mod tests {
    use super::sealed::Format as _;
    use super::*;
    use crate::write::test::*;

    #[test]
    fn arrow_request_fields() {
        let format = ArrowFormat::new(schema());
        let req = format.append_request(&write_stream(), arrow_rows(1));
        assert_eq!(req.write_stream, write_stream());
        let data = req.arrow_rows().expect("arrow rows should be set");
        let s = data.writer_schema.as_ref().expect("schema should be set");
        assert_eq!(s.serialized_schema, "test");
        let r = data.rows.as_ref().expect("rows should be set");
        assert_eq!(r.serialized_record_batch, "1");
    }

    #[test]
    fn proto_request_fields() {
        let format = ProtoFormat::new(proto_schema());
        let req = format.append_request(&write_stream(), proto_rows(1));
        assert_eq!(req.write_stream, write_stream());
        let data = req.proto_rows().expect("proto rows should be set");
        let s = data.writer_schema.as_ref().expect("schema should be set");
        assert_eq!(s.proto_descriptor.as_ref().unwrap().name, "TestMessage");
        let r = data.rows.as_ref().expect("rows should be set");
        assert_eq!(r.serialized_rows, vec![bytes::Bytes::from("1")]);
    }

    fn arrow_rows(id: i64) -> ArrowRecordBatch {
        ArrowRecordBatch::new().set_serialized_record_batch(id.to_string())
    }

    fn proto_rows(id: i64) -> ProtoRows {
        ProtoRows::new().set_serialized_rows(vec![bytes::Bytes::from(id.to_string())])
    }
}
