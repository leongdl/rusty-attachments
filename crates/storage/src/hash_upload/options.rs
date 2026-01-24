//! Configuration options for hash+upload pipeline.

/// Options for hash+upload pipeline.
#[derive(Debug, Clone)]
pub struct HashUploadOptions {
    /// Maximum memory for in-flight data (default: 1GB).
    pub max_memory_bytes: u64,
    /// Maximum concurrent operations (default: 10).
    pub max_concurrency: usize,
    /// Chunk size for large files (default: 256MB).
    pub chunk_size: u64,
    /// Whether to use hash cache.
    pub use_hash_cache: bool,
    /// Whether to use S3 check cache.
    pub use_s3_check_cache: bool,
    /// Force rehash even if cached.
    pub force_rehash: bool,
}

impl Default for HashUploadOptions {
    fn default() -> Self {
        Self {
            max_memory_bytes: 5 * 1024 * 1024 * 1024, // 5GB
            max_concurrency: 32, // Moderate concurrency - too high causes contention
            chunk_size: 256 * 1024 * 1024, // 256MB
            use_hash_cache: true,
            use_s3_check_cache: true,
            force_rehash: false,
        }
    }
}

impl HashUploadOptions {
    /// Create options with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum memory for in-flight data.
    ///
    /// # Arguments
    /// * `bytes` - Maximum bytes to allow in-flight
    pub fn with_max_memory(mut self, bytes: u64) -> Self {
        self.max_memory_bytes = bytes;
        self
    }

    /// Set maximum concurrent operations.
    ///
    /// # Arguments
    /// * `concurrency` - Maximum number of concurrent file operations
    pub fn with_max_concurrency(mut self, concurrency: usize) -> Self {
        self.max_concurrency = concurrency;
        self
    }

    /// Set chunk size for large files.
    ///
    /// # Arguments
    /// * `size` - Chunk size in bytes
    pub fn with_chunk_size(mut self, size: u64) -> Self {
        self.chunk_size = size;
        self
    }

    /// Enable or disable hash cache usage.
    ///
    /// # Arguments
    /// * `enabled` - Whether to use hash cache
    pub fn with_hash_cache(mut self, enabled: bool) -> Self {
        self.use_hash_cache = enabled;
        self
    }

    /// Enable or disable S3 check cache usage.
    ///
    /// # Arguments
    /// * `enabled` - Whether to use S3 check cache
    pub fn with_s3_check_cache(mut self, enabled: bool) -> Self {
        self.use_s3_check_cache = enabled;
        self
    }

    /// Force rehashing even if hash is cached.
    ///
    /// # Arguments
    /// * `force` - Whether to force rehash
    pub fn with_force_rehash(mut self, force: bool) -> Self {
        self.force_rehash = force;
        self
    }

    /// Calculate memory based on system resources.
    ///
    /// Uses heuristic: min(16GB, max(256MB, quarter_of_total, available - 1GB))
    ///
    /// # Returns
    /// Recommended max memory in bytes.
    pub fn auto_memory() -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                let mut total_kb: u64 = 0;
                let mut available_kb: u64 = 0;

                for line in meminfo.lines() {
                    if line.starts_with("MemTotal:") {
                        total_kb = parse_meminfo_value(line);
                    } else if line.starts_with("MemAvailable:") {
                        available_kb = parse_meminfo_value(line);
                    }
                }

                let quarter_total: u64 = total_kb * 1024 / 4;
                let available_minus_1gb: u64 =
                    available_kb.saturating_sub(1024 * 1024) * 1024;

                let min_bytes: u64 = 256 * 1024 * 1024; // 256MB
                let max_bytes: u64 = 16 * 1024 * 1024 * 1024; // 16GB

                return max_bytes.min(min_bytes.max(quarter_total).max(available_minus_1gb));
            }
        }

        // Default fallback
        1024 * 1024 * 1024 // 1GB
    }
}

/// Parse a value from /proc/meminfo line.
///
/// # Arguments
/// * `line` - Line from /proc/meminfo (e.g., "MemTotal:       16384000 kB")
///
/// # Returns
/// Value in KB, or 0 if parsing fails.
#[cfg(target_os = "linux")]
fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = HashUploadOptions::default();
        assert_eq!(opts.max_memory_bytes, 5 * 1024 * 1024 * 1024);
        assert_eq!(opts.max_concurrency, 32);
        assert_eq!(opts.chunk_size, 256 * 1024 * 1024);
        assert!(opts.use_hash_cache);
        assert!(opts.use_s3_check_cache);
        assert!(!opts.force_rehash);
    }

    #[test]
    fn test_builder_pattern() {
        let opts = HashUploadOptions::new()
            .with_max_memory(512 * 1024 * 1024)
            .with_max_concurrency(20)
            .with_chunk_size(128 * 1024 * 1024)
            .with_hash_cache(false)
            .with_s3_check_cache(false)
            .with_force_rehash(true);

        assert_eq!(opts.max_memory_bytes, 512 * 1024 * 1024);
        assert_eq!(opts.max_concurrency, 20);
        assert_eq!(opts.chunk_size, 128 * 1024 * 1024);
        assert!(!opts.use_hash_cache);
        assert!(!opts.use_s3_check_cache);
        assert!(opts.force_rehash);
    }

    #[test]
    fn test_auto_memory_returns_reasonable_value() {
        let mem: u64 = HashUploadOptions::auto_memory();
        // Should be at least 256MB
        assert!(mem >= 256 * 1024 * 1024);
        // Should be at most 16GB
        assert!(mem <= 16 * 1024 * 1024 * 1024);
    }
}
