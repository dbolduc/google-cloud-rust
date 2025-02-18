// Copyright 2024 Google LLC
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

use crate::Result;

/// Represents an auth token.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    /// The actual token string.
    ///
    /// This is the value used in `Authorization:` header.
    pub token: String,

    /// The type of the token.
    ///
    /// The most common type is `"Bearer"` but other types may appear in the
    /// future.
    pub token_type: String,

    /// The instant at which the token expires.
    ///
    /// If `None`, the token does not expire or its expiration is unknown.
    ///
    /// Note that the `Instant` is not valid across processes. It is
    /// recommended to let the authentication library refresh tokens within a
    /// process instead of handling expirations yourself. If you do need to
    /// copy an expiration across processes, consider converting it to a
    /// `time::OffsetDateTime` first:
    ///
    /// ```
    /// # let expires_at = Some(std::time::Instant::now());
    /// expires_at.map(|i| time::OffsetDateTime::now_utc() + (i - std::time::Instant::now()));
    /// ```
    pub expires_at: Option<std::time::Instant>,

    /// Optional metadata associated with the token.
    ///
    /// This might include information like granted scopes or other claims.
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

#[async_trait::async_trait]
pub(crate) trait TokenProvider: std::fmt::Debug + Send + Sync {
    async fn get_token(&self) -> Result<Token>;
}

use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify};
// Using tokio's wrapper makes the cache testable without relying on clock times.
use tokio::time::Instant;

#[derive(Debug)]
pub(crate) struct TokenCache<T>
where
    T: TokenProvider,
{
    // The cached token, or the last seen error.
    token: Arc<RwLock<Result<Token>>>,

    // Tracks if a refresh is ongoing. If the lock is held, there is a refresh. We could use Mutex<i32> if we need to count past 1.
    refresh_in_progress: Arc<AsyncMutex<()>>,
    // Allows us to await the result of a refresh in multiple threads.
    refresh_notify: Arc<Notify>,

    // The token provider. This thing does the refreshing.
    inner: Arc<T>,
}

static TOKEN_REFRESH_WINDOW: Duration = Duration::from_secs(900); // 15 mins

// TODO : consider `invalid`
fn expired(token: &Result<Token>) -> bool {
    match token {
        Ok(t) => t.expires_at.is_some_and(|e| e <= Instant::now().into_std()),
        Err(_) => true,
    }
}

// TODO : consider `valid_but_expiring_soon`
fn stale(token: &Result<Token>) -> bool {
    match token {
        Ok(t) => t
            .expires_at
            .is_some_and(|e| e <= Instant::now().into_std() + TOKEN_REFRESH_WINDOW),
        Err(_) => true,
    }
}

// We manually implement the `Clone` trait because the Rust compiler will
// squawk if `T` is not `Clone`, even though we only hold an `Arc<T>`. :shrug:
impl<T: TokenProvider> Clone for TokenCache<T> {
    fn clone(&self) -> TokenCache<T> {
        TokenCache {
            token: self.token.clone(),
            refresh_in_progress: self.refresh_in_progress.clone(),
            refresh_notify: self.refresh_notify.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<T: TokenProvider> TokenCache<T> {
    pub fn new(inner: T) -> TokenCache<T> {
        TokenCache {
            token: Arc::new(RwLock::new(Err(crate::errors::CredentialError::retryable("No token in the cache. This should never happen. Something has gone wrong. Open an issue with google-cloud-rust.")))),
            refresh_in_progress: Arc::new(AsyncMutex::new(())),
            refresh_notify: Arc::new(Notify::new()),
            inner: Arc::new(inner),
        }
    }

    // Clones the current token, in a thread-safe manner. Releases the lock on return.
    fn current_token(&self) -> Result<Token> {
        self.token.read().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl<T: TokenProvider + 'static> TokenProvider for TokenCache<T> {
    async fn get_token(&self) -> Result<Token> {
        let token = self.current_token();

        if expired(&token) {
            match self.refresh_in_progress.try_lock() {
                // Check if there are any outstanding refreshes...
                Ok(_guard) => {
                    // No refreshes. We should start one
                    // Store the token, or an updated error
                    *self.token.write().unwrap() = self.inner.get_token().await;
                    // Notify any and all waiters
                    self.refresh_notify.notify_waiters();
                }
                Err(_) => {
                    // There is already a refresh
                    self.refresh_notify.notified().await;
                }
            }

            // If no errors have occured, the token has been written, return it.
            return self.current_token();
        } else if stale(&token) {
            // Check if there are any outstanding refreshes...
            if let Ok(_guard) = self.refresh_in_progress.try_lock() {
                // No refreshes. We should start one, in the background.
                let inner = self.inner.clone();
                let token_data = self.token.clone();
                let refresh_notify = self.refresh_notify.clone();
                tokio::spawn(async move {
                    // Note that `_guard` has been moved into this closure.
                    let token = inner.get_token().await;
                    match token {
                        Ok(token) => {
                            // We got a new token, store it.
                            *token_data.write().unwrap() = Ok(token);
                            // Notify any and all waiters
                            refresh_notify.notify_waiters();
                        }
                        Err(_) => { /* do nothing */ }
                    }
                });
            }
        }

        token
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub TokenProvider { }

        #[async_trait::async_trait]
        impl TokenProvider for TokenProvider {
            async fn get_token(&self) -> Result<Token>;
        }
    }

    use crate::errors::CredentialError;
    use std::sync::{Arc, Condvar, Mutex};

    // The standard token lifetime is 1 hour.
    static TOKEN_VALID_DURATION: Duration = Duration::from_secs(3600);

    #[tokio::test]
    async fn initial_token_success() {
        let expected = Token {
            token: "test-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: None,
            metadata: None,
        };
        let expected_clone = expected.clone();

        let mut mock = MockTokenProvider::new();
        mock.expect_get_token()
            .times(1)
            .return_once(|| Ok(expected_clone));

        let cache = TokenCache::new(mock);
        let actual = cache.get_token().await.unwrap();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn initial_token_failure() {
        let mut mock = MockTokenProvider::new();
        mock.expect_get_token()
            .times(1)
            .return_once(|| Err(CredentialError::non_retryable("fail")));

        let cache = TokenCache::new(mock);
        assert!(cache.get_token().await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn expired_token_success() {
        let now = Instant::now();

        let initial = Token {
            token: "initial-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some((now + TOKEN_VALID_DURATION).into_std()),
            metadata: None,
        };
        let initial_clone = initial.clone();

        let refresh = Token {
            token: "refresh-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some((now + 2 * TOKEN_VALID_DURATION).into_std()),
            metadata: None,
        };
        let refresh_clone = refresh.clone();

        let mut mock = MockTokenProvider::new();
        mock.expect_get_token()
            .times(1)
            .return_once(|| Ok(initial_clone));

        mock.expect_get_token()
            .times(1)
            .return_once(|| Ok(refresh_clone));

        // fetch an initial token
        let cache = TokenCache::new(mock);
        let actual = cache.get_token().await.unwrap();
        assert_eq!(actual, initial);

        // wait long enough for the token to be expired
        let sleep = TOKEN_VALID_DURATION;
        tokio::time::advance(sleep).await;

        // make sure this is the new token
        let actual = cache.get_token().await.unwrap();
        assert_eq!(actual, refresh);
    }

    #[tokio::test(start_paused = true)]
    async fn expired_token_failure() {
        let now = Instant::now();

        let initial = Token {
            token: "initial-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some((now + TOKEN_VALID_DURATION).into_std()),
            metadata: None,
        };
        let initial_clone = initial.clone();

        let mut mock = MockTokenProvider::new();
        mock.expect_get_token()
            .times(1)
            .return_once(|| Ok(initial_clone));

        mock.expect_get_token()
            .times(1)
            .return_once(|| Err(CredentialError::non_retryable("fail")));

        // fetch an initial token
        let cache = TokenCache::new(mock);
        let actual = cache.get_token().await.unwrap();
        assert_eq!(actual, initial);

        // wait long enough for the token to be expired
        let sleep = TOKEN_VALID_DURATION;
        tokio::time::advance(sleep).await;

        // make sure we return the error, not the expired token
        assert!(cache.get_token().await.is_err());
    }

    #[tokio::test]
    async fn stale_token_background_refresh_custom_window() {
        // We should probably be able to set the refresh window (e.g. to 0, meaning no refreshes) via options.
        // We might do this, for example, if we are in a single threaded runtime.
        assert!(false, "TODO: maybe defer. We need Options.");
    }

    #[tokio::test]
    async fn stale_token_background_refresh_success() {
        // TODO : might also want a case for an immediate returning refresh. That is what happens if
    }

    #[tokio::test]
    async fn stale_token_background_refresh_silent_failure() {}

    #[tokio::test]
    async fn stale_token_background_refresh_single_threaded() {
        // TODO : might also want a case for an immediate returning refresh. That is what happens if
    }

    // In this test, we start with a cache that has no token.
    // We spawn N=100 tasks that all ask for a token at once.
    // The underlying token provider waits for all N requests to be made.
    // When all tasks are ready, the token provider yields a single token.
    // All waiters receive this token from the cache.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn initial_token_thundering_herd_success() {
        let now = Instant::now();

        let token = Token {
            token: "initial-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some((now + TOKEN_VALID_DURATION).into_std()),
            metadata: None,
        };
        let token_clone = token.clone();

        let thundering_herd_ready = Arc::new((Mutex::new(false), Condvar::new()));
        let thundering_herd_ready_clone = thundering_herd_ready.clone();

        let mut mock = MockTokenProvider::new();
        // Note that we only expect one total call to the token provider.
        mock.expect_get_token().times(1).return_once(move || {
            // Do not serve a token until all N tasks are waiting.
            let (lock, cvar) = &*thundering_herd_ready_clone;
            let mut ready = lock.lock().unwrap();
            while !*ready {
                ready = cvar.wait(ready).unwrap();
            }
            Ok(token_clone)
        });

        let cache = TokenCache::new(mock);

        // Make the N requests for a token to the token cache. Note that we
        // must perform this work in a different thread than the mock token
        // provider's expectation, which will block on the condition variable
        // that waits for the tasks to be launched.
        let launch_tasks = tokio::spawn(async move {
            let tasks = (0..100)
                .map(|_| {
                    let cache_clone = cache.clone();
                    tokio::spawn(async move { cache_clone.get_token().await })
                })
                .collect::<Vec<_>>();

            // Notify the token provider that it should return its token.
            let (lock, cvar) = &*thundering_herd_ready;
            let mut ready = lock.lock().unwrap();
            *ready = true;
            cvar.notify_one();

            // Return the handles
            tasks
        });
        // Wait for the N token requests to start
        let tasks = launch_tasks.await.unwrap();

        // Wait for the N token requests to complete, verifying the returned token.
        for task in tasks {
            let actual = task.await.unwrap();
            assert!(actual.is_ok(), "{}", actual.err().unwrap());
            assert_eq!(actual.unwrap(), token);
        }
    }

    /*
    // In this test, we start with a cache that has no token.
    // We spawn N=100 tasks that all ask for a token at once.
    // The underlying token provider waits for all N requests to be made.
    // When all tasks are ready, the token provider yields a single error.
    // All waiters receive this error from the cache.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn initial_token_thundering_herd_failure_shares_error() {
        let now = Instant::now();

        let thundering_herd_ready = Arc::new((Mutex::new(false), Condvar::new()));
        let thundering_herd_ready_clone = thundering_herd_ready.clone();

        let mut mock = MockTokenProvider::new();
        // Note that we only expect one total call to the token provider.
        mock.expect_get_token().times(1).return_once(move || {
            // Do not serve a token until all N tasks are waiting.
            let (lock, cvar) = &*thundering_herd_ready;
            let mut ready = lock.lock().unwrap();
            while !*ready {
                ready = cvar.wait(ready).unwrap();
            }
            Err(CredentialError::non_retryable("fail"))
        });

        let cache = TokenCache::new(mock);

        // Make the N requests for a token to the token cache. Note that we
        // must perform this work in a different thread than the mock token
        // provider's expectation, which will block on the condition variable
        // that waits for the tasks to be launched.
        let launch_tasks = tokio::spawn(async move {
            let tasks = (0..100)
                .map(|_| {
                    let cache_clone = cache.clone();
                    tokio::spawn(async move { cache_clone.get_token().await })
                })
                .collect::<Vec<_>>();

            // Notify the token provider that it should return its token.
            let (lock, cvar) = &*thundering_herd_ready_clone;
            let mut ready = lock.lock().unwrap();
            *ready = true;
            cvar.notify_one();

            // Return the handles
            tasks
        });
        // Wait for the N token requests to start
        let tasks = launch_tasks.await.unwrap();

        // Wait for the N token requests to complete, verifying the returned token.
        for task in tasks {
            let actual = task.await.unwrap();
            assert!(actual.is_err(), "{:?}", actual.unwrap());
            let e = format!("{}", actual.err().unwrap());
            assert!(e.contains("fail"), "{e}");
        }
    }*/

    #[tokio::test]
    async fn background_refresh_thundering_herd_sends_one_request() {
        // I am pretty sure we only want 1 outgoing request at a time here.
        // Maaaaaybe we want N outgoing requests?
    }

    #[tokio::test]
    async fn use_token_with_latest_expiry() {
        // I am not totally sure about this one. It makes sense only if we allow hedging.
        // And we could fall into the trap where the client has a token it thinks is valid based on expiration
        // But maybe it is not, in practice? Does this happen.

        // Get a token
        // It is stale
        // Return a new token that has a worse expiry.
        // Confirm that we still use the first token
    }
}
