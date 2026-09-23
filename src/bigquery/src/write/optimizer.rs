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

use crate::google::cloud::bigquery::storage::v1::{
    AppendRowsRequest, ArrowSchema, ProtoSchema, append_rows_request::Rows,
};

#[derive(Clone, Debug, PartialEq)]
enum WriterSchema {
    Arrow(ArrowSchema),
    Proto(Box<ProtoSchema>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum WriterSchemaRef<'a> {
    Arrow(&'a ArrowSchema),
    Proto(&'a ProtoSchema),
}

impl WriterSchema {
    fn as_ref(&self) -> WriterSchemaRef<'_> {
        match self {
            Self::Arrow(s) => WriterSchemaRef::Arrow(s),
            Self::Proto(s) => WriterSchemaRef::Proto(s),
        }
    }
}

fn extract_schema(req: &AppendRowsRequest) -> Option<WriterSchema> {
    match req.rows.as_ref()? {
        Rows::ArrowRows(data) => data.writer_schema.clone().map(WriterSchema::Arrow),
        Rows::ProtoRows(data) => data
            .writer_schema
            .clone()
            .map(Box::new)
            .map(WriterSchema::Proto),
    }
}

fn schema_ref(req: &AppendRowsRequest) -> Option<WriterSchemaRef<'_>> {
    match req.rows.as_ref()? {
        Rows::ArrowRows(data) => data.writer_schema.as_ref().map(WriterSchemaRef::Arrow),
        Rows::ProtoRows(data) => data.writer_schema.as_ref().map(WriterSchemaRef::Proto),
    }
}

fn clear_schema(req: &mut AppendRowsRequest) {
    match req.rows.as_mut() {
        Some(Rows::ArrowRows(data)) => data.writer_schema = None,
        Some(Rows::ProtoRows(data)) => data.writer_schema = None,
        None => {}
    }
}

/// Optimizes outgoing `AppendRowsRequest` messages on a single `AppendRows`
/// stream connection by redacting redundant `write_stream` and `writer_schema`
/// fields.
///
/// Per the `AppendRowsRequest` specification:
/// - The initial request on a stream must include `trace_id`, `write_stream`,
///   and `writer_schema`.
/// - Subsequent requests to the same `write_stream` and `writer_schema` omit
///   both `write_stream` and `writer_schema`, unless the stream has previously
///   switched `write_stream`s or changed `writer_schema`.
/// - Once a stream switches `write_stream` or changes `writer_schema`, that
///   request must include both `write_stream` and `writer_schema`, and all
///   subsequent requests on the stream must continue to populate `write_stream`
///   (while still omitting `writer_schema` when consecutive requests share the
///   same `write_stream` and `writer_schema`).
#[derive(Debug)]
pub(super) struct SendOptimizer {
    prev_write_stream: String,
    prev_schema: Option<WriterSchema>,
    keep_write_stream: bool,
}

impl SendOptimizer {
    pub(super) fn new(initial_req: &AppendRowsRequest) -> Self {
        Self {
            prev_write_stream: initial_req.write_stream.clone(),
            prev_schema: extract_schema(initial_req),
            keep_write_stream: false,
        }
    }

    pub(super) fn optimize(&mut self, req: &mut AppendRowsRequest) {
        if req.write_stream == self.prev_write_stream
            && schema_ref(req) == self.prev_schema.as_ref().map(WriterSchema::as_ref)
        {
            if !self.keep_write_stream {
                req.write_stream.clear();
            }
            clear_schema(req);
        } else {
            self.keep_write_stream = true;
            self.prev_write_stream = req.write_stream.clone();
            self.prev_schema = extract_schema(req);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::cloud::bigquery::storage::v1::{
        ArrowRecordBatch, ProtoRows,
        append_rows_request::{ArrowData, ProtoData},
    };

    fn arrow_req(stream: &str, schema: &'static [u8], batch: &'static [u8]) -> AppendRowsRequest {
        AppendRowsRequest {
            write_stream: stream.to_string(),
            rows: Some(Rows::ArrowRows(ArrowData {
                writer_schema: Some(ArrowSchema {
                    serialized_schema: schema.into(),
                }),
                rows: Some(ArrowRecordBatch {
                    serialized_record_batch: batch.into(),
                    ..Default::default()
                }),
            })),
            ..Default::default()
        }
    }

    fn proto_req(stream: &str, msg_name: &str, row: &'static [u8]) -> AppendRowsRequest {
        AppendRowsRequest {
            write_stream: stream.to_string(),
            rows: Some(Rows::ProtoRows(ProtoData {
                writer_schema: Some(ProtoSchema {
                    proto_descriptor: Some(prost_types::DescriptorProto {
                        name: Some(msg_name.to_string()),
                        ..Default::default()
                    }),
                }),
                rows: Some(ProtoRows {
                    serialized_rows: vec![row.into()],
                }),
            })),
            ..Default::default()
        }
    }

    #[test]
    fn simplex_arrow() {
        let r1 = arrow_req("stream_1", b"schema_1", b"batch_1");
        let mut optimizer = SendOptimizer::new(&r1);

        let mut r2 = arrow_req("stream_1", b"schema_1", b"batch_2");
        optimizer.optimize(&mut r2);
        assert_eq!(r2.write_stream, "");
        let Some(Rows::ArrowRows(data2)) = r2.rows else {
            panic!("expected ArrowRows");
        };
        assert!(data2.writer_schema.is_none());
        assert_eq!(
            data2.rows.expect("rows").serialized_record_batch,
            b"batch_2".as_slice()
        );

        let mut r3 = arrow_req("stream_1", b"schema_1", b"batch_3");
        optimizer.optimize(&mut r3);
        assert_eq!(r3.write_stream, "");
        let Some(Rows::ArrowRows(data3)) = r3.rows else {
            panic!("expected ArrowRows");
        };
        assert!(data3.writer_schema.is_none());
    }

    #[test]
    fn multiplex_destination_switch() {
        // r1: {write_stream: stream_1}
        let r1 = arrow_req("stream_1", b"schema_1", b"batch_1");
        let mut optimizer = SendOptimizer::new(&r1);

        // r2: {write_stream: /*omit*/, writer_schema: /*omit*/}
        let mut r2 = arrow_req("stream_1", b"schema_1", b"batch_2");
        optimizer.optimize(&mut r2);
        assert_eq!(r2.write_stream, "");
        let Some(Rows::ArrowRows(data2)) = r2.rows else {
            panic!("expected ArrowRows");
        };
        assert!(data2.writer_schema.is_none());

        // r3: {write_stream: /*omit*/, writer_schema: /*omit*/}
        let mut r3 = arrow_req("stream_1", b"schema_1", b"batch_3");
        optimizer.optimize(&mut r3);
        assert_eq!(r3.write_stream, "");
        let Some(Rows::ArrowRows(data3)) = r3.rows else {
            panic!("expected ArrowRows");
        };
        assert!(data3.writer_schema.is_none());

        // r4: {write_stream: stream_2, writer_schema: schema_1}
        let mut r4 = arrow_req("stream_2", b"schema_1", b"batch_4");
        optimizer.optimize(&mut r4);
        assert_eq!(r4.write_stream, "stream_2");
        let Some(Rows::ArrowRows(data4)) = r4.rows else {
            panic!("expected ArrowRows");
        };
        assert!(data4.writer_schema.is_some());

        // r5: {write_stream: stream_2, writer_schema: /*omit*/}
        // Destination changed in r4, so write_stream must be populated in all subsequent requests.
        let mut r5 = arrow_req("stream_2", b"schema_1", b"batch_5");
        optimizer.optimize(&mut r5);
        assert_eq!(r5.write_stream, "stream_2");
        let Some(Rows::ArrowRows(data5)) = r5.rows else {
            panic!("expected ArrowRows");
        };
        assert!(data5.writer_schema.is_none());
    }

    #[test]
    fn schema_evolution_proto() {
        let r1 = proto_req("stream_1", "Msg1", b"row_1");
        let mut optimizer = SendOptimizer::new(&r1);

        // Consecutive write with same stream and schema omits both.
        let mut r2 = proto_req("stream_1", "Msg1", b"row_2");
        optimizer.optimize(&mut r2);
        assert_eq!(r2.write_stream, "");
        let Some(Rows::ProtoRows(data2)) = r2.rows else {
            panic!("expected ProtoRows");
        };
        assert!(data2.writer_schema.is_none());

        // Schema change on the default stream sends both write_stream and writer_schema.
        let mut r3 = proto_req("stream_1", "Msg2", b"row_3");
        optimizer.optimize(&mut r3);
        assert_eq!(r3.write_stream, "stream_1");
        let Some(Rows::ProtoRows(data3)) = r3.rows else {
            panic!("expected ProtoRows");
        };
        assert!(data3.writer_schema.is_some());

        // Subsequent write after schema change keeps write_stream and omits writer_schema.
        let mut r4 = proto_req("stream_1", "Msg2", b"row_4");
        optimizer.optimize(&mut r4);
        assert_eq!(r4.write_stream, "stream_1");
        let Some(Rows::ProtoRows(data4)) = r4.rows else {
            panic!("expected ProtoRows");
        };
        assert!(data4.writer_schema.is_none());
    }
}
