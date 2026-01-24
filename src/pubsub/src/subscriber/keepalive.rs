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

use crate::google::pubsub::v1::StreamingPullRequest;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, interval_at};

pub(super) const KEEPALIVE_PERIOD: Duration = Duration::from_secs(30);

/// Spawns a task to keepalive a stream.
///
/// This task periodically writes requests into a channel. The receiver of this
/// channel is the request stream for a StreamingPull bidi RPC. One of these
/// tasks is spawned for each attempt to open a stream.
///
/// Callers signal a shutdown of this task by dropping the request receiver.
pub(super) fn spawn(request_tx: Sender<StreamingPullRequest>) {
    spawn_impl(request_tx);
}

/// Spawns a task to keepalive a stream.
///
/// Returns a `JoinHandle` on the task, so our unit tests can confirm that the
/// task is eventually joined.
///
/// Note that this function is intentionally private, so that we cannot await
/// the result of the task outside of this module.
fn spawn_impl(request_tx: Sender<StreamingPullRequest>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut keepalive = interval_at(Instant::now() + KEEPALIVE_PERIOD, KEEPALIVE_PERIOD);
        loop {
            tokio::select! {
                _ = keepalive.tick() => {
                    match request_tx.send(StreamingPullRequest::default()).await {
                        Ok(_) => continue,
                        Err(_) => break,
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::channel;

    #[tokio::test(start_paused = true)]
    async fn keepalive_interval() {
        let start = Instant::now();
        let (request_tx, mut request_rx) = channel(1);
        let _handle = spawn(request_tx);

        // Wait for the first keepalive
        let r = request_rx.recv().await.unwrap();
        assert_eq!(r, StreamingPullRequest::default());
        assert_eq!(start.elapsed(), KEEPALIVE_PERIOD);

        // Wait for the second keepalive
        let r = request_rx.recv().await.unwrap();
        assert_eq!(r, StreamingPullRequest::default());
        assert_eq!(start.elapsed(), KEEPALIVE_PERIOD * 2);

        // Wait for the third keepalive
        let r = request_rx.recv().await.unwrap();
        assert_eq!(r, StreamingPullRequest::default());
        assert_eq!(start.elapsed(), KEEPALIVE_PERIOD * 3);
    }

    #[tokio::test(start_paused = true)]
    async fn no_leaked_tasks() -> anyhow::Result<()> {
        let start = Instant::now();
        let (request_tx, mut request_rx) = channel(1);
        let handle = spawn_impl(request_tx);

        // Wait for the first keepalive
        let _ = request_rx.recv().await.unwrap();
        assert_eq!(start.elapsed(), KEEPALIVE_PERIOD);

        // Signal a shutdown
        drop(request_rx);
        // Confirm the task eventually joins. We do not want to leak tasks doing
        // work in the background for each attempt to open a stream.
        handle.await?;

        Ok(())
    }
}
