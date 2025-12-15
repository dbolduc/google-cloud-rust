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

use super::stub::Stub;
use crate::Result;
use crate::generated::gapic_dataplane::stub::dynamic::Subscriber as GapicStub;
use crate::generated::gapic_dataplane::transport::Subscriber as Transport;
use crate::google::pubsub::v1::{StreamingPullRequest, StreamingPullResponse};
use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;

mod info {
    const NAME: &str = env!("CARGO_PKG_NAME");
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    lazy_static::lazy_static! {
        pub(crate) static ref X_GOOG_API_CLIENT_HEADER: String = {
            let ac = gaxi::api_header::XGoogApiClient{
                name:          NAME,
                version:       VERSION,
                library_type:  gaxi::api_header::GCCL,
            };
            ac.grpc_header_value()
        };
    }
}

#[async_trait::async_trait]
impl Stub for Transport {
    type Stream = tonic::codec::Streaming<StreamingPullResponse>;

    async fn modify_ack_deadline(
        &self,
        req: crate::model::ModifyAckDeadlineRequest,
        options: gax::options::RequestOptions,
    ) -> Result<gax::response::Response<()>> {
        GapicStub::modify_ack_deadline(self, req, options).await
    }

    async fn acknowledge(
        &self,
        req: crate::model::AcknowledgeRequest,
        options: gax::options::RequestOptions,
    ) -> Result<gax::response::Response<()>> {
        GapicStub::acknowledge(self, req, options).await
    }

    async fn streaming_pull(
        &self,
        request_rx: Receiver<StreamingPullRequest>,
        options: gax::options::RequestOptions,
    ) -> Result<tonic::Response<Self::Stream>> {
        let request = ReceiverStream::new(request_rx);

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
        self.inner
            .bidi_stream(
                extensions,
                path,
                request,
                options,
                &info::X_GOOG_API_CLIENT_HEADER,
                "",
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn todo() -> anyhow::Result<()> {
        todo!("idr")
    }
}
