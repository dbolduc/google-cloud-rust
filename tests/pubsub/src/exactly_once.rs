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

use bytes::Bytes;
use google_cloud_pubsub::client::Subscriber;
use google_cloud_pubsub::subscriber::handler::Handler::ExactlyOnce;
use std::collections::HashSet;

pub async fn subscribe(subscription_name: &str) -> anyhow::Result<()> {
    let subscriber = Subscriber::builder().build().await?;
    let mut session = subscriber.stream(subscription_name).build();

    let mut got = HashSet::new();
    // TODO : way more messages would be better
    let mut handles = Vec::new();
    for _ in 0..2 {
        if let Some((m, ExactlyOnce(h))) = session.next().await.transpose()? {
            got.insert(m.data);
            handles.push(h);
        }
    }
    for h in handles {
        h.confirmed_ack().await?;
    }

    let want = HashSet::from([Bytes::from("Hello"), Bytes::from("World")]);
    assert_eq!(got, want);
    tracing::info!("successfully received messages: {got:#?}");

    Ok(())
}
