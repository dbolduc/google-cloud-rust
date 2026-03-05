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

use super::builder::StreamingPull;
use super::lease_loop::LeaseLoop;
use super::lease_state::{LeaseInfo, LeaseOptions, NewMessage};
use super::leaser::DefaultLeaser;
use super::message_stream::Senders;
use crate::{Error, Result};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
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
    pub(super) shutdown: CancellationToken,
    pub(super) lease_loop: LeaseLoop,
}

impl StreamHandle {
    // TODO : prefer multiple APIs per shutdown behavior?
    // Note to self, I think I just start with one shutdown behavior of
    // `WaitForProcessing`.

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
    pub async fn shutdown(self) {
        self.shutdown.cancel();
        let LeaseLoop {
            handle,
            ack_tx,
            message_tx,
        } = self.lease_loop;
        drop(ack_tx);
        drop(message_tx);
        let _ = handle.await;
    }

    pub(super) fn new(builder: StreamingPull) -> Self {
        let shutdown = CancellationToken::new();

        let (confirmed_tx, _confirmed_rx) = unbounded_channel();
        let leaser = DefaultLeaser::new(
            builder.inner,
            confirmed_tx,
            builder.subscription,
            builder.ack_deadline_seconds,
            builder.grpc_subchannel_count,
        );
        let lease_loop = LeaseLoop::new(leaser, LeaseOptions::default());

        Self {
            shutdown,
            lease_loop,
        }
    }
}
