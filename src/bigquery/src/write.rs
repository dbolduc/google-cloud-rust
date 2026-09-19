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

/// Data format configuration and traits for stream writers.
pub mod format;
/// Stream type markers and traits for stream writers.
pub mod stream;

pub use append_future::AppendFuture;
pub use buffered::BufferedWriter;
pub use committed::CommittedWriter;
pub use default::DefaultWriter;
pub use pending::PendingWriter;
pub(super) use writer_builder::WriterBuilder;

pub(super) mod append_future;
pub(super) mod append_response;
pub(super) mod base;
pub(super) mod buffered;
pub(super) mod builder;
pub(super) mod client;
pub(super) mod client_builder;
pub(super) mod committed;
pub(super) mod default;
pub(super) mod error;
pub(super) mod pending;
pub(super) mod writer_builder;

mod dispatcher;
mod entry;
mod grpc_stream;
mod pool;
mod proto_schema;
#[allow(dead_code)]
mod retry_policy;
mod runner;
mod transport;
mod validate;

// TODO(#4832) - remove handwritten code.
mod status;

#[allow(dead_code)]
pub(crate) mod generated;

#[cfg(test)]
mod test;
