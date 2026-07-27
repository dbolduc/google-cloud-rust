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

use crate::model::TableSchema;

/// The return type of an `append()` operation on the default stream.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct AppendResult {
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

/// The return type of an `append()` operation on an exclusive stream.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ExclusiveAppendResult {
    /// The row offset at which the last append occurred.
    pub offset: i64,

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
