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
use crate::model::{AcknowledgeRequest, ModifyAckDeadlineRequest};
use gax::options::RequestOptions;
use gax::retry_policy::NeverRetry;
use std::sync::Arc;

/// A trait representing leaser actions.
///
/// We stub out the interface, in order to test the lease management.
#[async_trait::async_trait]
pub(super) trait Leaser {
    /// Acknowledge a batch of messages.
    async fn ack(&self, subscription: String, ack_ids: Vec<String>);
    /// Negatively acknowledge a batch of messages.
    async fn nack(&self, subscription: String,ack_ids: Vec<String>);
    /// Extend lease deadlines for a batch of messages.
    async fn extend(&self,subscription: String, ack_deadline_seconds: i32, ack_ids: Vec<String>);
}

pub(super) struct DefaultLeaser<T>
where
    T: Stub,
{
    inner: Arc<T>,
}

impl<T> DefaultLeaser<T>
where
    T: Stub,
{
    pub(super) fn new(inner: Arc<T>) -> Self {
        DefaultLeaser {
            inner,
        }
    }
}

fn no_retry() -> RequestOptions {
    let mut o = RequestOptions::default();
    o.set_retry_policy(NeverRetry);
    o
}

#[async_trait::async_trait]
impl<T> Leaser for DefaultLeaser<T>
where
    T: Stub,
{
    async fn ack(&self, subscription: String, ack_ids: Vec<String>) {
        let req = AcknowledgeRequest::new()
            .set_subscription(subscription)
            .set_ack_ids(ack_ids);
        let _ = self.inner.acknowledge(req, no_retry()).await;
    }
    async fn nack(&self,subscription: String, ack_ids: Vec<String>) {
        let req = ModifyAckDeadlineRequest::new()
            .set_subscription(subscription)
            .set_ack_ids(ack_ids)
            .set_ack_deadline_seconds(0);
        let _ = self.inner.modify_ack_deadline(req, no_retry()).await;
    }
    async fn extend(&self,subscription: String, ack_deadline_seconds: i32, ack_ids: Vec<String>) {
        let req = ModifyAckDeadlineRequest::new()
            .set_subscription(subscription)
            .set_ack_ids(ack_ids)
            .set_ack_deadline_seconds(ack_deadline_seconds);
        let _ = self.inner.modify_ack_deadline(req, no_retry()).await;
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::lease_state::tests::test_ids;
    use super::super::stub::tests::MockStub;
    use super::*;
    use gax::response::Response;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    mockall::mock! {
        #[derive(Debug)]
        pub(in super::super) Leaser {}
        #[async_trait::async_trait]
        impl Leaser for Leaser {
            async fn ack(&self, subscription: String, ack_ids: Vec<String>);
            async fn nack(&self,  subscription: String,ack_ids: Vec<String>);
            async fn extend(&self, subscription: String, ack_deadline_seconds: i32, ack_ids: Vec<String>);
        }
    }

    #[async_trait::async_trait]
    impl Leaser for Arc<Mutex<MockLeaser>> {
        async fn ack(&self, subscription: String, ack_ids: Vec<String>) {
            self.lock().await.ack(subscription, ack_ids).await
        }
        async fn nack(&self, subscription: String, ack_ids: Vec<String>) {
            self.lock().await.nack(subscription, ack_ids).await
        }
        async fn extend(&self, subscription: String, ack_deadline_seconds: i32, ack_ids: Vec<String>) {
            self.lock().await.extend(subscription, ack_deadline_seconds, ack_ids).await
        }
    }

    #[tokio::test]
    async fn ack() {
        let mut mock = MockStub::new();
        mock.expect_acknowledge().times(1).return_once(|r, o| {
            assert_eq!(
                r.subscription,
                "projects/my-project/subscriptions/my-subscription"
            );
            assert_eq!(r.ack_ids, test_ids(0..10));
            assert!(
                format!("{o:?}").contains("NeverRetry"),
                "Basic acks should not have a retry policy. o={o:?}"
            );
            Ok(Response::from(()))
        });

        let leaser = DefaultLeaser::new(Arc::new(mock));
        leaser.ack(
            "projects/my-project/subscriptions/my-subscription".to_string(),
            test_ids(0..10)
        ).await;
    }

    #[tokio::test]
    async fn nack() {
        let mut mock = MockStub::new();
        mock.expect_modify_ack_deadline()
            .times(1)
            .return_once(|r, o| {
                assert_eq!(r.ack_deadline_seconds, 0);
                assert_eq!(
                    r.subscription,
                    "projects/my-project/subscriptions/my-subscription"
                );
                assert_eq!(r.ack_ids, test_ids(0..10));
                assert!(
                    format!("{o:?}").contains("NeverRetry"),
                    "Basic modacks should not have a retry policy. o={o:?}"
                );
                Ok(Response::from(()))
            });

        let leaser = DefaultLeaser::new(Arc::new(mock));
        leaser.nack(
            "projects/my-project/subscriptions/my-subscription".to_string(),
            test_ids(0..10)
        ).await;
    }

    #[tokio::test]
    async fn extend() {
        let mut mock = MockStub::new();
        mock.expect_modify_ack_deadline()
            .times(1)
            .return_once(|r, o| {
                assert_eq!(r.ack_deadline_seconds, 10);
                assert_eq!(
                    r.subscription,
                    "projects/my-project/subscriptions/my-subscription"
                );
                assert_eq!(r.ack_ids, test_ids(0..10));
                assert!(
                    format!("{o:?}").contains("NeverRetry"),
                    "Basic acks should not have a retry policy. o={o:?}"
                );
                Ok(Response::from(()))
            });

        let leaser = DefaultLeaser::new(Arc::new(mock));
        leaser.extend(
            "projects/my-project/subscriptions/my-subscription".to_string(),
            10,
            test_ids(0..10)
        ).await;
    }
}
