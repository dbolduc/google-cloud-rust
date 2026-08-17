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

use crate::pool::StreamPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

/// Spawns a background watchdog task that periodically monitors and manages the stream pool.
///
/// It wakes up at the configured interval (e.g., every 5 seconds) to prune dead stream connections
/// and trigger rebalancing.
pub(crate) fn spawn_watchdog(
    pool: Arc<StreamPool>,
    interval_duration: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(interval_duration);
        loop {
            ticker.tick().await;
            pool.prune_dead_streams();
            pool.rebalance();
        }
    })
}
