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

use crate::Result;
use crate::model::ReadObjectRequest;
use crate::read_object::ReadObjectResponse;
use crate::storage::checksum::details::Checksum;
use crate::storage::request_options::RequestOptions;

pub(crate) mod dynamic;

/// Defines the trait used to implement [crate::client::Storage].
///
/// Application developers may need to implement this trait to mock
/// `client::Storage`. In other use-cases, application developers only
/// use `client::Storage` and need not be concerned with this trait or
/// its implementations.
///
/// Services gain new RPCs routinely. Consequently, this trait gains new methods
/// too. To avoid breaking applications the trait provides a default
/// implementation of each method. Most of these implementations just return an
/// error.
pub trait Storage: std::fmt::Debug + Send + Sync {
    /// Implements [crate::client::Storage::read_object].
    fn read_object(
        &self,
        _req: ReadObjectRequest,
        _options: RequestOptions,
        _checksum: Checksum,
    ) -> impl std::future::Future<Output = Result<ReadObjectResponse>> + Send {
        unimplemented_stub::<ReadObjectResponse>()
    }
}

async fn unimplemented_stub<T>() -> gax::Result<T> {
    unimplemented!(concat!(
        "to prevent breaking changes as services gain new RPCs, the stub ",
        "traits provide default implementations of each method. In the client ",
        "libraries, all implementations of the traits override all methods. ",
        "Therefore, this error should not appear in normal code using the ",
        "client libraries. The only expected context for this error is test ",
        "code mocking the client libraries. If that is how you got this ",
        "error, verify that you have mocked all methods used in your test. ",
        "Otherwise, please open a bug at ",
        "https://github.com/googleapis/google-cloud-rust/issues"
    ));
}

// TODO : make all the relevant types public and
#[cfg(test)]
mod tests {
    use crate::model_ext::ObjectHighlights;

    use super::{Checksum, ReadObjectRequest, ReadObjectResponse, RequestOptions, Result};

    mockall::mock! {
        #[derive(Debug)]
        Storage {}
        impl crate::storage::stub::Storage for Storage {
            async fn read_object(&self, _req: ReadObjectRequest, _options: RequestOptions, _checksum: Checksum) -> Result<ReadObjectResponse>;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn mock_read_object_fail() {
        use gax::error::{
            Error,
            rpc::{Code, Status},
        };
        let mut mock = MockStorage::new();
        mock.expect_read_object()
            .times(1)
            .returning(|_, _, _| Err(Error::service(Status::default().set_code(Code::Aborted))));
        let client = crate::client::Storage::from_stub(mock);
        let _ = client
            .read_object("projects/_/buckets/my-bucket", "my-object")
            .send()
            .await
            .unwrap_err();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn mock_read_object_success() -> anyhow::Result<()> {
        use crate::model_ext::ObjectHighlights;
        use crate::streaming_source::Payload;
        const LAZY: &str = "the quick brown fox jumps over the lazy dog";
        let mut object = ObjectHighlights::default();
        object.etag = "darren".to_string();
        let object_clone = object.clone();

        let mut mock = MockStorage::new();
        mock.expect_read_object()
            .times(1)
            .returning(move |_, _, _| {
                Ok(ReadObjectResponse::from_source(
                    object_clone.clone(),
                    Payload::from(LAZY),
                ))
            });

        let client = crate::client::Storage::from_stub(mock);
        let mut reader = client
            .read_object("projects/_/buckets/my-bucket", "my-object")
            .send()
            .await?;
        assert_eq!(&object, &reader.object());

        let mut contents = Vec::new();
        while let Some(chunk) = reader.next().await.transpose()? {
            contents.extend_from_slice(&chunk);
        }
        let contents = bytes::Bytes::from_owner(contents);
        assert_eq!(contents, LAZY);
        Ok(())
    }
}
