// Copyright 2025 Google LLC
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

use crate::Result;
use std::collections::HashSet;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use tokio_util::sync::CancellationToken;

/// How often we flush acks/nacks and extend leases.
const ACK_FLUSH: Duration = Duration::from_secs(1);

/// A trait representing leaser actions
///
/// We stub out the interface, so we can test the lease management.
#[async_trait::async_trait]
pub(crate) trait Leaser: Clone {
    /// Acknowledge a batch of messages.
    async fn ack(&self, ack_ids: Vec<String>) -> Result<()>;
    /// Negatively acknowledge a batch of messages.
    async fn nack(&self, ack_ids: Vec<String>) -> Result<()>;
    /// Extend the leases for a batch of messages.
    async fn extend(&self, ack_ids: Vec<String>) -> Result<()>;
}

/// The action an application does with a message.
pub(crate) enum AckResult {
    Ack(String),
    Nack(String),
    // TODO(#3964) - support exactly once acking
}

/// A convenience struct that groups the components of the lease loop
pub(crate) struct LeaseLoop {
    /// A handle to the task running the lease loop
    handle: JoinHandle<()>,
    /// For sending messages from the stream to the lease loop
    message_tx: UnboundedSender<String>,
    /// For sending acks/nacks from the application to the lease loop
    ack_tx: UnboundedSender<AckResult>,
}

struct LeaseState<L>
where
    L: Leaser + Send + 'static,
{
    // TODO(#3957) - support message expiry
    under_lease: HashSet<String>,
    to_ack: Vec<String>,
    to_nack: Vec<String>,
    leaser: L,
}

impl<L> LeaseState<L>
where
    L: Leaser + Send + 'static,
{
    fn new(leaser: L) -> Self {
        Self {
            under_lease: HashSet::new(),
            to_ack: Vec::new(),
            to_nack: Vec::new(),
            leaser,
        }
    }

    fn add(&mut self, ack_id: String) {}
    // TODO : consider async functions if we ever want to flush based on the
    // size of a batch (ack and modack RPCs have a size limit).
    fn ack(&mut self, ack_id: String) {}
    fn nack(&mut self, ack_id: String) {}

    async fn flush(&mut self) {
        // TODO : call into leaser's ack(), nack(), extend().
    }
    async fn shutdown(&mut self) {
        // TODO : bulk nack everything via leaser's nack().
    }
}

pub(crate) fn start_lease_loop<L>(leaser: L, shutdown: CancellationToken) -> LeaseLoop
where
    L: Leaser + Send + 'static,
{
    let (message_tx, mut message_rx) = unbounded_channel();
    let (ack_tx, mut ack_rx) = unbounded_channel();

    let handle = tokio::spawn(async move {
        let mut flush_interval = interval(ACK_FLUSH);

        let mut state = LeaseState::new(leaser);
        loop {
            tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                state.shutdown().await;
                break;
            },
                _ = flush_interval.tick() => state.flush().await,
                message = message_rx.recv() => {
                    match message {
                        None => break,
                        Some(ack_id) => state.add(ack_id),
                    }
                },
                ack_id = ack_rx.recv() => {
                    match ack_id {
                        None => break,
                        Some(AckResult::Ack(ack_id)) => state.ack(ack_id),
                        Some(AckResult::Nack(ack_id)) => state.nack(ack_id),
                    }
                },
            }
        }
    });
    LeaseLoop {
        handle,
        message_tx,
        ack_tx,
    }
}
