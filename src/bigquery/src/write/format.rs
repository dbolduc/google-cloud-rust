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
use crate::model::{AppendRowsRequest, ArrowRecordBatch, ArrowSchema, ProtoRows, ProtoSchema};

/// Schema configuration and data format for [Arrow] streams.
///
/// [Arrow]: https://arrow.apache.org/
#[derive(Clone, Debug, PartialEq)]
pub struct Arrow {
    pub(crate) schema: ArrowSchema,
}

impl Arrow {
    pub(crate) fn new(schema: ArrowSchema) -> Self {
        Self { schema }
    }
}

/// Format marker and schema configuration for Protobuf streams.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Proto {
    pub(crate) schema: ProtoSchema,
}

impl Proto {
    #[allow(dead_code)]
    pub(crate) fn new(schema: ProtoSchema) -> Self {
        Self { schema }
    }
}

pub(crate) mod sealed {
    use crate::model::AppendRowsRequest;

    /// Sealed trait for stream data formats.
    pub trait DataFormat {
        fn append_request(&self, write_stream: &str, rows: Self::Row) -> AppendRowsRequest
        where
            Self: super::DataFormat;
    }
}

/// Trait implemented by stream data formats ([`Arrow`]).
///
/// This trait is sealed and cannot be implemented for types outside this crate.
pub trait DataFormat: sealed::DataFormat {
    /// The row payload type accepted by writers of this format.
    type Row;
}

impl sealed::DataFormat for Arrow {
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

impl DataFormat for Arrow {
    type Row = ArrowRecordBatch;
}

impl sealed::DataFormat for Proto {
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

impl DataFormat for Proto {
    type Row = ProtoRows;
}

#[cfg(test)]
mod tests {
    use super::sealed::DataFormat as _;
    use super::*;
    use crate::write::test::*;

    #[test]
    fn arrow_request_fields() {
        let format = Arrow::new(schema());
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
        let format = Proto::new(proto_schema());
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
