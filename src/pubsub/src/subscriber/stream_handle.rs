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

use crate::{Error, Result};
use tokio::task::JoinHandle;
use tokio_util::sync::{CancellationToken, DropGuard};

/// A handle to manage stream shutdowns.
///
/// # Example
/// ```rs
/// todo!("write me.")
/// ```
///
/// The handle controls the stream(s) it was returned with. It encapsulates a
/// background task performing [lease management], which is shared by each of
/// its streams.
///
/// See: [Shutdown options] for how to configure the shutdown behavior.
///
/// Applications that do not need to await a graceful shutdown can drop this
/// struct.
///
/// TODO : write more documentation after the code. Documenting is a waste if we
/// don't agree on the design.
///
/// # Drop behavior
///
/// Dropping this struct detaches the background task performing lease
/// management. The task continues to run until completion, as determined by the
/// configured shutdown options.
///
/// Note that if the completion of this task races with the program exiting,
/// message leases can be left in a bad state. That is, if the client library
/// does not have time to nack a message on shutdown, the message can be frozen
/// for as long as 10 minutes (regardless of the configured [ack deadline])
/// before the server redelivers it to another client. If this is unacceptable
/// to your application, you should await the shutdown.
pub struct StreamHandle {
    lease_loop: JoinHandle<()>,
}

impl StreamHandle {
    /// Shutdown the stream(s) associated with this handle.
    ///
    /// The exact behavior is configured via
    /// `Subscribe::set_shutdown_behavior()` builder settings.
    ///
    /// In all cases, calling this function will immediately shutdown the
    /// underlying gRPC streams.
    ///
    /// The `MessageStream`s associated with this handle are not dropped. You
    /// can continue to iterate over them. The streams will likely yield a final
    /// "CANCELLED" error.
    ///
    /// The lease management event loop also continues to run. Its termination
    /// depends on the configured shutdown behavior.
    async fn shutdown(self) -> Result<()> {
        self.lease_loop.await.map_err(Error::io)
    }
}
