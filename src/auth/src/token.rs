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

use time::OffsetDateTime;

use crate::{errors::CredentialError, Result};

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
use tokio::sync::{Mutex as AsyncMutex, Notify};
use std::time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct TokenCache<T>
where T: TokenProvider {
    // TODO : we just start with an expired token. That is probably easiest.
    token: Arc<RwLock<Token>>,

    // Tracks if a refresh is ongoing. If the lock is held, there is a refresh. We could use Mutex<i32> if we need to count past 1.
    refresh_in_progress: Arc<AsyncMutex<()>>,
    // Allows us to await the result of a refresh in multiple threads.
    refresh_notify: Arc<Notify>,

    // The token provider. This thing does the refreshing.
    inner: Arc<T>,
}

static TOKEN_VALID_DURATION: Duration = Duration::from_secs(6);
static TOKEN_REFRESH_WINDOW: Duration = Duration::from_secs(1);

fn expired(token: &Token) -> bool {
    token.expires_at.is_some_and(|e| e + TOKEN_REFRESH_WINDOW <= OffsetDateTime::now_utc())
}

fn stale(token: &Token) -> bool {
    token.expires_at.is_some_and(|e| e <= OffsetDateTime::now_utc())
}
//impl<T: TokenProvider + 'static> TokenCache<T> {
impl<T: TokenProvider> TokenCache<T> {
    pub fn new(inner: T) -> TokenCache<T> {
        TokenCache {
            token: Arc::new(RwLock::new(Token {
                token: String::new(),
                token_type: String::new(),
                expires_at: Some(OffsetDateTime::UNIX_EPOCH),
                metadata: None,
            })),
            refresh_in_progress: Arc::new(AsyncMutex::new(())),
            refresh_notify: Arc::new(Notify::new()),
            inner: Arc::new(inner),
        }
    }

    // Clones the current token, in a thread-safe manner. Releases the lock on return.
    fn current_token(&self) -> Token {
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
                    let t = self.inner.get_token().await?;
                    // We got a new token, store it.
                    *self.token.write().unwrap() = t;
                    // Notify any and all waiters
                    self.refresh_notify.notify_waiters();
                }
                Err(_) => {
                    // There is already a refresh
                    self.refresh_notify.notified().await;
                }
            }
        }

        else if stale(&token) {
            // Check if there are any outstanding refreshes...
            if let Ok(_guard) = self.refresh_in_progress.try_lock() {
                // No refreshes. We should start one, in the background.
                let inner = self.inner.clone();
                let token_data = self.token.clone();
                let refresh_notify = self.refresh_notify.clone();
                tokio::spawn(async move {
                    // Note that _guard has been moved into this closure.
                    let token = inner.get_token().await;
                    match token {
                        Ok(token) => {
                            // We got a new token, store it.
                            *token_data.write().unwrap() = token;
                            // Notify any and all waiters
                            refresh_notify.notify_waiters();
                        },
                        Err(_) => { /* do nothing */ },
                    }
                });
            }
        }

        // TODO : consider returning the token clone immediately if 
        Ok(self.current_token())
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
        let now = OffsetDateTime::now_utc();
        println!("s: {}, ns: {}", now.unix_timestamp(), now.unix_timestamp_nanos());

        let initial = Token {
            token: "initial-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(now + TOKEN_VALID_DURATION),
            metadata: None,
        };
        let initial_clone = initial.clone();

        let refresh = Token {
            token: "refresh-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(now + 2*TOKEN_VALID_DURATION),
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
        let sleep = TOKEN_VALID_DURATION + Duration::from_secs(1);
        tokio::time::advance(sleep).await;
        // DEBUG test expiry. We do not use Instant::Now, unfortunately.
        std::thread::sleep(sleep);


        // make sure this is the new token
        let actual = cache.get_token().await.unwrap();
        assert_eq!(actual, refresh);

    }
            
    #[tokio::test]
    async fn expired_token_failure() {}

    #[tokio::test]
    async fn stale_token_background_refresh_custom_window() {
        // We should probably be able to set the refresh window (e.g. to 0, meaning no refreshes) via options.
        // We might do this, for example, if we are in a single threaded runtime.
    }

    #[tokio::test]
    async fn stale_token_background_refresh_silent_failure() {
    }

    #[tokio::test]
    async fn stale_token_background_refresh_success() {
        // TODO : might also want a case for an immediate returning refresh. That is what happens if 
    }
    
    #[tokio::test]
    async fn initial_token_thundering_herd() {
        // I am not sure what is the behavior we want here. 1 total request?
        // Or just let these things race?
    }

    #[tokio::test]
    async fn background_refresh_thundering_herd() {
        // I am pretty sure we only want 1 outgoing request at a time here.
        // Maaaaaybe we want N outgoing requests?
    }
    
    #[tokio::test]
    async fn use_token_with_latest_expiry() {
        // I am not totally sure about this one. It makes sense only if we allow hedging.
        // And we could fall into the trap where the client has a token it thinks is valid based on expiration
        // But maybe it is not, in practice? Does this happen.
    }
}
