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
use crate::google::pubsub::v1;
use crate::info;
use crate::{Error, Result};

// NOTE : This abstraction will be useful for OUR tests. I do not think we expose the interface for this. (We do not want )
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

    // NOTE : I am just copying generated code for now. Would be nice to use the stub directly.
    async fn modify_ack_deadline(
        &self,
        req: crate::model::ModifyAckDeadlineRequest,
        options: gax::options::RequestOptions,
    ) -> Result<gax::response::Response<()>> {
        use gaxi::prost::ToProto;
        let options = gax::options::internal::set_default_idempotency(options, false);
        let extensions = {
            let mut e = tonic::Extensions::new();
            e.insert(tonic::GrpcMethod::new(
                "google.pubsub.v1.Subscriber",
                "ModifyAckDeadline",
            ));
            e
        };
        let path =
            http::uri::PathAndQuery::from_static("/google.pubsub.v1.Subscriber/ModifyAckDeadline");
        let x_goog_request_params = [Some(&req)
            .map(|m| &m.subscription)
            .map(|s| s.as_str())
            .map(|v| format!("subscription={v}"))]
        .into_iter()
        .flatten()
        .fold(String::new(), |b, p| b + "&" + &p);

        type TR = ();
        self.inner
            .execute(
                extensions,
                path,
                req.to_proto().map_err(Error::deser)?,
                options,
                &info::X_GOOG_API_CLIENT_HEADER,
                &x_goog_request_params,
            )
            .await
            .and_then(gaxi::grpc::to_gax_response::<TR, ()>)
    }

    async fn acknowledge(
        &self,
        req: crate::model::AcknowledgeRequest,
        options: gax::options::RequestOptions,
    ) -> Result<gax::response::Response<()>> {
        use gaxi::prost::ToProto;
        let options = gax::options::internal::set_default_idempotency(options, false);
        let extensions = {
            let mut e = tonic::Extensions::new();
            e.insert(tonic::GrpcMethod::new(
                "google.pubsub.v1.Subscriber",
                "Acknowledge",
            ));
            e
        };
        let path = http::uri::PathAndQuery::from_static("/google.pubsub.v1.Subscriber/Acknowledge");
        let x_goog_request_params = [Some(&req)
            .map(|m| &m.subscription)
            .map(|s| s.as_str())
            .map(|v| format!("subscription={v}"))]
        .into_iter()
        .flatten()
        .fold(String::new(), |b, p| b + "&" + &p);

        type TR = ();
        self.inner
            .execute(
                extensions,
                path,
                req.to_proto().map_err(Error::deser)?,
                options,
                &info::X_GOOG_API_CLIENT_HEADER,
                &x_goog_request_params,
            )
            .await
            .and_then(gaxi::grpc::to_gax_response::<TR, ()>)
    }
}
