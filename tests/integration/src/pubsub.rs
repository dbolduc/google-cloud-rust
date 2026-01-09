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

use crate::{Error, Result};

use futures::future::join_all;
use pubsub::client::Client;
use pubsub::model::PubsubMessage;
pub use pubsub_samples::{cleanup_test_topic, create_test_topic};
pub use pubsub_samples::create_test_subscription;

pub async fn basic_publisher() -> Result<()> {
    let (topic_admin, topic) = create_test_topic().await?;

    tracing::info!("testing publish()");
    let client = Client::builder().build().await?;
    let publisher = client.publisher(topic.name.clone()).build();
    let messages: [PubsubMessage; 2] = [
        PubsubMessage::new().set_data("Hello"),
        PubsubMessage::new().set_data("World"),
    ];

    let mut handles = Vec::new();
    for msg in messages {
        handles.push(publisher.publish(msg));
    }

    let results = join_all(handles).await;
    let message_ids: Vec<_> = results
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::from)?;
    tracing::info!("successfully published messages: {:#?}", message_ids);
    cleanup_test_topic(&topic_admin, topic.name).await?;

    Ok(())
}

pub async fn basic_topic() -> Result<()> {
    let (client, topic) = create_test_topic().await?;

    tracing::info!("testing get_topic()");
    let get_topic = client
        .get_topic()
        .set_topic(topic.name.clone())
        .send()
        .await?;
    tracing::info!("success with get_topic={get_topic:?}");
    assert_eq!(get_topic.name, topic.name);

    cleanup_test_topic(&client, topic.name).await?;

    Ok(())
}

pub async fn basic_roundtrip() -> Result<()> {
    let (topic_admin, topic) = create_test_topic().await?;
    let (sub_admin, sub) = create_test_subscription(topic.name.clone()).await?;

    tracing::info!("testing publish()");
    let client = Client::builder().build().await?;
    let publisher = client.publisher(topic.name.clone()).build();
    let messages: [PubsubMessage; 2] = [
        PubsubMessage::new().set_data("Hello"),
        PubsubMessage::new().set_data("World"),
    ];

    let mut handles = Vec::new();
    for msg in messages {
        handles.push(publisher.publish(msg));
    }

    let results = join_all(handles).await;
    let message_ids: Vec<_> = results
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::from)?;
    tracing::info!("successfully published messages: {:#?}", message_ids);

    use pubsub::client::Subscriber;
    let subscriber = Subscriber::builder().build().await?;
    let mut session = subscriber.streaming_pull(sub.name.clone()).start().await?;

    let mut data = Vec::new();
    for i in 0..2 {
        let Some((m, h)) = session.next().await.transpose()? else {
            anyhow::bail!("Expected at least 2 messages.")
        };
        data.push(m.data);
    }
    drop(session);
    data.sort();
    assert_eq!(data[0], "Hello", "data={data:?}");
    assert_eq!(data[1], "World", "data={data:?}");
    tracing::info!("successfully received messages: {data:#?}");

    cleanup_test_topic(&topic_admin, topic.name).await?;
    //cleanup_test_subscription(&sub_admin, sub.name).await?;

    Ok(())
}

