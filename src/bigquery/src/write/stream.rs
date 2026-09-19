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
    pub trait Stream: Sized {
        const STREAM_TYPE: Option<Type>;

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> Self::Writer<F>
        where
            Self: super::Stream;
    }

    impl Stream for DefaultStream {
        const STREAM_TYPE: Option<Type> = None;

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> DefaultWriter<F> {
            DefaultWriter::new(builder.stream_pool(), write_stream, format)
        }
    }

    impl Stream for PendingStream {
        const STREAM_TYPE: Option<Type> = Some(Type::Pending);

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> PendingWriter<F> {
            PendingWriter::new(builder.inner, write_stream, format)
        }
    }

    impl Stream for CommittedStream {
        const STREAM_TYPE: Option<Type> = Some(Type::Committed);

        fn construct<F>(
            builder: WriterBuilder<Self>,
            write_stream: String,
            format: F,
        ) -> CommittedWriter<F> {
            CommittedWriter::new(builder.inner, write_stream, format)
        }
    }

    impl Stream for BufferedStream {
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
    pub trait ApplicationCreatedStream {}

    impl ApplicationCreatedStream for PendingStream {}
    impl ApplicationCreatedStream for CommittedStream {}
    impl ApplicationCreatedStream for BufferedStream {}

    /// Sealed trait for mapping a writer back to its stream type.
    pub trait HasStream {}

    impl<F> HasStream for DefaultWriter<F> {}
    impl<F> HasStream for PendingWriter<F> {}
    impl<F> HasStream for CommittedWriter<F> {}
    impl<F> HasStream for BufferedWriter<F> {}
}

/// Trait mapping a write stream marker ([`DefaultStream`], [`PendingStream`], [`CommittedStream`],
/// [`BufferedStream`]) to its corresponding writer type.
///
/// This trait is sealed and cannot be implemented for types outside this crate.
pub trait Stream: sealed::Stream {
    /// The writer type constructed for this stream type and data format `F`.
    type Writer<F>;
}

impl Stream for DefaultStream {
    type Writer<F> = DefaultWriter<F>;
}

impl Stream for PendingStream {
    type Writer<F> = PendingWriter<F>;
}

impl Stream for CommittedStream {
    type Writer<F> = CommittedWriter<F>;
}

impl Stream for BufferedStream {
    type Writer<F> = BufferedWriter<F>;
}

/// Marker trait for [application-created stream] types ([`PendingStream`], [`CommittedStream`],
/// [`BufferedStream`]) that can be created via [`Write::create_stream`][crate::client::Write::create_stream]
/// or attached to via [`Write::attach_to_stream`][crate::client::Write::attach_to_stream].
///
/// This trait is sealed and cannot be implemented for types outside this crate.
///
/// [application-created stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#application-created_streams
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an application-created stream type",
    label = "expected `PendingStream`, `CommittedStream`, or `BufferedStream`",
    note = "default streams are managed by BigQuery and cannot be created via `create_stream`; use `Write::open_default_stream` instead"
)]
pub trait ApplicationCreatedStream: Stream + sealed::ApplicationCreatedStream {}

impl ApplicationCreatedStream for PendingStream {}
impl ApplicationCreatedStream for CommittedStream {}
impl ApplicationCreatedStream for BufferedStream {}

/// Trait mapping a writer type ([`DefaultWriter`], [`PendingWriter`], [`CommittedWriter`],
/// [`BufferedWriter`]) back to its corresponding [`Stream`].
///
/// This trait is sealed and cannot be implemented for types outside this crate.
#[diagnostic::on_unimplemented(
    message = "cannot infer the writer or stream type",
    label = "type annotations needed for this writer",
    note = "annotate the variable type (e.g. `let writer: PendingWriter<Arrow> = ...`) or specify a stream type via turbofish (e.g. `client.create_stream::<PendingStream>(...)`)"
)]
pub trait HasStream: sealed::HasStream {
    /// The stream marker corresponding to this writer.
    type Stream: Stream;
}

impl<F> HasStream for DefaultWriter<F> {
    type Stream = DefaultStream;
}

impl<F> HasStream for PendingWriter<F> {
    type Stream = PendingStream;
}

impl<F> HasStream for CommittedWriter<F> {
    type Stream = CommittedStream;
}

impl<F> HasStream for BufferedWriter<F> {
    type Stream = BufferedStream;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::format::{Arrow, Proto};
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    macro_rules! assert_format_mappings {
        ($F:ty) => {
            assert_impl_all!(DefaultStream: Stream<Writer<$F> = DefaultWriter<$F>>);
            assert_impl_all!(PendingStream: Stream<Writer<$F> = PendingWriter<$F>>);
            assert_impl_all!(CommittedStream: Stream<Writer<$F> = CommittedWriter<$F>>);
            assert_impl_all!(BufferedStream: Stream<Writer<$F> = BufferedWriter<$F>>);

            assert_impl_all!(DefaultWriter<$F>: HasStream<Stream = DefaultStream>);
            assert_impl_all!(PendingWriter<$F>: HasStream<Stream = PendingStream>);
            assert_impl_all!(CommittedWriter<$F>: HasStream<Stream = CommittedStream>);
            assert_impl_all!(BufferedWriter<$F>: HasStream<Stream = BufferedStream>);
        };
    }

    #[test]
    fn stream_and_writer_mappings() {
        assert_format_mappings!(Arrow);
        assert_format_mappings!(Proto);

        assert_impl_all!(PendingStream: ApplicationCreatedStream);
        assert_impl_all!(CommittedStream: ApplicationCreatedStream);
        assert_impl_all!(BufferedStream: ApplicationCreatedStream);
        assert_not_impl_any!(DefaultStream: ApplicationCreatedStream);
    }
}
