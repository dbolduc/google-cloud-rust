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

/// A wrapper over the proto message with ack/nack fns.
#[derive(Clone, Debug, Default)]
pub struct Message {
    pub data: bytes::Bytes,
    //pub attributes: HashMap<String, String>,
    pub message_id: String,
    //pub publish_time: wkt::Timestamp,
    //pub ordering_key: String,

    // NOTE : In C++, ack IDs are not associated with the public message type.
    // Ack IDs are not in the proto, but I think they would be associated with our messages.
    pub(crate) ack_id: String,
}

impl Message {
    // TODO : calling ack/nack should not consume self. But it should consume self.ack_id
    // Now I understand why C++ and others have a separate AckHandler / MessageConsumer type, vs. a built in.
    // We could achieve this with inner-mutability. But it seems nicer not to? That involves locks / heap allocations.
    // If we need Send + Sync safety, then maybe we already have it, though. Let me just proceed.
    pub async fn ack(mut self) {}
    pub async fn nack(mut self) {}
}
