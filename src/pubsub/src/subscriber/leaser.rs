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

use super::transport::TransportStub;
use crate::{Error, Result};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc::{Sender, channel};
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use tokio_util::sync::CancellationToken;

pub(crate) enum AckResult {
    Ack(String),
    Nack(String),
}

// Stub out the interface for leasing, so we can test.
#[async_trait::async_trait]
trait Leaser {
    async fn ack(ack_ids: Vec<String>) -> Result<()>;
    async fn nack(ack_ids: Vec<String>) -> Result<()>;
    async fn mod_ack(ack_ids: Vec<String>) -> Result<()>;
}

struct DefaultLeaser {
    inner: Arc<TransportStub>,
}

#[async_trait::async_trait]
impl Leaser for DefaultLeaser {
    async fn ack(ack_ids: Vec<String>) -> Result<()> {
        unimplemented!();
    }
    async fn nack(ack_ids: Vec<String>) -> Result<()> {
        unimplemented!();
    }
    async fn mod_ack(ack_ids: Vec<String>) -> Result<()> {
        unimplemented!();
    }
}

// TODO : I think this gets refactored to not even be a struct. We just return the three items on the stack.
// No reason to hoard the senders.
//
// TODO : and the LeaseManager::new is a plain old func, that also takes in a `T: impl Leaser`.
pub(crate) struct LeaseManager {
    // We will move messages from here to either `to_ack` on an ack, or
    // `to_nack` on a nack or lease expiration.
    //under_lease: HashSet<String>,
    // TODO : consider exactly once where we can not just flush these and forget about them.
    // Messages to ack (ack IDs)
    //to_ack: Vec<String>,
    // Messages to nack (ack IDs)
    //to_nack: Vec<String>,
    new_message_tx: Sender<String>,
    //new_message_rx: Receiver<String>,
    ack_tx: Sender<AckResult>,
    //ack_rx: Receiver<AckResult>,

    // TODO : this thing needs to handle exactly once. Will generalize after I sketch it.
    lease_loop: JoinHandle<()>,
}

// TODO : do we need a way to cancel this?
// or does it look more like sending a bunch of nacks?
//   - that would keep this class simpler.
//   - It would be less efficient though.
//   - I think the drawback is that we want it to nack immediately.
impl LeaseManager {
    // TODO : this could just be a fn start_lease_loop {} -> handle, Sender<String>, Sender<AckResult>
    pub(crate) fn new(shutdown: CancellationToken) -> Self {
        // TODO : Options for these channel sizes?
        let (new_message_tx, mut new_message_rx) = channel(1000);
        let (ack_tx, mut ack_rx) = channel(1000);
        let lease_loop = tokio::spawn(async move {
            let mut flush_interval = interval(Duration::from_secs(5));
            // We will move messages from here to either `to_ack` on an ack, or
            // `to_nack` on a nack or lease expiration.
            let mut under_lease = HashSet::new();
            // TODO : consider exactly once where we can not just flush these and forget about them.
            // Messages to ack (ack IDs) - consider using `Acknowledge` message directly.
            let mut to_ack = Vec::new();
            // Messages to nack (ack IDs) - consider using `ModAck` message directly.
            let mut to_nack = Vec::new();

            loop {
                // TODO : do we have a preference for draining the channel vs. flushing?
                tokio::select! {
                    biased; // NOTE : incoming messages can race with their acks, as written. Could be resolved with an ignore list taht is updated when remove fails. Seems fugly tho.
                    _ = shutdown.cancelled() => {
                        tracing::info!("Shutdown: Leaser");
                        // TODO : depends on shutdown behavior. Maybe we nack
                        // everything and wait. Maybe we just drop everything on the
                        // floor.
                        break;
                    },
                    _ = flush_interval.tick() => {
                        tracing::info!("5s have passed. Flushing acks / nacks / modacks");
                        // TODO : move expiring messages into the nack bin
                        to_ack.clear(); // NOTE : simulating a flush
                        to_nack.clear(); // NOTE : simulating a flush
                    },
                    new_message = new_message_rx.recv() => {
                        match new_message {
                            None => {
                                tracing::info!("Sender (new_message_tx) closed.");
                                break;
                            },
                            Some(ack_id) => {
                                tracing::info!("New message under lease management. ack_id={ack_id}");
                                under_lease.insert(ack_id);
                            }
                        }
                    },
                    ack_result = ack_rx.recv() => {
                        match ack_result {
                            None => {
                                // FWIW, I am not sure this can happen. The `Leaser` holds a sender.
                                // Maybe we have to `drop()` the sender before we can join this handle?
                                tracing::info!("Leaser has shutdown? maybe?");
                                break;
                            }
                            Some(AckResult::Ack(ack_id)) => {
                                tracing::info!("Acking message with ack_id={ack_id}");
                                // TODO : this is naive.
                                // With exactly-once, I think we cannot remove IDs from lease management unless the ack was successful?
                                under_lease.remove(&ack_id);
                                to_ack.push(ack_id);
                            },
                            Some(AckResult::Nack(ack_id)) => {
                                tracing::info!("Nacking message with ack_id={ack_id}");
                                // TODO : this is naive.
                                // With exactly-once, I think we cannot remove IDs from lease management unless the ack was successful?
                                under_lease.remove(&ack_id);
                                to_nack.push(ack_id);
                            },
                        }
                    },
                }
                tracing::info!(
                    "\nDEBUG STATE:\nunder_lease={under_lease:?}\nto_ack={to_ack:?}\nto_nack={to_nack:?}"
                );
            }
            tracing::info!(
                "\nDEBUG STATE:\nunder_lease={under_lease:?}\nto_ack={to_ack:?}\nto_nack={to_nack:?}"
            );
        });
        Self {
            //under_lease: HashSet::new(),
            //to_ack: Vec::new(),
            //to_nack: Vec::new(),
            new_message_tx,
            //new_message_rx,
            ack_tx,
            //ack_rx,
            lease_loop,
        }
    }

    pub(crate) fn ack_tx(&self) -> Sender<AckResult> {
        self.ack_tx.clone()
    }

    pub(crate) fn new_message_tx(&self) -> Sender<String> {
        // TODO : This is less than elegant. There is only one thing sending the Leaser messages.
        // So the Leaser should not be holding this prototype sender.
        self.new_message_tx.clone()
    }

    pub(crate) async fn close(self) -> Result<()> {
        // TODO : not 100% necessary, but should be done. It makes shutdown more graceful.
        // If we just hold a JoinHandle instead of a Leaser in the Session, we can just await it in Session::close()
        self.lease_loop.await.map_err(Error::io)
    }
}
