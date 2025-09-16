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

//! Google Cloud Client Libraries for Rust - BigQuery
//!
//! **WARNING:** this crate is under active development. We expect multiple
//! breaking changes in the upcoming releases. Testing is also incomplete, we do
//! **not** recommend that you use this crate in production. We welcome feedback
//! about the APIs, documentation, missing features, bugs, etc.
//!
//! This crate contains traits, types, and functions to interact with
//! [BigQuery].
//!
//! [bigquery]: https://cloud.google.com/bigquery

// Sidekick-generated protos. Exposed due to laziness.
pub mod google {
    pub mod api {
        include!("generated/protos/google.api.rs");
    }
    pub mod cloud {
        pub mod bigquery {
            pub mod storage {
                pub mod v1 {
                    include!("generated/protos/google.cloud.bigquery.storage.v1.rs");
                }
            }
        }
    }
    pub mod rpc {
        include!("generated/protos/google.rpc.rs");
    }
}

use crate::google::cloud::bigquery::storage::v1;
use gax::Result;

const DEFAULT_HOST: &str = "https://bigquerystorage.googleapis.com";

#[derive(Clone)]
pub struct BQReadClient {
    inner: gaxi::grpc::Client,
}

impl std::fmt::Debug for BQReadClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("BQReadClient")
            .field("inner", &self.inner)
            .finish()
    }
}

impl BQReadClient {
    pub async fn new(config: gaxi::options::ClientConfig) -> gax::client_builder::Result<Self> {
        let inner = gaxi::grpc::Client::new(config, DEFAULT_HOST).await?;
        Ok(Self { inner })
    }

    // Similar to a transport stub's API, but strictly in gRPC-land. (dbolduc is
    // too lazy to wrap the gRPC-generated types and convert them.)
    pub async fn create_read_session(
        &self,
        request: v1::CreateReadSessionRequest,
        options: gax::options::RequestOptions,
    ) -> Result<tonic::Response<v1::ReadSession>> {
        let extensions = {
            let mut e = tonic::Extensions::new();
            e.insert(tonic::GrpcMethod::new(
                "google.cloud.bigquery.storage.v1.BigQueryRead",
                "CreateReadSession",
            ));
            e
        };
        let path = http::uri::PathAndQuery::from_static(
            "/google.cloud.bigquery.storage.v1.BigQueryRead/CreateReadSession",
        );

        // hardcode the routing params. should be url-encoded, but lazy.
        let x_goog_request_params = format!(
            "read_session.table={}",
            request.read_session.clone().unwrap_or_default().table
        );
        let api_client_header = "N/A - darren is testing";
        self.inner
            .execute(
                extensions,
                path,
                request,
                options,
                api_client_header,
                &x_goog_request_params,
            )
            .await
    }

    pub async fn read_rows(
        &self,
        request: v1::ReadRowsRequest,
        options: gax::options::RequestOptions,
    ) -> Result<tonic::Response<tonic::codec::Streaming<v1::ReadRowsResponse>>> {
        let extensions = {
            let mut e = tonic::Extensions::new();
            e.insert(tonic::GrpcMethod::new(
                "google.cloud.bigquery.storage.v1.BigQueryRead",
                "ReadRows",
            ));
            e
        };
        let path = http::uri::PathAndQuery::from_static(
            "/google.cloud.bigquery.storage.v1.BigQueryRead/ReadRows",
        );

        // hardcode the routing params. should be url-encoded, but lazy.
        let x_goog_request_params = format!("read_stream={}", &request.read_stream);
        let api_client_header = "N/A - darren is testing";
        self.inner
            .streaming_read(
                extensions,
                path,
                request,
                options,
                api_client_header,
                &x_goog_request_params,
            )
            .await
    }
}
