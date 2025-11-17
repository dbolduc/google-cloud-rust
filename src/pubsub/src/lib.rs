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

//! Google Cloud Client Libraries for Rust - Pub/Sub
//!
//! **WARNING:** this crate is under active development. We expect multiple
//! breaking changes in the upcoming releases. Testing is also incomplete, we do
//! **not** recommend that you use this crate in production. We welcome feedback
//! about the APIs, documentation, missing features, bugs, etc.
//!
//! This crate contains traits, types, and functions to interact with
//! [Pub/Sub]. Most applications will use the structs defined in the
//! [client] module.
//!
//! For administrative operations:
//! * [TopicAdmin][client::TopicAdmin]
//! * [SubscriptionAdmin][client::SubscriptionAdmin]
//! * [SchemaService][client::SchemaService]
//!
//! For publishing messages:
//! * [Client][client::Client] and [Publisher][client::Publisher]
//!
//! Receiving messages is not yet supported by this crate.
//!
//! **NOTE:** This crate used to contain a different implementation, with a
//! different surface. [@yoshidan](https://github.com/yoshidan) generously
//! donated the crate name to Google. Their crate continues to live as
//! [gcloud-pubsub].
//!
//! [pub/sub]: https://cloud.google.com/pubsub
//! [gcloud-pubsub]: https://crates.io/crates/gcloud-pubsub

pub(crate) mod generated;

pub(crate) mod publisher;
pub mod subscriber;

pub use gax::Result;
pub use gax::error::Error;

/// Request and client builders.
pub mod builder {
    /// Request and client builders for the [Publisher][crate::client::Publisher] client.
    pub mod publisher {
        #[doc(hidden)]
        pub use crate::generated::gapic_dataplane::builder::publisher::*;
        pub use crate::publisher::client::ClientBuilder;
        pub use crate::publisher::publisher::PublisherBuilder;
    }
    /// Request and client builders for the [SchemaService][crate::client::SchemaService] client.
    pub use crate::generated::gapic::builder::schema_service;
    /// Request and client builders for the [SubscriptionAdmin][crate::client::SubscriptionAdmin] client.
    pub use crate::generated::gapic::builder::subscription_admin;
    /// Request and client builders for the [TopicAdmin][crate::client::TopicAdmin] client.
    pub use crate::generated::gapic::builder::topic_admin;
}

/// The messages and enums that are part of this client library.
pub mod model {
    pub use crate::generated::gapic::model::*;
    pub use crate::generated::gapic_dataplane::model::PubsubMessage;
    pub(crate) use crate::generated::gapic_dataplane::model::*;
}

/// Extends [model] with types that improve type safety and/or ergonomics.
pub mod model_ext {
    pub use crate::publisher::model_ext::*;
}

/// Clients to interact with Google Cloud Pub/Sub.
///
/// This module contains the primary entry points for the library, including
/// clients for publishing messages and managing topics and subscriptions.
///
/// # Example: Publishing Messages
///
/// ```
/// # async fn sample() -> anyhow::Result<()> {
/// use google_cloud_pubsub::client::Client;
/// use google_cloud_pubsub::model::PubsubMessage;
///
/// // Create a client for creating publishers.
/// let client = Client::builder().build().await?;
///
/// // Create a publisher that handles batching for a specific topic.
/// let publisher = client.publisher("projects/my-project/topics/my-topic").build();
///
/// // Publish several messages.
/// // The client will automatically batch them in the background.
/// let mut handles = Vec::new();
/// for i in 0..10 {
///     let msg = PubsubMessage::new().set_data(format!("message {}", i));
///     handles.push(publisher.publish(msg));
/// }
///
/// // The handles are futures that resolve to the server-assigned message IDs.
/// // You can await them to get the results. Messages will still be sent even
/// // if the handles are dropped.
/// for (i, handle) in handles.into_iter().enumerate() {
///     let message_id = handle.await?;
///     println!("Message {} sent with ID: {}", i, message_id);
/// }
/// # Ok(())
/// # }
/// ```
pub mod client {
    pub use crate::generated::gapic::client::*;
    pub use crate::publisher::client::Client;
    pub use crate::publisher::publisher::Publisher;
}

/// Traits to mock the clients in this library.
pub mod stub {
    pub use crate::generated::gapic::stub::*;
}

const DEFAULT_HOST: &str = "https://pubsub.googleapis.com";

mod info {
    const NAME: &str = env!("CARGO_PKG_NAME");
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    lazy_static::lazy_static! {
        pub(crate) static ref X_GOOG_API_CLIENT_HEADER: String = {
            let ac = gaxi::api_header::XGoogApiClient{
                name:          NAME,
                version:       VERSION,
                library_type:  gaxi::api_header::GAPIC,
            };
            ac.grpc_header_value()
        };
    }
}

// Sidekick-generated protos. Exposed due to laziness.
pub mod google {
    pub mod api {
        include!("generated/protos/google.api.rs");
    }
    pub mod pubsub {
        pub mod v1 {
            include!("generated/convert/pubsub/convert.rs");
            include!("generated/protos/google.pubsub.v1.rs");
        }
    }
}

use crate::google::pubsub::v1;

#[derive(Clone)]
pub struct RawGrpcSubscriberBooooo {
    inner: gaxi::grpc::Client,
}

impl std::fmt::Debug for RawGrpcSubscriberBooooo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Subscriber")
            .field("inner", &self.inner)
            .finish()
    }
}

impl RawGrpcSubscriberBooooo {
    pub async fn new(config: gaxi::options::ClientConfig) -> gax::client_builder::Result<Self> {
        let inner = gaxi::grpc::Client::new(config, DEFAULT_HOST).await?;
        Ok(Self { inner })
    }

    // Similar to a transport stub's API, but strictly in gRPC-land. (dbolduc is
    // too lazy to wrap the gRPC-generated types and convert them.)
    pub async fn streaming_pull(
        &self,
        // TODO : We would need to wrap the futures::Stream in our own streaming type
        // We have `StreamingSource` in storage, specifically for conversion to bytes.
        // I think we add a generic wrapper for conversion to `T`, `where T: wkt::Message`.
        // TODO : we might also need to consider how a veneer adds metadata for the initial
        // write. e.g. I know GCS WriteObject has a routing parameter we are supposed to send.
        request: impl futures::Stream<Item = v1::StreamingPullRequest> + Send + 'static,
        options: gax::options::RequestOptions,
    ) -> Result<tonic::Response<tonic::codec::Streaming<v1::StreamingPullResponse>>> {
        let extensions = {
            let mut e = tonic::Extensions::new();
            e.insert(tonic::GrpcMethod::new(
                "google.pubsub.v1.Subscriber",
                "StreamingPull",
            ));
            e
        };
        let path =
            http::uri::PathAndQuery::from_static("/google.pubsub.v1.Subscriber/StreamingPull");

        let api_client_header = "N/A - darren is testing";
        self.inner
            .bidi_stream(
                extensions,
                path,
                request,
                options,
                api_client_header,
                "", // I don't think streaming writes have routing params
            )
            .await
    }
}
