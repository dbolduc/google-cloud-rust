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

use super::SizeHint;

/// A dyn-compatible, crate-private version of [super::StreamingSource].
#[async_trait::async_trait]
pub trait StreamingSource {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn next(&mut self) -> Option<Result<bytes::Bytes, Self::Error>>;

    async fn size_hint(&self) -> Result<SizeHint, Self::Error> {
        Ok(SizeHint::new())
    }
}

/// All implementations of [super::StreamingSource] also implement [StreamingSource].
#[async_trait::async_trait]
impl<T: super::StreamingSource + Send + Sync> StreamingSource for T {
    type Error = T::Error;

    async fn next(&mut self) ->Option<Result<bytes::Bytes, Self::Error>> {
        T::next(self).await
    }
    async fn size_hint(&self) -> Result<SizeHint, Self::Error> {
        T::size_hint(self).await
    }
}
