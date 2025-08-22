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

use crate::{Error, Result};
use crate::error::ReadError;
use crate::model::ReadObjectRequest;
use crate::model::ObjectChecksums;
use crate::model_ext::ObjectHighlights;
use crate::read_object_response::ReadObjectResponse;
use crate::storage::checksum::{
    ChecksumEngine,
    details::validate,
};
use crate::storage::client::{apply_customer_supplied_encryption_headers, enc, info};
use crate::storage::request_options::RequestOptions;
use crate::storage::v1;
use base64::Engine;
use serde_with::DeserializeAs;
use std::sync::Arc;
use super::client::StorageInner;
#[cfg(feature = "unstable-stream")]
use futures::Stream;

#[derive(Clone, Debug)]
pub(crate) struct Storage {
    inner: Arc<StorageInner>,
}

impl super::stub::Storage for Storage {
    async fn read_object(
        &self,
        req: ReadObjectRequest,
        options: RequestOptions,
    ) -> Result<impl ReadObjectResponse> {
        ReadObjectResponseImpl::new(Arc::new(self.clone()), req, options).await
    }
}

impl Storage {
    pub fn new(inner: Arc<StorageInner>) -> Arc<Self> {
        Arc::new(Self { inner })
    }

    async fn read(&self, req: &ReadObjectRequest, options: &RequestOptions) -> Result<reqwest::Response> {
        let throttler = options.retry_throttler.clone();
        let retry = options.retry_policy.clone();
        let backoff = options.backoff_policy.clone();

        gax::retry_loop_internal::retry_loop(
            async move |_| self.read_attempt(req.clone()).await,
            async |duration| tokio::time::sleep(duration).await,
            true,
            throttler,
            retry,
            backoff,
        )
        .await
    }

    async fn read_attempt(&self, request: ReadObjectRequest) -> Result<reqwest::Response> {
        let builder = self.http_request_builder(request).await?;
        let response = builder.send().await.map_err(Error::io)?;
        if !response.status().is_success() {
            return gaxi::http::to_http_error(response).await;
        }
        Ok(response)
    }

    async fn http_request_builder(&self, request: ReadObjectRequest) -> Result<reqwest::RequestBuilder> {
        // Collect the required bucket and object parameters.
        let bucket = &request.bucket;
        let bucket_id = bucket
            .as_str()
            .strip_prefix("projects/_/buckets/")
            .ok_or_else(|| {
                Error::binding(format!(
                    "malformed bucket name, it must start with `projects/_/buckets/`: {bucket}"
                ))
            })?;
        let object = &request.object;

        // Build the request.
        let builder = self
            .inner
            .client
            .request(
                reqwest::Method::GET,
                format!(
                    "{}/storage/v1/b/{bucket_id}/o/{}",
                    &self.inner.endpoint,
                    enc(object)
                ),
            )
            .query(&[("alt", "media")])
            .header(
                "x-goog-api-client",
                reqwest::header::HeaderValue::from_static(&self::info::X_GOOG_API_CLIENT_HEADER),
            );

        // Add the optional query parameters.
        let builder = if request.generation != 0 {
            builder.query(&[("generation", request.generation)])
        } else {
            builder
        };
        let builder = request
            .if_generation_match
            .iter()
            .fold(builder, |b, v| b.query(&[("ifGenerationMatch", v)]));
        let builder = request
            .if_generation_not_match
            .iter()
            .fold(builder, |b, v| b.query(&[("ifGenerationNotMatch", v)]));
        let builder = request
            .if_metageneration_match
            .iter()
            .fold(builder, |b, v| b.query(&[("ifMetagenerationMatch", v)]));
        let builder = request
            .if_metageneration_not_match
            .iter()
            .fold(builder, |b, v| b.query(&[("ifMetagenerationNotMatch", v)]));

        let builder = apply_customer_supplied_encryption_headers(
            builder,
            &request.common_object_request_params,
        );

        // Apply "range" header for read limits and offsets.
        let builder = match (request.read_offset, request.read_limit) {
            // read_limit can't be negative.
            (_, l) if l < 0 => {
                unreachable!("ReadObject build never sets a negative read_limit value")
            }
            // negative offset can't also have a read_limit.
            (o, l) if o < 0 && l > 0 => unreachable!(
                "ReadObject builder never sets a positive read_offset value with a negative read_limit value"
            ),
            // If both are zero, we use default implementation (no range header).
            (0, 0) => builder,
            // negative offset with no limit means the last N bytes.
            (o, 0) if o < 0 => builder.header("range", format!("bytes={o}")),
            // read_limit is zero, means no limit. Read from offset to end of file.
            // This handles cases like (5, 0) -> "bytes=5-"
            (o, 0) => builder.header("range", format!("bytes={o}-")),
            // General case: non-negative offset and positive limit.
            // This covers cases like (0, 100) -> "bytes=0-99", (5, 100) -> "bytes=5-104"
            (o, l) => builder.header("range", format!("bytes={o}-{}", o + l - 1)),
        };

        self.inner.apply_auth_headers(builder).await
    }
}

fn headers_to_crc32c(headers: &http::HeaderMap) -> Option<u32> {
    headers
        .get("x-goog-hash")
        .and_then(|hash| hash.to_str().ok())
        .and_then(|hash| hash.split(",").find(|v| v.starts_with("crc32c")))
        .and_then(|hash| {
            let hash = hash.trim_start_matches("crc32c=");
            v1::Crc32c::deserialize_as(serde_json::json!(hash)).ok()
        })
}

fn headers_to_md5_hash(headers: &http::HeaderMap) -> Vec<u8> {
    headers
        .get("x-goog-hash")
        .and_then(|hash| hash.to_str().ok())
        .and_then(|hash| hash.split(",").find(|v| v.starts_with("md5")))
        .and_then(|hash| {
            let hash = hash.trim_start_matches("md5=");
            base64::prelude::BASE64_STANDARD.decode(hash).ok()
        })
        .unwrap_or_default()
}

/// A response to a [Storage::read_object] request.
#[derive(Debug)]
struct ReadObjectResponseImpl<C>
where C: ChecksumEngine + Send + Sync + 'static {
    stub: Arc<Storage>,
    inner: Option<reqwest::Response>,
    highlights: ObjectHighlights,
    // Fields for tracking the crc checksum checks.
    response_checksums: ObjectChecksums,
    // Fields for resuming a read request.
    range: ReadRange,
    generation: i64,
    request: ReadObjectRequest,
    options: RequestOptions,
    checksum: C,
    resume_count: u32,
}

impl<C> ReadObjectResponseImpl<C>
where
    C: ChecksumEngine + Clone + Send + Sync + 'static,
{
    async fn new(stub: Arc<Storage>, request: ReadObjectRequest, options: RequestOptions) -> Result<Self> {
        let inner = stub.read(&request, &options).await?;

        let full = request.read_offset == 0 && request.read_limit == 0;
        let response_checksums = checksums_from_response(full, inner.status(), inner.headers());
        let range = response_range(&inner).map_err(Error::deser)?;
        let generation = response_generation(&inner).map_err(Error::deser)?;

        let headers = inner.headers();
        let get_as_i64 = |header_name: &str| -> i64 {
            headers
                .get(header_name)
                .and_then(|s| s.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or_default()
        };
        let get_as_string = |header_name: &str| -> String {
            headers
                .get(header_name)
                .and_then(|sc| sc.to_str().ok())
                .map(|sc| sc.to_string())
                .unwrap_or_default()
        };
        let highlights = ObjectHighlights {
            generation,
            metageneration: get_as_i64("x-goog-metageneration"),
            size: get_as_i64("x-goog-stored-content-length"),
            content_encoding: get_as_string("x-goog-stored-content-encoding"),
            storage_class: get_as_string("x-goog-storage-class"),
            content_type: get_as_string("content-type"),
            content_language: get_as_string("content-language"),
            content_disposition: get_as_string("content-disposition"),
            etag: get_as_string("etag"),
            checksums: headers.get("x-goog-hash").map(|_| {
                crate::model::ObjectChecksums::new()
                    .set_or_clear_crc32c(headers_to_crc32c(headers))
                    .set_md5_hash(headers_to_md5_hash(headers))
            }),
        };

        Ok(Self {
            stub,
            inner: Some(inner),
            highlights,
            // Fields for computing checksums.
            response_checksums,
            // Fields for resuming a read request.
            range,
            generation,
            request,
            options,
            resume_count: 0,
        })
    }
}

impl<C> ReadObjectResponse for ReadObjectResponseImpl<C>
where
    C: ChecksumEngine + Clone + Send + Sync + 'static,
{
    fn object(&self) -> ObjectHighlights {
        self.highlights.clone()
    }

    // A type-checking cycle is detected with `async fn` when its return type
    // depends on an opaque type that is defined within the function body.
    // Writing out `impl Future` breaks this cycle, allowing the compiler to
    // resolve the return type and proceed.
    #[allow(clippy::manual_async_fn)]
    fn next(&mut self) -> impl Future<Output = Option<Result<bytes::Bytes>>> + Send {
        async move {
            match self.next_attempt().await {
                None => None,
                Some(Ok(b)) => Some(Ok(b)),
                // Recursive async requires pin:
                //     https://rust-lang.github.io/async-book/07_workarounds/04_recursion.html
                Some(Err(e)) => Box::pin(self.resume(e)).await,
            }
        }
    }

    #[cfg(feature = "unstable-stream")]
    #[cfg_attr(docsrs, doc(cfg(feature = "unstable-stream")))]
    fn into_stream(self) -> impl Stream<Item = Result<bytes::Bytes>> + Unpin {
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

impl<C> ReadObjectResponseImpl<C>
where
    C: ChecksumEngine + Clone + Send + Sync + 'static,
{
    async fn next_attempt(&mut self) -> Option<Result<bytes::Bytes>> {
        let inner = self.inner.as_mut()?;
        let res = inner.chunk().await.map_err(Error::io);
        match res {
            Ok(Some(chunk)) => {
                self.checksum.update(self.range.start, &chunk);
                let len = chunk.len() as u64;
                if self.range.limit < len {
                    return Some(Err(Error::deser(ReadError::LongRead {
                        expected: self.range.limit,
                        got: len,
                    })));
                }
                self.range.limit -= len;
                self.range.start += len;
                Some(Ok(chunk))
            }
            Ok(None) => {
                if self.range.limit != 0 {
                    return Some(Err(Error::io(ReadError::ShortRead(self.range.limit))));
                }
                let computed = self.checksum.finalize();
                let res = validate(&self.response_checksums, &Some(computed));
                match res {
                    Err(e) => Some(Err(Error::deser(ReadError::ChecksumMismatch(e)))),
                    Ok(()) => None,
                }
            }
            Err(e) => Some(Err(e)),
        }
    }

    async fn resume(&mut self, error: Error) -> Option<Result<bytes::Bytes>> {
        use crate::read_resume_policy::{ResumeQuery, ResumeResult};

        // The existing read is no longer valid.
        self.inner = None;
        self.resume_count += 1;
        let query = ResumeQuery::new(self.resume_count);
        match self
            .options
            .read_resume_policy
            .on_error(&query, error)
        {
            ResumeResult::Continue(_) => {}
            ResumeResult::Permanent(e) => return Some(Err(e)),
            ResumeResult::Exhausted(e) => return Some(Err(e)),
        };
        self.request.read_offset = self.range.start as i64;
        self.request.read_limit = self.range.limit as i64;
        self.request.generation = self.generation;
        self.inner = match self.stub.read(self.request).await {
            Ok(r) => Some(r),
            Err(e) => return Some(Err(e)),
        };
        self.next().await
    }
}

/// Returns the object checksums to validate against.
///
/// For some responses, the checksums are not expected to match the data.
/// The function returns an empty `ObjectChecksums` in such a case.
///
/// Checksum validation is supported iff:
/// 1. We requested the full content.
/// 2. We got all the content (status != PartialContent).
/// 3. The server sent a CRC header.
/// 4. The http stack did not uncompress the file.
/// 5. We were not served compressed data that was uncompressed on read.
///
/// For 4, we turn off automatic decompression in reqwest::Client when we
/// create it,
fn checksums_from_response(
    full_content_requested: bool,
    status: http::StatusCode,
    headers: &http::HeaderMap,
) -> ObjectChecksums {
    let checksums = ObjectChecksums::new();
    if !full_content_requested || status == http::StatusCode::PARTIAL_CONTENT {
        return checksums;
    }
    let stored_encoding = headers
        .get("x-goog-stored-content-encoding")
        .and_then(|e| e.to_str().ok())
        .map_or("", |e| e);
    let content_encoding = headers
        .get("content-encoding")
        .and_then(|e| e.to_str().ok())
        .map_or("", |e| e);
    if stored_encoding == "gzip" && content_encoding != "gzip" {
        return checksums;
    }
    checksums
        .set_or_clear_crc32c(headers_to_crc32c(headers))
        .set_md5_hash(headers_to_md5_hash(headers))
}

fn response_range(response: &reqwest::Response) -> std::result::Result<ReadRange, ReadError> {
    match response.status() {
        reqwest::StatusCode::OK => {
            let header = required_header(response, "content-length")?;
            let limit = header
                .parse::<u64>()
                .map_err(|e| ReadError::BadHeaderFormat("content-length", e.into()))?;
            Ok(ReadRange { start: 0, limit })
        }
        reqwest::StatusCode::PARTIAL_CONTENT => {
            let header = required_header(response, "content-range")?;
            let header = header.strip_prefix("bytes ").ok_or_else(|| {
                ReadError::BadHeaderFormat("content-range", "missing bytes prefix".into())
            })?;
            let (range, _) = header.split_once('/').ok_or_else(|| {
                ReadError::BadHeaderFormat("content-range", "missing / separator".into())
            })?;
            let (start, end) = range.split_once('-').ok_or_else(|| {
                ReadError::BadHeaderFormat("content-range", "missing - separator".into())
            })?;
            let start = start
                .parse::<u64>()
                .map_err(|e| ReadError::BadHeaderFormat("content-range", e.into()))?;
            let end = end
                .parse::<u64>()
                .map_err(|e| ReadError::BadHeaderFormat("content-range", e.into()))?;
            // HTTP ranges are inclusive, we need to compute the number of bytes
            // in the range:
            let end = end + 1;
            let limit = end
                .checked_sub(start)
                .ok_or_else(|| ReadError::BadHeaderFormat("content-range", format!("range start ({start}) should be less than or equal to the range end ({end})").into()))?;
            Ok(ReadRange { start, limit })
        }
        s => Err(ReadError::UnexpectedSuccessCode(s.as_u16())),
    }
}

fn response_generation(response: &reqwest::Response) -> std::result::Result<i64, ReadError> {
    let header = required_header(response, "x-goog-generation")?;
    header
        .parse::<i64>()
        .map_err(|e| ReadError::BadHeaderFormat("x-goog-generation", e.into()))
}

fn required_header<'a>(
    response: &'a reqwest::Response,
    name: &'static str,
) -> std::result::Result<&'a str, ReadError> {
    let header = response
        .headers()
        .get(name)
        .ok_or_else(|| ReadError::MissingHeader(name))?;
    header
        .to_str()
        .map_err(|e| ReadError::BadHeaderFormat(name, e.into()))
}

#[derive(Debug, PartialEq)]
struct ReadRange {
    start: u64,
    limit: u64,
}
