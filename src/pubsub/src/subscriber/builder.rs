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

use super::model::Message;
use super::session::SubscribeSession;
use super::transport::TransportStub;
use std::sync::Arc;

/// Builder for the `client::Subscriber::subscribe` method.
pub struct Subscribe<C, F>
where
    C: Fn(Message) -> F + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    pub(crate) inner: Arc<TransportStub>,
    pub(crate) subscription: String,
    pub(crate) callback: C,
}

impl<C, F> Subscribe<C, F>
where
    C: Fn(Message) -> F + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    pub(crate) fn new(inner: Arc<TransportStub>, subscription: String, callback: C) -> Self {
        Subscribe {
            inner,
            subscription,
            callback,
        }
    }

    /// Starts a subscribe session.
    ///
    /// Returns a result if we can't start the stream.
    pub async fn start(self) -> crate::Result<SubscribeSession> {
        SubscribeSession::new(self).await
    }

    // Your typical request field setters

    pub fn set_stream_ack_deadline_seconds(mut self, s: i32) -> Self {
        // Aside: I hate that "seconds" is in the name.
        // Consider accepting a Duration and rounding to nearest second.
        unimplemented!();
    }
    pub fn set_max_outstanding_messages(mut self, n: i64) -> Self {
        unimplemented!();
    }
    pub fn set_max_outstanding_bytes(mut self, n: i64) -> Self {
        unimplemented!();
    }

    // pub fn with_whatever_other_options() -> Self
}
