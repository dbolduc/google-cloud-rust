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

use crate::model::write_stream::Type;
use crate::model::{ArrowSchema, ProtoSchema};

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

/// Marker trait for stream modes that can be attached to an existing write stream.
///
/// This trait is sealed and cannot be implemented for types outside this crate.
pub trait Writer: sealed::Attachable {}

impl Writer for PendingStream {}
impl Writer for CommittedStream {}
impl Writer for BufferedStream {}
