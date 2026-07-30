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

use crate::error::AppendResult;
use crate::model::AppendRowsResponse;
use crate::model::TableSchema;

/// The return type of an `append()` operation.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct AppendResponse {
    /// The row offset at which the last append occurred. The offset will not be
    /// set if appending using default streams.
    pub offset: Option<i64>,

    /// If set, the service reports that the table schema has changed.
    ///
    /// Note that this notification is best effort. Changing a table schema can
    /// take several minutes to propagate on the server side.
    ///
    /// The client library does not use this information to modify any internal
    /// state. It only forwards the notification to the application, which
    /// should react accordingly (if necessary).
    pub updated_schema: Option<TableSchema>,
}

pub(crate) fn to_result(resp: AppendRowsResponse) -> AppendResult<AppendResponse> {
    use crate::generated::gapic_storage::model::append_rows_response::Response::{
        AppendResult, Error,
    };
    // TODO : map stream_errors.
    let result = match resp.response {
        None => return Ok(AppendResponse::default()),
        Some(AppendResult(r)) => r,
        Some(Error(_)) => {
            // TODO : turn rpc::Status into a gax::Error.
            return Err(crate::Error::io("TODO").into());
        }
    };
    Ok(AppendResponse {
        offset: result.offset.map(|v| v as i64),
        updated_schema: resp.updated_schema,
    })
}
