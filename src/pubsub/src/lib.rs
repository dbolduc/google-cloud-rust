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
//! [Pub/Sub].
//!
//! [pub/sub]: https://cloud.google.com/pubsub

pub(crate) mod generated;

pub mod publisher;

pub use gax::Result;
pub use gax::error::Error;

pub mod builder {
    pub use crate::generated::gapic::builder::*;
    pub mod publisher {
        pub use crate::generated::gapic_dataplane::builder::publisher::*;
        pub use crate::publisher::client::ClientBuilder;
    }
}
pub mod model {
    pub use crate::generated::gapic::model::*;
    pub use crate::generated::gapic_dataplane::model::*;
}
pub mod client {
    pub use crate::generated::gapic::client::*;
    pub use crate::publisher::client::*;
}
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
pub struct Subscriber {
    inner: gaxi::grpc::Client,
}

impl std::fmt::Debug for Subscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Subscriber")
            .field("inner", &self.inner)
            .finish()
    }
}

impl Subscriber {
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
