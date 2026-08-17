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
use std::sync::Weak;
use std::time::Duration;
use tokio::time::interval;

/// Spawns a background watchdog task that periodically monitors and manages the stream pool.
///
/// It wakes up at the configured interval (e.g., every 5 seconds) to prune dead stream connections
/// and trigger rebalancing.
pub(crate) fn spawn_watchdog(
    pool: Weak<StreamPool>,
    interval_duration: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(interval_duration);
        loop {
            ticker.tick().await;
            if let Some(pool) = pool.upgrade() {
                pool.prune_dead_streams();
                pool.rebalance();
            } else {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::tests::test_transport;
    use std::sync::Arc;

    #[tokio::test]
    async fn watchdog_terminates_when_pool_dropped() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1".to_string()).await?);
        let pool = Arc::new(StreamPool::new(transport, 0));
        let weak_pool = Arc::downgrade(&pool);

        // Spawn watchdog with a short interval (e.g., 1ms)
        let handle = spawn_watchdog(weak_pool, Duration::from_millis(1));

        // Let the watchdog tick a few times while pool is alive
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(
            !handle.is_finished(),
            "watchdog should still be running while pool is alive"
        );

        // Drop the strong reference to pool
        drop(pool);

        // Now the watchdog should notice that pool was dropped and exit.
        // Wait for it with a timeout to avoid hangs if it leaks.
        tokio::time::timeout(Duration::from_millis(50), handle).await??;

        Ok(())
    }
}
