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

// TODO : we probably want separate fns on the StreamHandle.
/// The behavior on shutdown.
///
/// # Example
/// ```
/// todo!("write me.");
/// ```
///
/// The default behavior is WaitForProcessing.
#[derive(Clone, Copy, Debug)]
pub enum ShutdownBehavior {
    /// The subscriber will continue to accept acks for messages it has
    /// delivered to the application. The subscriber will continue to extend
    /// leases for these messages while it waits.
    ///
    /// The shutdown is complete when all message handles delivered to the
    /// application have been used, and all pending ack/nack RPCs have
    /// completed.
    WaitForProcessing,

    /// The subscriber flushes all pending acks from the application. No further
    /// acks are accepted. The lease loop is closed.
    ///
    /// All other messages under lease management are immediately nacked,
    /// regardless of if they have been returned to the application, or not.
    ///
    /// The shutdown is complete when all pending ack/nack RPCs have completed.
    NackImmediately,
}
