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

//! Defines the return interface for [Storage::read_object][crate::client::Storage::read_object]

use crate::Result;
use crate::model_ext::ObjectHighlights;
use crate::streaming_source::StreamingSource;
#[cfg(feature = "unstable-stream")]
use futures::Stream;

/// The result of a `ReadObject` RPC.
///
/// A stream of data to be iterated over, with an option to retrieve the objec
/// metadata.
#[derive(Debug)]
pub struct ReadObjectResponse {
    inner: Box<dyn Darren>,
}

impl ReadObjectResponse {
    pub(crate) fn from_inner<T: Darren + 'static>(inner: Box<T>) -> Self {
        Self { inner }
    }

    // Useful for mocking
    pub fn from_source<P>(object: ObjectHighlights, source: P) -> Self
    where
        P: StreamingSource + Send + 'static,
    {
        Self {
            inner: Box::new(FakeReadObjectResponse { object, source }),
        }
    }

    /// Get the highlights of the object metadata included in the
    /// response.
    ///
    /// To get full metadata about this object, use [crate::client::StorageControl::get_object].
    ///
    /// # Example
    /// ```
    /// # use google_cloud_storage::client::Storage;
    /// # async fn sample(client: &Storage) -> anyhow::Result<()> {
    /// use google_cloud_storage::read_object::ReadObjectResponse;
    /// let object = client
    ///     .read_object("projects/_/buckets/my-bucket", "my-object")
    ///     .send()
    ///     .await?
    ///     .object();
    /// println!("object generation={}", object.generation);
    /// println!("object metageneration={}", object.metageneration);
    /// println!("object size={}", object.size);
    /// println!("object content encoding={}", object.content_encoding);
    /// # Ok(()) }
    /// ```
    pub fn object(&self) -> ObjectHighlights {
        self.inner.object()
    }

    /// Stream the next bytes of the object.
    ///
    /// When the response has been exhausted, this will return None.
    ///
    /// # Example
    /// ```
    /// # use google_cloud_storage::client::Storage;
    /// # async fn sample(client: &Storage) -> anyhow::Result<()> {
    /// use google_cloud_storage::read_object::ReadObjectResponse;
    /// let mut resp = client
    ///     .read_object("projects/_/buckets/my-bucket", "my-object")
    ///     .send()
    ///     .await?;
    /// while let Some(next) = resp.next().await {
    ///     println!("next={:?}", next?);
    /// }
    /// # Ok(()) }
    /// ```
    pub fn next(&mut self) -> impl Future<Output = Option<Result<bytes::Bytes>>> + Send {
        self.inner.next()
    }

    #[cfg(feature = "unstable-stream")]
    #[cfg_attr(docsrs, doc(cfg(feature = "unstable-stream")))]
    pub fn into_stream(self) -> impl Stream<Item = Result<bytes::Bytes>> + Unpin
    where
        Self: Sized,
    {
        use futures::stream::unfold;
        Box::pin(unfold(Some(self), move |state| async move {
            if let Some(mut this) = state {
                if let Some(chunk) = this.next().await {
                    return Some((chunk, Some(this)));
                }
            };
            None
        }))
    }
}

#[async_trait::async_trait]
pub(crate) trait Darren: std::fmt::Debug {
    fn object(&self) -> ObjectHighlights;
    async fn next(&mut self) -> Option<Result<bytes::Bytes>>;
}

struct FakeReadObjectResponse<P>
where
    P: StreamingSource + Send,
{
    object: ObjectHighlights,
    source: P,
}

impl<P> std::fmt::Debug for FakeReadObjectResponse<P>
where
    P: StreamingSource + Send,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[async_trait::async_trait]
impl<P> crate::read_object::Darren for FakeReadObjectResponse<P>
where
    P: crate::streaming_source::StreamingSource + Send,
{
    fn object(&self) -> ObjectHighlights {
        self.object.clone()
    }

    async fn next(&mut self) -> Option<Result<bytes::Bytes>> {
        self.source
            .next()
            .await
            .transpose()
            .map_err(gax::error::Error::io)
            .transpose()
    }
}
