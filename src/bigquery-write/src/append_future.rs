// Copyright 2026 Google LLC
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

use crate::errors::{AppendError, AppendResult};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::sync::oneshot;

type Response = AppendResult<AppendResponse>;

/// A [`Future`] representing the result of an append operation.
///
/// The client library accepts rows to append. Requests with an offset are
/// ordered, and the client library must deliver them to the server in order. 
///
/// TODO : write the documentation. Say words about why we have a custom future
/// and how application should await the requests it sends.
///
/// TODO : write an example.
pub struct AppendFuture {
    pub(crate) rx: oneshot::Receiver<Response>,
}

impl Future for AppendFuture {
    /// The result of an append operation.
    type Output = Response;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = ready!(Pin::new(&mut self.rx).poll(cx));
            match result {
                Ok(result) => Poll::Ready(result),
                Err(_) => Poll::Ready(Err(crate::error::AppendError::UnexpectedEndOfStream)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_append_future_success() -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let handle = AppendFuture { rx };
        let _ = tx.send(Ok("message_id".to_string()));
        assert_eq!(handle.await?, "message_id");

        Ok(())
    }

    #[tokio::test]
    async fn resolve_append_future_error() -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let fut = AppendFuture { rx };
        let _ = tx.send(Err(crate::error::AppendError::OrderingKeyPaused));
        let res = fut.await;
        assert!(
            matches!(res, Err(crate::error::AppendError::OrderingKeyPaused)),
            "{res:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn resolve_append_future_error_send_error() -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let fut = AppendFuture { rx };
        drop(tx);
        let res = fut.await;
        assert!(
            matches!(res, Err(crate::error::AppendError::UnexpectedEndOfStream)),
            "{res:?}"
        );

        Ok(())
    }
}
