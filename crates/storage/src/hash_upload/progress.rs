//! Progress tracking for hash+upload operations.
//!
//! Provides thread-safe progress tracking for concurrent hash and upload operations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Progress snapshot for hash+upload operations.
#[derive(Debug, Clone)]
pub struct HashUploadProgress {
    // Totals
    /// Total files to process.
    pub total_files: u64,
    /// Total bytes to process.
    pub total_bytes: u64,

    // Hashing progress
    /// Files hashed (computed, not from cache).
    pub hashed_files: u64,
    /// Bytes hashed (computed, not from cache).
    pub hashed_bytes: u64,
    /// Files with hash from cache.
    pub hash_skipped_files: u64,
    /// Bytes with hash from cache.
    pub hash_skipped_bytes: u64,

    // Upload progress
    /// Files uploaded (transferred, not skipped).
    pub uploaded_files: u64,
    /// Bytes uploaded (transferred, not skipped).
    pub uploaded_bytes: u64,
    /// Files skipped (already existed).
    pub upload_skipped_files: u64,
    /// Bytes skipped (already existed).
    pub upload_skipped_bytes: u64,

    // Timing
    /// Elapsed time in seconds.
    pub elapsed_secs: f64,
    /// Transfer rate in bytes per second.
    pub transfer_rate_bytes_per_sec: f64,

    // Overall
    /// Progress percentage (0-100).
    pub progress_percent: f64,
    /// Human-readable progress message.
    pub message: String,
}

/// Thread-safe progress tracker.
pub struct ProgressTracker {
    start_time: Instant,
    total_files: u64,
    total_bytes: u64,

    hashed_files: AtomicU64,
    hashed_bytes: AtomicU64,
    hash_skipped_files: AtomicU64,
    hash_skipped_bytes: AtomicU64,

    uploaded_files: AtomicU64,
    uploaded_bytes: AtomicU64,
    upload_skipped_files: AtomicU64,
    upload_skipped_bytes: AtomicU64,
}

impl ProgressTracker {
    /// Create a new progress tracker.
    ///
    /// # Arguments
    /// * `total_files` - Total number of files to process
    /// * `total_bytes` - Total bytes to process
    pub fn new(total_files: u64, total_bytes: u64) -> Self {
        Self {
            start_time: Instant::now(),
            total_files,
            total_bytes,
            hashed_files: AtomicU64::new(0),
            hashed_bytes: AtomicU64::new(0),
            hash_skipped_files: AtomicU64::new(0),
            hash_skipped_bytes: AtomicU64::new(0),
            uploaded_files: AtomicU64::new(0),
            uploaded_bytes: AtomicU64::new(0),
            upload_skipped_files: AtomicU64::new(0),
            upload_skipped_bytes: AtomicU64::new(0),
        }
    }

    /// Record hash completion for a file.
    ///
    /// # Arguments
    /// * `bytes` - Size of the file
    /// * `from_cache` - Whether hash was from cache (skipped computation)
    pub fn record_hash_complete(&self, bytes: u64, from_cache: bool) {
        if from_cache {
            self.hash_skipped_files.fetch_add(1, Ordering::Relaxed);
            self.hash_skipped_bytes.fetch_add(bytes, Ordering::Relaxed);
        } else {
            self.hashed_files.fetch_add(1, Ordering::Relaxed);
            self.hashed_bytes.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    /// Record upload completion for a file.
    ///
    /// # Arguments
    /// * `bytes` - Size of the file
    /// * `skipped` - Whether upload was skipped (already existed)
    pub fn record_upload_complete(&self, bytes: u64, skipped: bool) {
        if skipped {
            self.upload_skipped_files.fetch_add(1, Ordering::Relaxed);
            self.upload_skipped_bytes.fetch_add(bytes, Ordering::Relaxed);
        } else {
            self.uploaded_files.fetch_add(1, Ordering::Relaxed);
            self.uploaded_bytes.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    /// Get a consistent snapshot of current progress.
    ///
    /// # Returns
    /// Current progress state.
    pub fn snapshot(&self) -> HashUploadProgress {
        let elapsed: f64 = self.start_time.elapsed().as_secs_f64();

        let hashed_files: u64 = self.hashed_files.load(Ordering::Relaxed);
        let hashed_bytes: u64 = self.hashed_bytes.load(Ordering::Relaxed);
        let hash_skipped_files: u64 = self.hash_skipped_files.load(Ordering::Relaxed);
        let hash_skipped_bytes: u64 = self.hash_skipped_bytes.load(Ordering::Relaxed);

        let uploaded_files: u64 = self.uploaded_files.load(Ordering::Relaxed);
        let uploaded_bytes: u64 = self.uploaded_bytes.load(Ordering::Relaxed);
        let upload_skipped_files: u64 = self.upload_skipped_files.load(Ordering::Relaxed);
        let upload_skipped_bytes: u64 = self.upload_skipped_bytes.load(Ordering::Relaxed);

        let total_processed_bytes: u64 = uploaded_bytes + upload_skipped_bytes;

        let progress_percent: f64 = if self.total_bytes > 0 {
            (total_processed_bytes as f64 / self.total_bytes as f64) * 100.0
        } else {
            100.0
        };

        let transfer_rate: f64 = if elapsed > 0.0 {
            uploaded_bytes as f64 / elapsed
        } else {
            0.0
        };

        let total_hashed_bytes: u64 = hashed_bytes + hash_skipped_bytes;
        let message: String = format!(
            "Hashed {:.1} MB, Uploaded {:.1} MB / {:.1} MB ({:.1}%)",
            total_hashed_bytes as f64 / 1_000_000.0,
            total_processed_bytes as f64 / 1_000_000.0,
            self.total_bytes as f64 / 1_000_000.0,
            progress_percent
        );

        HashUploadProgress {
            total_files: self.total_files,
            total_bytes: self.total_bytes,
            hashed_files,
            hashed_bytes,
            hash_skipped_files,
            hash_skipped_bytes,
            uploaded_files,
            uploaded_bytes,
            upload_skipped_files,
            upload_skipped_bytes,
            elapsed_secs: elapsed,
            transfer_rate_bytes_per_sec: transfer_rate,
            progress_percent,
            message,
        }
    }

    /// Get total files processed (hashed + hash_skipped).
    pub fn files_hashed(&self) -> u64 {
        self.hashed_files.load(Ordering::Relaxed)
            + self.hash_skipped_files.load(Ordering::Relaxed)
    }

    /// Get total files uploaded (uploaded + upload_skipped).
    pub fn files_uploaded(&self) -> u64 {
        self.uploaded_files.load(Ordering::Relaxed)
            + self.upload_skipped_files.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tracker() {
        let tracker = ProgressTracker::new(100, 1000);
        let progress = tracker.snapshot();

        assert_eq!(progress.total_files, 100);
        assert_eq!(progress.total_bytes, 1000);
        assert_eq!(progress.hashed_files, 0);
        assert_eq!(progress.uploaded_files, 0);
    }

    #[test]
    fn test_record_hash_computed() {
        let tracker = ProgressTracker::new(10, 1000);

        tracker.record_hash_complete(100, false);

        let progress = tracker.snapshot();
        assert_eq!(progress.hashed_files, 1);
        assert_eq!(progress.hashed_bytes, 100);
        assert_eq!(progress.hash_skipped_files, 0);
    }

    #[test]
    fn test_record_hash_cached() {
        let tracker = ProgressTracker::new(10, 1000);

        tracker.record_hash_complete(100, true);

        let progress = tracker.snapshot();
        assert_eq!(progress.hashed_files, 0);
        assert_eq!(progress.hash_skipped_files, 1);
        assert_eq!(progress.hash_skipped_bytes, 100);
    }

    #[test]
    fn test_record_upload_transferred() {
        let tracker = ProgressTracker::new(10, 1000);

        tracker.record_upload_complete(100, false);

        let progress = tracker.snapshot();
        assert_eq!(progress.uploaded_files, 1);
        assert_eq!(progress.uploaded_bytes, 100);
        assert_eq!(progress.upload_skipped_files, 0);
    }

    #[test]
    fn test_record_upload_skipped() {
        let tracker = ProgressTracker::new(10, 1000);

        tracker.record_upload_complete(100, true);

        let progress = tracker.snapshot();
        assert_eq!(progress.uploaded_files, 0);
        assert_eq!(progress.upload_skipped_files, 1);
        assert_eq!(progress.upload_skipped_bytes, 100);
    }

    #[test]
    fn test_progress_percent() {
        let tracker = ProgressTracker::new(10, 1000);

        tracker.record_upload_complete(500, false);

        let progress = tracker.snapshot();
        assert!((progress.progress_percent - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_files_hashed_total() {
        let tracker = ProgressTracker::new(10, 1000);

        tracker.record_hash_complete(100, false);
        tracker.record_hash_complete(100, true);
        tracker.record_hash_complete(100, false);

        assert_eq!(tracker.files_hashed(), 3);
    }
}
