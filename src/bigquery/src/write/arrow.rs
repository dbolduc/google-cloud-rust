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

pub use super::writer::ArrowFormat;

/// A writer for a [buffered stream] using Arrow as the data format.
///
/// [buffered stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#buffered_type
pub type BufferedWriter = super::BufferedWriter<ArrowFormat>;

/// A writer for a [committed stream] using Arrow as the data format.
///
/// [committed stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#committed_type
pub type CommittedWriter = super::CommittedWriter<ArrowFormat>;

/// A writer for the [default stream] using Arrow as the data format.
///
/// [default stream]: https://docs.cloud.google.com/bigquery/docs/write-api#default_stream
pub type DefaultWriter = super::DefaultWriter<ArrowFormat>;

/// A writer for a [pending stream] using Arrow as the data format.
///
/// [pending stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#pending_type
pub type PendingWriter = super::PendingWriter<ArrowFormat>;
