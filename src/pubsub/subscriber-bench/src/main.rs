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

use google_cloud_pubsub::subscriber::client::Subscriber;

fn trace_guard() -> tracing::dispatcher::DefaultGuard {
    use tracing_subscriber::fmt::format::FmtSpan;
    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .with_thread_ids(true)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .finish();

    tracing::subscriber::set_default(subscriber)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
//#[tokio::main(flavor="current_thread")]
async fn main() -> anyhow::Result<()> {
    //let _guard = trace_guard();

    //let client = Subscriber::new().await?;

    tracing::info!("Opening subscriber session...");

    // Wait for N seconds then close the session.
    use tokio::time::{Duration, sleep};
    use tokio_util::sync::CancellationToken;
    let cancel = CancellationToken::new();
    let cancel_cloned = cancel.clone();
    tokio::spawn(async move {
        // NOTE : go for a minute instead.
        sleep(Duration::from_secs(60)).await;
        tracing::info!("Closing subscriber session...");
        cancel_cloned.cancel();
    });

    let mut tasks = Vec::new();
    // I am seeing like: 716k messages over 60 seconds. 12 MB/s. Pretty bad, but
    // better than before.
    for i in 0..64 {
        let cancel = cancel.clone();
        // NOTE: keeping the client outside of the task loop is awful. I think
        // that means having one gRPC channel is a bottleneck.
        //
        // Or some other internal thing, like back pressure on one of the
        // channels?
        let client = Subscriber::new().await?;

        tracing::info!("Opening subscriber session {i}.");
        let mut session = client
            .subscribe(format!(
                "projects/dbolduc-test/subscriptions/subscription-id"
            ))
            .start()
            .await?;
        tasks.push(tokio::spawn(async move {
            let mut ack_count = 0;
            while let Some(m) = tokio::select! {
                v = session.next() => v,
                _ = cancel.cancelled() => None,
            } {
                tracing::info!("Application received message={m:?}.");
                m.ack().await;
                ack_count += 1;
            }

            let status = session.close().await;
            tracing::info!("Subscriber session {i} is closed. Status={status:?}");
            ack_count
        }))
    }
    let mut ack_count = 0;
    for t in tasks {
        ack_count += t.await?;
    }

    println!("Acked {ack_count} messages in 60 seconds, via 64 streams/clients.");

    Ok(())
}
