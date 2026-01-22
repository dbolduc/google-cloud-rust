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

const DESCRIPTION: &str = concat!(
    "The Pub/Sub endurance benchmark aims to measure the reliability of a ",
    "Subscriber message stream.",
    "",
    "It spawns a task publishing messages. It spawns a task receiving ",
    "messages. It runs until a time cutoff.",
    "",
    "If anything fails along the way, the program reports the error and exits."
);

use google_cloud_pubsub::client::Subscriber;
use humantime::parse_duration;
use tokio::time::{Duration, sleep};
use tokio::task::JoinSet;

fn trace_guard() -> tracing::dispatcher::DefaultGuard {
    use tracing_subscriber::fmt::format::FmtSpan;
    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .with_thread_ids(true)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .finish();

    tracing::subscriber::set_default(subscriber)
}

const NUM_CHANNELS: usize = 48;
const TEST_DURATION: Duration = Duration::from_secs(4 * 60 * 60);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = trace_guard();

    let client = Client::builder().build().await;
    let publisher = client.publish("")::builder().build().await;

    let client = Subscriber::builder()
            .with_grpc_subchannel_count(NUM_CHANNELS)
            .build().await?;

    // Wait for N seconds then close the session.
    use tokio_util::sync::CancellationToken;
    let cancel = CancellationToken::new();
    let cancel_cloned = cancel.clone();
    tokio::spawn(async move {
        sleep(TEST_DURATION).await;
        tracing::info!("Closing subscriber session...");
        cancel_cloned.cancel();
    });

    let mut tasks: JoinSet<(i32, google_cloud_pubsub::Result<()>)> = JoinSet::new();
    for i in 0..NUM_STREAMS {
        let cancel = cancel.clone();
        let client = client.clone();

        tracing::info!("Opening subscriber session {i}.");
        let mut session = client
            .streaming_pull("projects/dbolduc-test/subscriptions/subscription-id")
            .set_max_outstanding_messages(0)
            .set_max_outstanding_bytes(1_000_000_000)
            .start();
        tracing::info!("Opened subscriber session {i}.");
        tasks.spawn(async move {
            let mut ack_count = 0;
            while let Some(r) = tokio::select! {
                v = session.next() => v,
                _ = cancel.cancelled() => None,
            } {
                let (m, h) = match r {
                    Ok((m, h)) => (m, h),
                    Err(e) => return (ack_count, Err(e)),
                };
                tracing::info!("Application received message={m:?}.");
                h.ack();
                ack_count += 1;
            }

            //let status = session.close().await;
            tracing::info!("Subscriber session {i} is closed.");
            (ack_count, Ok(()))
        });
    }
    let mut ack_count = 0;
    for t in tasks.join_all().await {
        ack_count += t.0;
        println!("Final stream status={:?}", t.1);
    }

    println!("Acked {ack_count} messages in {TEST_DURATION:?}, via {NUM_STREAMS} streams/clients.");

    Ok(())
}

/// Configure the benchmark
#[derive(Clone, Debug, Parser)]
#[command(version, about, long_about = DESCRIPTION)]
struct Args {
    /// The name of the project used by the benchmark.
    #[arg(long)]
    project_name: String,

    /// The maximum time for the retry loop.
    #[arg(long, value_parser = parse_duration)]
    test_timeout: Duration,

    // TODO : set publish rate?
    // num_streams / tasks / channels?
}
