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

//! Google Cloud Client Libraries for Rust - Storage Control
//!
//! **WARNING:** this crate is under active development. We expect multiple
//! breaking changes in the upcoming releases. Testing is also incomplete, we do
//! **not** recommend that you use this crate in production. We welcome feedback
//! about the APIs, documentation, missing features, bugs, etc.
//!
//! This crate contains traits, types, and functions to interact with the
//! [Storage] control APIs.
//!
//! [storage]: https://cloud.google.com/storage

pub use gax::Result;
pub use gax::error::Error;
use gaxi::prost::ConvertError;
#[allow(dead_code)]
// TODO(#1813) - fix the broken link to `[here]`.
#[allow(rustdoc::broken_intra_doc_links)]
pub(crate) mod generated;

pub mod builder {
    // TODO(#1813) - Consider renaming this to storage_control
    pub mod storage {
        pub use crate::generated::gapic::builder::storage::*;
        pub use crate::generated::gapic_control::builder::storage_control::*;
    }
}
pub mod model {
    pub use crate::generated::gapic::model::*;
    pub use crate::generated::gapic_control::model::*;
}
// TODO(#1813) - Consider moving client and stub into a storage_control module
pub mod client;
pub mod stub;

pub(crate) mod google {
    pub mod iam {
        pub mod v1 {
            include!("generated/protos/storage/google.iam.v1.rs");
            include!("generated/convert/iam/convert.rs");
        }
    }
    pub mod longrunning {
        include!("generated/protos/control/google.longrunning.rs");
        include!("generated/convert/longrunning/convert.rs");
    }
    pub mod r#type {
        include!("generated/protos/storage/google.r#type.rs");
        include!("generated/convert/type/convert.rs");
    }
    pub mod rpc {
        include!("generated/protos/storage/google.rpc.rs");
    }
    pub mod storage {
        #[allow(deprecated)]
        pub mod v2 {
            include!("generated/protos/storage/google.storage.v2.rs");
            include!("generated/convert/storage/convert.rs");
        }
        pub mod control {
            pub mod v2 {
                include!("generated/protos/control/google.storage.control.v2.rs");
                include!("generated/convert/control/convert.rs");
            }
        }
    }
}

impl gaxi::prost::ToProto<google::rpc::Status> for rpc::model::Status {
    type Output = google::rpc::Status;
    fn to_proto(self) -> std::result::Result<google::rpc::Status, gaxi::prost::ConvertError> {
        Ok(google::rpc::Status {
            code: self.code.to_proto()?,
            message: self.message.to_proto()?,
            // TODO(#) - detail with the error details
            ..Default::default()
        })
    }
}

impl gaxi::prost::FromProto<rpc::model::Status> for google::rpc::Status {
    fn cnv(self) -> rpc::model::Status {
        rpc::model::Status::new()
            .set_code(self.code)
            .set_message(self.message)
        // TODO(#1699) - deal with the error details
        // .set_details(self.details.into_iter().filter_map(any_from_prost))
    }
}

// TODO : want unit tests for the conversion functions.
// NOTE : if we are going to hardcode LRO conversion in the generator, we should probably do the same for rpc::Status.
// There is a way to unify them with a macro that accepts the set of possible conversion types.

// TODO : Move this to `transport.rs`
impl gaxi::prost::ToProto<google::longrunning::Operation> for longrunning::model::Operation {
    type Output = google::longrunning::Operation;
    fn to_proto(
        self,
    ) -> std::result::Result<google::longrunning::Operation, gaxi::prost::ConvertError> {
        use crate::google::longrunning::operation::Result as U;
        use longrunning::model::operation::Result as T;
        let metadata = self
            .metadata
            .map(|metadata| any_to_prost_metadata(metadata))
            .transpose()?;
        let result = self
            .result
            .map(|result| match result {
                T::Error(status) => status.to_proto().map(|status| U::Error(status)),
                T::Response(any) => any_to_prost_result(*any).map(|any| U::Response(any)),
                _ => Err(ConvertError::Unimplemented),
            })
            .transpose()?;

        Ok(google::longrunning::Operation {
            name: self.name,
            metadata,
            done: self.done,
            result,
        })
    }
}

impl gaxi::prost::FromProto<longrunning::model::Operation> for google::longrunning::Operation {
    // TODO : we need to return a Result<T> if we are given an unknown type url.
    //        Creating a synthetic `error` seems wrong to me.
    fn cnv(self) -> longrunning::model::Operation {
        use crate::google::longrunning::operation::Result as U;
        use longrunning::model::operation::Result as T;
        let metadata = self.metadata.map_or_else(
            || wkt::Any::default(),
            |md| any_from_prost_metadata(md).unwrap(),
        );
        let result = self.result.map(|result| match result {
            U::Error(status) => T::Error(status.cnv().into()),
            U::Response(any) => T::Response(any_from_prost_result(any).unwrap().into()),
        });

        longrunning::model::Operation::new()
            .set_name(self.name)
            .set_metadata(metadata)
            .set_done(self.done)
            .set_result(result)
    }
}

pub(crate) fn any_from_prost_metadata(
    value: prost_types::Any,
) -> std::result::Result<wkt::Any, gaxi::prost::ConvertError> {
    use gaxi::prost::FromProto;
    match value.type_url.as_str() {
        "" => Ok(wkt::Any::default()),
        "type.googleapis.com/google.storage.control.v2.RenameFolderMetadata" => {
            let our_msg = value
                .to_msg::<google::storage::control::v2::RenameFolderMetadata>()
                .map_err(|_| ConvertError::Unimplemented)?
                .cnv();
            wkt::Any::try_from(&our_msg).map_err(|_| ConvertError::Unimplemented)
        }
        "type.googleapis.com/google.storage.control.v2.CreateAnywhereCacheMetadata" => {
            let our_msg = value
                .to_msg::<google::storage::control::v2::CreateAnywhereCacheMetadata>()
                .map_err(|_| ConvertError::Unimplemented)?
                .cnv();
            wkt::Any::try_from(&our_msg).map_err(|_| ConvertError::Unimplemented)
        }
        "type.googleapis.com/google.storage.control.v2.UpdateAnywhereCacheMetadata" => {
            let our_msg = value
                .to_msg::<google::storage::control::v2::UpdateAnywhereCacheMetadata>()
                .map_err(|_| ConvertError::Unimplemented)?
                .cnv();
            wkt::Any::try_from(&our_msg).map_err(|_| ConvertError::Unimplemented)
        }
        type_url => Err(ConvertError::UnexpectedTypeUrl(type_url.to_string())),
    }
}

pub(crate) fn any_to_prost_metadata(
    value: wkt::Any,
) -> std::result::Result<prost_types::Any, gaxi::prost::ConvertError> {
    use gaxi::prost::ToProto;
    match value.type_url().unwrap_or_default() {
        "" => Ok(prost_types::Any::default()),
        "type.googleapis.com/google.storage.control.v2.RenameFolderMetadata" => {
            let prost_msg = value
                .try_into_message::<crate::model::RenameFolderMetadata>()
                .map_err(|_| ConvertError::Unimplemented)?
                .to_proto()?;
            prost_types::Any::from_msg(&prost_msg).map_err(|_| ConvertError::Unimplemented)
        }
        "type.googleapis.com/google.storage.control.v2.CreateAnywhereCacheMetadata" => {
            let prost_msg = value
                .try_into_message::<crate::model::CreateAnywhereCacheMetadata>()
                .map_err(|_| ConvertError::Unimplemented)?
                .to_proto()?;
            prost_types::Any::from_msg(&prost_msg).map_err(|_| ConvertError::Unimplemented)
        }
        "type.googleapis.com/google.storage.control.v2.UpdateAnywhereCacheMetadata" => {
            let prost_msg = value
                .try_into_message::<crate::model::UpdateAnywhereCacheMetadata>()
                .map_err(|_| ConvertError::Unimplemented)?
                .to_proto()?;
            prost_types::Any::from_msg(&prost_msg).map_err(|_| ConvertError::Unimplemented)
        }
        type_url => Err(ConvertError::UnexpectedTypeUrl(type_url.to_string())),
    }
}

pub(crate) fn any_from_prost_result(
    value: prost_types::Any,
) -> std::result::Result<wkt::Any, gaxi::prost::ConvertError> {
    use gaxi::prost::FromProto;
    match value.type_url.as_str() {
        "" => Ok(wkt::Any::default()),
        "type.googleapis.com/google.storage.control.v2.Folder" => {
            let our_msg = value
                .to_msg::<google::storage::control::v2::Folder>()
                .map_err(|_| ConvertError::Unimplemented)?
                .cnv();
            wkt::Any::try_from(&our_msg).map_err(|_| ConvertError::Unimplemented)
        }
        "type.googleapis.com/google.storage.control.v2.AnywhereCache" => {
            let our_msg = value
                .to_msg::<google::storage::control::v2::AnywhereCache>()
                .map_err(|_| ConvertError::Unimplemented)?
                .cnv();
            wkt::Any::try_from(&our_msg).map_err(|_| ConvertError::Unimplemented)
        }
        type_url => Err(ConvertError::UnexpectedTypeUrl(type_url.to_string())),
    }
}

pub(crate) fn any_to_prost_result(
    value: wkt::Any,
) -> std::result::Result<prost_types::Any, gaxi::prost::ConvertError> {
    use gaxi::prost::ToProto;
    match value.type_url().unwrap_or_default() {
        "" => Ok(prost_types::Any::default()),
        "type.googleapis.com/google.storage.control.v2.Folder" => {
            let prost_msg = value
                .try_into_message::<crate::model::Folder>()
                .map_err(|_| ConvertError::Unimplemented)?
                .to_proto()?;
            prost_types::Any::from_msg(&prost_msg).map_err(|_| ConvertError::Unimplemented)
        }
        "type.googleapis.com/google.storage.control.v2.AnywhereCache" => {
            let prost_msg = value
                .try_into_message::<crate::model::CreateAnywhereCacheMetadata>()
                .map_err(|_| ConvertError::Unimplemented)?
                .to_proto()?;
            prost_types::Any::from_msg(&prost_msg).map_err(|_| ConvertError::Unimplemented)
        }
        type_url => Err(ConvertError::UnexpectedTypeUrl(type_url.to_string())),
    }
}
