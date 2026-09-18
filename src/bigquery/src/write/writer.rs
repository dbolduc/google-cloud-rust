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

/// Marker type representing a [default stream].
///
/// [default stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#default_stream
#[derive(Clone, Copy, Debug)]
pub struct DefaultStream;

/// Marker type representing a [pending type] write stream.
///
/// [pending type]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#pending_type
#[derive(Clone, Copy, Debug)]
pub struct PendingStream;

/// Marker type representing a [committed type] write stream.
///
/// [committed type]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#committed_type
#[derive(Clone, Copy, Debug)]
pub struct CommittedStream;

/// Marker type representing a [buffered type] write stream.
///
/// [buffered type]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#buffered_type
#[derive(Clone, Copy, Debug)]
pub struct BufferedStream;

pub(crate) mod sealed {
    use super::*;

    /// Sealed trait for all write stream types.
    pub trait StreamType: Sized {
        const STREAM_TYPE: Option<Type>;

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> Self::Writer<F>
        where
            Self: super::StreamType;
    }

    impl StreamType for DefaultStream {
        const STREAM_TYPE: Option<Type> = None;

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> DefaultWriter<F> {
            DefaultWriter::new(builder.stream_pool(), write_stream, format)
        }
    }

    impl StreamType for PendingStream {
        const STREAM_TYPE: Option<Type> = Some(Type::Pending);

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> PendingWriter<F> {
            PendingWriter::new(builder.inner, write_stream, format)
        }
    }

    impl StreamType for CommittedStream {
        const STREAM_TYPE: Option<Type> = Some(Type::Committed);

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> CommittedWriter<F> {
            CommittedWriter::new(builder.inner, write_stream, format)
        }
    }

    impl StreamType for BufferedStream {
        const STREAM_TYPE: Option<Type> = Some(Type::Buffered);

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> BufferedWriter<F> {
            BufferedWriter::new(builder.inner, write_stream, format)
        }
    }

    /// Sealed trait for application-created write stream types.
    pub trait CreatedStreamType {}

    impl CreatedStreamType for PendingStream {}
    impl CreatedStreamType for CommittedStream {}
    impl CreatedStreamType for BufferedStream {}
}

/// Trait mapping a write stream type ([`DefaultStream`], [`PendingStream`], [`CommittedStream`],
/// [`BufferedStream`]) to its corresponding writer type.
///
/// This trait is sealed and cannot be implemented for types outside this crate.
pub trait StreamType: sealed::StreamType {
    /// The writer type constructed for this stream type and data format `F`.
    type Writer<F>;
}

impl StreamType for DefaultStream {
    type Writer<F> = DefaultWriter<F>;
}

impl StreamType for PendingStream {
    type Writer<F> = PendingWriter<F>;
}

impl StreamType for CommittedStream {
    type Writer<F> = CommittedWriter<F>;
}

impl StreamType for BufferedStream {
    type Writer<F> = BufferedWriter<F>;
}

/// Marker trait for [application-created stream] types ([`PendingStream`], [`CommittedStream`],
/// [`BufferedStream`]) that can be created via [`Write::create_stream`][crate::client::Write::create_stream]
/// or attached to via [`Write::attach_to_stream`][crate::client::Write::attach_to_stream].
///
/// This trait is sealed and cannot be implemented for types outside this crate.
///
/// [application-created stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#application-created_streams
pub trait CreatedStreamType: StreamType + sealed::CreatedStreamType {}

impl CreatedStreamType for PendingStream {}
impl CreatedStreamType for CommittedStream {}
impl CreatedStreamType for BufferedStream {}
