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

use crate::read_object::ReadObjectResponse;
use crate::streaming_source::StreamingSource;
//use crate::streaming_source::dynamic::StreamingSource;
use super::{WriteObjectSpec, CommonObjectRequestParams, Object, RequestOptions, Checksum};
use crate::Result;

/// A dyn-compatible, crate-private version of [super::Storage].
#[async_trait::async_trait]
pub trait Storage: std::fmt::Debug + Send + Sync {
    async fn read_object(
        &self,
        req: crate::model::ReadObjectRequest,
        options: RequestOptions,
        checksum: Checksum,
    ) -> crate::Result<ReadObjectResponse>;

    async fn write_object_buffered<P>(
        &self,
        __payload: P,
         _checksum: Checksum,
         _spec: WriteObjectSpec,
         _params: Option<CommonObjectRequestParams>,
        __options: RequestOptions,
    ) -> Result<Object>
    where P: StreamingSource + Sized + Send + Sync + 'static;
}

/// All implementations of [super::Storage] also implement [Storage].
#[async_trait::async_trait]
impl<T: super::Storage> Storage for T {
    /// Forwards the call to the implementation provided by `T`.
    async fn read_object(
        &self,
        req: crate::model::ReadObjectRequest,
        options: RequestOptions,
        checksum: Checksum,
    ) -> crate::Result<ReadObjectResponse> {
        T::read_object(self, req, options, checksum).await
    }

    async fn write_object_buffered<P>(
        &self,
        payload: P,
        checksum: Checksum,
        spec: WriteObjectSpec,
        params: Option<CommonObjectRequestParams>,
        options: RequestOptions,
    ) -> Result<Object>
    where P: StreamingSource + Sized + Send + Sync + 'static
    {
        T::write_object_buffered(self, payload, checksum, spec, params, options).await
    }
}
