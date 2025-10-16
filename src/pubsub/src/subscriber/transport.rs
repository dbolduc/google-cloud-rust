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

use super::session::TonicStream;
use crate::Result;
use crate::google::pubsub::v1;

pub struct TransportStub {
    inner: gaxi::grpc::Client,
}

impl TransportStub {
    pub async fn new(config: gaxi::options::ClientConfig) -> gax::client_builder::Result<Self> {
        let inner = gaxi::grpc::Client::new(config, crate::DEFAULT_HOST).await?;
        Ok(Self { inner })
    }

    // TODO : the stub should be dealing in `model::` types, not prost types.
    pub async fn streaming_pull(
        &self,
        request: impl futures::Stream<Item = v1::StreamingPullRequest> + Send + 'static,
        options: gax::options::RequestOptions,
    ) -> Result<TonicStream> {
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
