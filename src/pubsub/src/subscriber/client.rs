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

use super::builder::Subscribe;
use super::transport::TransportStub;
use std::sync::Arc;

pub struct Subscriber {
    // TODO : this is probably abstracted into a stub? too early.
    inner: Arc<TransportStub>,
    // TODO : Radical idea: why even have a Subscriber client? What if we only had Subscriber session builders?
}

impl Subscriber {
    // TODO : play the ClientBuilder game
    pub async fn new() -> gax::client_builder::Result<Self> {
        let config = gaxi::options::ClientConfig::default();
        let transport = TransportStub::new(config).await?;
        Ok(Self {
            inner: Arc::new(transport),
        })
    }

    // Returns a result if we can't start the steram.
    pub fn subscribe<T>(&self, subscription: T) -> Subscribe
    where
        T: Into<String>,
    {
        Subscribe::new(self.inner.clone(), subscription.into())
    }
}
