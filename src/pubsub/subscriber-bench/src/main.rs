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

use google_cloud_pubsub::client::Subscriber;
use tokio::task::JoinSet;
use tokio::time::{Duration, sleep};

fn enable_tracing() {
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::util::SubscriberInitExt;
    tracing_subscriber::fmt()
        .with_level(true)
        .with_thread_ids(true)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .finish()
        .init();
}

// TODO : These should be command line options. Duh.
//const NUM_STREAMS: usize = 32;
//const NUM_CHANNELS: usize = 16;
//const NUM_EVENT_LOOPS: usize = 16;
//const BYTE_LIMIT: i64 = 4 * 1024 * 1024 * 1024;
//const TEST_DURATION: Duration = Duration::from_secs(36 * 60 * 60);

const NUM_STREAMS: usize = 1;
const NUM_CHANNELS: usize = 1;
const NUM_EVENT_LOOPS: usize = 1;
const BYTE_LIMIT: i64 = 4 * 1024 * 1024 * 1024;
const TEST_DURATION: Duration = Duration::from_secs(60);

//const NUM_STREAMS: usize = 16;
//const NUM_CHANNELS: usize = 4;
//const NUM_EVENT_LOOPS: usize = 16;
//const BYTE_LIMIT: i64 = 4 * 1024 * 1024 * 1024;
//const TEST_DURATION: Duration = Duration::from_secs(300);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    enable_tracing();

    let client = Subscriber::builder()
        .with_grpc_subchannel_count(NUM_CHANNELS)
        .build()
        .await?;

    assert!(0 < NUM_EVENT_LOOPS);
    assert!(NUM_EVENT_LOOPS <= NUM_STREAMS);
    let mut stream_prototypes = Vec::new();
    for i in 0..NUM_EVENT_LOOPS {
        tracing::info!("Creating prototype {i}.");
        let stream = client
            .subscribe("projects/dbolduc-test/subscriptions/subscription-id")
            .set_max_outstanding_messages(0)
            .set_max_outstanding_bytes(BYTE_LIMIT)
            .build();
        stream_prototypes.push(stream);
    }

    // Wait for N seconds then close the streams.
    for stream in &stream_prototypes {
        let token = stream.shutdown_token();
        tokio::spawn(async move {
            sleep(TEST_DURATION).await;
            tracing::info!("Closing subscriber stream...");
            token.shutdown().await;
            tracing::info!("Done...");
        });
    }

    let mut tasks: JoinSet<(u64, google_cloud_pubsub::Result<()>)> = JoinSet::new();
    for i in 0..NUM_STREAMS {
        tracing::info!(
            "Creating subscriber stream {i} from prototype {}.",
            i % NUM_EVENT_LOOPS
        );
        let mut stream = stream_prototypes[i % NUM_EVENT_LOOPS].clone();

        tasks.spawn(async move {
            let mut ack_count = 0;
            while let Some(r) = stream.next().await {
                match r {
                    Ok((_m, h)) => h.ack(),
                    Err(e) => return (ack_count, Err(e)),
                };
                ack_count += 1;
            }

            tracing::info!("Subscriber stream {i} is closed.");
            (ack_count, Ok(()))
        });
    }
    let mut ack_count = 0;
    for t in tasks.join_all().await {
        ack_count += t.0;
        tracing::info!("Final stream status={:?}", t.1);
    }

    println!(
        "Acked {ack_count} messages in {TEST_DURATION:?}, via {NUM_STREAMS} streams, serviced by {NUM_EVENT_LOOPS} event loops. Client had {NUM_CHANNELS} channels."
    );

    Ok(())
}
