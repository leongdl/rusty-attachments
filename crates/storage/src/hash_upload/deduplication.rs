//! Hash deduplication for pipelined uploads.
//!
//! Tracks in-flight uploads to prevent duplicate uploads of the same hash.
//! When multiple files have the same content (same hash), only one upload
//! is performed. Other files wait for the first upload to complete.

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

/// Tracks in-flight uploads to prevent duplicate uploads of the same hash.
///
/// When multiple files have the same content (same hash), only one upload
/// is performed. Other files wait for the first upload to complete.
pub struct UploadDeduplicator {
    /// Map of hash -> broadcast sender for completion notification.
    in_flight: Mutex<HashMap<String, broadcast::Sender<UploadResult>>>,
}

/// Result of a deduplicated upload operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadResult {
    /// Upload completed successfully.
    Success,
    /// Upload failed.
    Failed,
}

/// Intent returned when registering an upload.
pub enum UploadIntent {
    /// Proceed with upload (first uploader).
    Proceed,
    /// Wait for existing upload to complete.
    Wait(broadcast::Receiver<UploadResult>),
}

impl UploadDeduplicator {
    /// Create a new deduplicator.
    pub fn new() -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// Register intent to upload a hash.
    ///
    /// # Arguments
    /// * `hash` - Content hash to upload
    ///
    /// # Returns
    /// - `UploadIntent::Proceed` if this is the first uploader
    /// - `UploadIntent::Wait(receiver)` if another upload is in progress
    pub fn register(&self, hash: &str) -> UploadIntent {
        let mut in_flight = self.in_flight.lock().unwrap();

        if let Some(sender) = in_flight.get(hash) {
            // Another upload in progress, subscribe to completion
            UploadIntent::Wait(sender.subscribe())
        } else {
            // First uploader, register and proceed
            let (sender, _) = broadcast::channel(1);
            in_flight.insert(hash.to_string(), sender);
            UploadIntent::Proceed
        }
    }

    /// Mark upload as complete.
    ///
    /// Notifies all waiters that the upload is done.
    ///
    /// # Arguments
    /// * `hash` - Content hash that was uploaded
    pub fn complete(&self, hash: &str) {
        let mut in_flight = self.in_flight.lock().unwrap();
        if let Some(sender) = in_flight.remove(hash) {
            // Notify all waiters (ignore errors if no receivers)
            let _ = sender.send(UploadResult::Success);
        }
    }

    /// Mark upload as failed.
    ///
    /// Removes the registration so another uploader can try.
    ///
    /// # Arguments
    /// * `hash` - Content hash that failed to upload
    pub fn failed(&self, hash: &str) {
        let mut in_flight = self.in_flight.lock().unwrap();
        if let Some(sender) = in_flight.remove(hash) {
            // Notify waiters of failure
            let _ = sender.send(UploadResult::Failed);
        }
    }

    /// Get the number of in-flight uploads.
    #[cfg(test)]
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.lock().unwrap().len()
    }
}

impl Default for UploadDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_uploader_proceeds() {
        let dedup = UploadDeduplicator::new();

        match dedup.register("hash1") {
            UploadIntent::Proceed => {}
            UploadIntent::Wait(_) => panic!("Expected Proceed"),
        }

        assert_eq!(dedup.in_flight_count(), 1);
    }

    #[test]
    fn test_second_uploader_waits() {
        let dedup = UploadDeduplicator::new();

        // First uploader
        match dedup.register("hash1") {
            UploadIntent::Proceed => {}
            UploadIntent::Wait(_) => panic!("Expected Proceed"),
        }

        // Second uploader should wait
        match dedup.register("hash1") {
            UploadIntent::Proceed => panic!("Expected Wait"),
            UploadIntent::Wait(_) => {}
        }
    }

    #[tokio::test]
    async fn test_complete_notifies_waiters() {
        let dedup = UploadDeduplicator::new();

        // First uploader
        match dedup.register("hash1") {
            UploadIntent::Proceed => {}
            UploadIntent::Wait(_) => panic!("Expected Proceed"),
        }

        // Second uploader gets receiver
        let mut receiver = match dedup.register("hash1") {
            UploadIntent::Proceed => panic!("Expected Wait"),
            UploadIntent::Wait(r) => r,
        };

        // Complete the upload
        dedup.complete("hash1");

        // Waiter should receive success
        let result = receiver.recv().await.unwrap();
        assert_eq!(result, UploadResult::Success);

        // No more in-flight
        assert_eq!(dedup.in_flight_count(), 0);
    }

    #[tokio::test]
    async fn test_failed_allows_retry() {
        let dedup = UploadDeduplicator::new();

        // First uploader
        match dedup.register("hash1") {
            UploadIntent::Proceed => {}
            UploadIntent::Wait(_) => panic!("Expected Proceed"),
        }

        // Fail the upload
        dedup.failed("hash1");

        // Next uploader should be able to proceed
        match dedup.register("hash1") {
            UploadIntent::Proceed => {}
            UploadIntent::Wait(_) => panic!("Expected Proceed after failure"),
        }
    }

    #[test]
    fn test_different_hashes_independent() {
        let dedup = UploadDeduplicator::new();

        match dedup.register("hash1") {
            UploadIntent::Proceed => {}
            UploadIntent::Wait(_) => panic!("Expected Proceed"),
        }

        match dedup.register("hash2") {
            UploadIntent::Proceed => {}
            UploadIntent::Wait(_) => panic!("Expected Proceed for different hash"),
        }

        assert_eq!(dedup.in_flight_count(), 2);
    }
}
