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

#[cfg(test)]
mod tests {
    use google_cloud_pubsub::subscriber::client::Subscriber;

    //#[tokio::test]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quickstart() -> anyhow::Result<()> {
        // Enable a basic subscriber. Useful to troubleshoot problems and visually
        // verify tracing is doing something.
        let _guard = {
            use tracing_subscriber::fmt::format::FmtSpan;
            let subscriber = tracing_subscriber::fmt()
                .with_level(true)
                .with_thread_ids(true)
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .finish();

            tracing::subscriber::set_default(subscriber)
        };

        let client = Subscriber::new().await?;

        tracing::info!("Opening subscriber session...");
        let session = client
            .subscribe(
                format!("projects/dbolduc-test/subscriptions/subscription-id"),
                async |m| {
                    println!("Received message m={m:?}");
                    m.ack().await;
                },
            )
            .start()
            .await?;
        tracing::info!("Subscriber session is open.");

        // Wait for 15 seconds then close the session.
        tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
        tracing::info!("Closing subscriber session...");
        let status = session.close().await;
        tracing::info!("Subscriber session closed. Status={status:?}");

        Ok(())
    }

    fn need_send<T: Send>(_val: &T) {}
    fn need_sync<T: Sync>(_val: &T) {}
    fn need_static<T: 'static>(_val: &T) {}

    #[tokio::test]
    async fn session_markers() -> anyhow::Result<()> {
        let client = Subscriber::new().await?;
        let builder = client.subscribe(
            format!("projects/dbolduc-test/subscriptions/subscription-id"),
            async |m| {
                println!("Received message m={m:?}");
                m.ack().await;
            },
        );

        let session_fut = builder.start();
        need_send(&session_fut);
        need_static(&session_fut);

        let session = session_fut.await?;
        need_send(&session);
        need_sync(&session);
        need_static(&session);

        session.close().await?;

        Ok(())
    }
}
