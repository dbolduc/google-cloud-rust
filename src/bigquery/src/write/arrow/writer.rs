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

use super::{BufferedWriter, CommittedWriter, DefaultWriter, PendingWriter, WriterBuilder};
use crate::model::write_stream::Type;
use crate::model::{ArrowSchema, ProtoSchema};
use crate::write::proto;

pub(crate) mod sealed {
    use super::*;

    pub trait AttachableWriter {
        const STREAM_TYPE: Type;
    }

    pub trait HasArrow: Sized {
        type Writer;
        fn build_arrow(builder: WriterBuilder<Self>, schema: ArrowSchema) -> Self::Writer;
    }

    pub trait IsArrowWriter {
        type BuilderParam;
    }

    pub trait HasProto: Sized {
        type Writer;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer;
    }

    pub trait IsProtoWriter {
        type BuilderParam;
    }

    impl AttachableWriter for PendingWriter {
        const STREAM_TYPE: Type = Type::Pending;
    }

    impl AttachableWriter for CommittedWriter {
        const STREAM_TYPE: Type = Type::Committed;
    }

    impl AttachableWriter for BufferedWriter {
        const STREAM_TYPE: Type = Type::Buffered;
    }

    impl AttachableWriter for proto::PendingWriter {
        const STREAM_TYPE: Type = Type::Pending;
    }

    impl AttachableWriter for proto::CommittedWriter {
        const STREAM_TYPE: Type = Type::Committed;
    }

    impl AttachableWriter for proto::BufferedWriter {
        const STREAM_TYPE: Type = Type::Buffered;
    }

    impl HasArrow for DefaultWriter {
        type Writer = DefaultWriter;
        fn build_arrow(builder: WriterBuilder<Self>, schema: ArrowSchema) -> Self::Writer {
            DefaultWriter::new(builder.stream_pool(), builder.write_stream, schema)
        }
    }
    impl IsArrowWriter for DefaultWriter {
        type BuilderParam = DefaultWriter;
    }

    impl HasArrow for PendingWriter {
        type Writer = PendingWriter;
        fn build_arrow(builder: WriterBuilder<Self>, schema: ArrowSchema) -> Self::Writer {
            PendingWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl IsArrowWriter for PendingWriter {
        type BuilderParam = PendingWriter;
    }

    impl HasArrow for CommittedWriter {
        type Writer = CommittedWriter;
        fn build_arrow(builder: WriterBuilder<Self>, schema: ArrowSchema) -> Self::Writer {
            CommittedWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl IsArrowWriter for CommittedWriter {
        type BuilderParam = CommittedWriter;
    }

    impl HasArrow for BufferedWriter {
        type Writer = BufferedWriter;
        fn build_arrow(builder: WriterBuilder<Self>, schema: ArrowSchema) -> Self::Writer {
            BufferedWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl IsArrowWriter for BufferedWriter {
        type BuilderParam = BufferedWriter;
    }

    impl HasProto for DefaultWriter {
        type Writer = proto::DefaultWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::DefaultWriter::new(builder.stream_pool(), builder.write_stream, schema)
        }
    }
    impl HasProto for proto::DefaultWriter {
        type Writer = proto::DefaultWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::DefaultWriter::new(builder.stream_pool(), builder.write_stream, schema)
        }
    }
    impl IsProtoWriter for proto::DefaultWriter {
        type BuilderParam = DefaultWriter;
    }

    impl HasProto for PendingWriter {
        type Writer = proto::PendingWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::PendingWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl HasProto for proto::PendingWriter {
        type Writer = proto::PendingWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::PendingWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl IsProtoWriter for proto::PendingWriter {
        type BuilderParam = PendingWriter;
    }

    impl HasProto for CommittedWriter {
        type Writer = proto::CommittedWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::CommittedWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl HasProto for proto::CommittedWriter {
        type Writer = proto::CommittedWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::CommittedWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl IsProtoWriter for proto::CommittedWriter {
        type BuilderParam = CommittedWriter;
    }

    impl HasProto for BufferedWriter {
        type Writer = proto::BufferedWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::BufferedWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl HasProto for proto::BufferedWriter {
        type Writer = proto::BufferedWriter;
        fn build_proto(builder: WriterBuilder<Self>, schema: ProtoSchema) -> Self::Writer {
            proto::BufferedWriter::new(builder.inner, builder.write_stream, schema)
        }
    }
    impl IsProtoWriter for proto::BufferedWriter {
        type BuilderParam = BufferedWriter;
    }
}

/// A trait for strongly-typed stream writers that can be attached to an existing stream.
///
/// This trait is sealed and cannot be implemented for types outside of this crate.
pub trait Writer: sealed::AttachableWriter + Sized {}

impl Writer for PendingWriter {}
impl Writer for CommittedWriter {}
impl Writer for BufferedWriter {}
impl Writer for proto::PendingWriter {}
impl Writer for proto::CommittedWriter {}
impl Writer for proto::BufferedWriter {}
