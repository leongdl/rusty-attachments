//! Benchmark runner for hash+upload performance testing.
//!
//! This example provides tools for benchmarking the pipelined hash+upload
//! implementation against the sequential approach.
//!
//! # Usage
//!
//! ```bash
//! # Generate test data
//! cargo run --release --example bench_hash_upload -- generate --test-dir /tmp/bench
//!
//! # Run benchmark with S3 (requires credentials)
//! source creds.sh
//! cargo run --release --example bench_hash_upload -- run \
//!   --test-dir /tmp/bench \
//!   --bucket adeadlineja \
//!   --prefix rusty/bench
//!
//! # Run benchmark with transfer manager (automatic multipart)
//! cargo run --release --example bench_hash_upload -- run \
//!   --test-dir /tmp/bench \
//!   --bucket adeadlineja \
//!   --prefix rusty/bench \
//!   --transfer-manager
//!
//! # Run benchmark with local filesystem
//! cargo run --release --example bench_hash_upload -- run \
//!   --test-dir /tmp/bench \
//!   --local
//! ```

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use rusty_attachments_common::get_machine_id;
use rusty_attachments_model::{v2025_12, Manifest};
use rusty_attachments_storage::{
    hash_upload_abs_manifest, hash_upload_abs_manifest_staged, FileSystemDataCache, HashCache,
    HashUploadOptions, OwnedS3DataCache, SqliteHashCache, StorageSettings,
};
use rusty_attachments_storage_crt::{CrtStorageClient, TransferManagerClient};

// ============================================================================
// Test Data Generator
// ============================================================================

/// Generate test files with reproducible random content.
pub struct TestDataGenerator {
    root: PathBuf,
    rng: StdRng,
}

impl TestDataGenerator {
    /// Create a new test data generator.
    ///
    /// # Arguments
    /// * `root` - Root directory for generated files
    /// * `seed` - Random seed for reproducibility
    pub fn new(root: PathBuf, seed: u64) -> Self {
        Self {
            root,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Generate a file with random content.
    ///
    /// # Arguments
    /// * `name` - Relative file name
    /// * `size` - File size in bytes
    ///
    /// # Returns
    /// Absolute path to generated file.
    pub fn generate_file(&mut self, name: &str, size: u64) -> std::io::Result<PathBuf> {
        let path: PathBuf = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file: std::fs::File = std::fs::File::create(&path)?;
        let mut remaining: u64 = size;
        let mut buffer: Vec<u8> = vec![0u8; 64 * 1024]; // 64KB chunks

        while remaining > 0 {
            let chunk_size: usize = std::cmp::min(remaining, buffer.len() as u64) as usize;
            self.rng.fill(&mut buffer[..chunk_size]);
            file.write_all(&buffer[..chunk_size])?;
            remaining -= chunk_size as u64;
        }

        Ok(path)
    }

    /// Generate VFX job test dataset (~6GB, 270 files).
    ///
    /// # Returns
    /// List of generated file paths.
    pub fn generate_vfx_dataset(&mut self) -> std::io::Result<Vec<PathBuf>> {
        let mut files: Vec<PathBuf> = Vec::new();

        println!("Generating VFX dataset...");

        // Scene files (5 × 10-50 MB)
        println!("  Creating scene files...");
        for i in 0..5 {
            let size: u64 = self.rng.gen_range(10..50) * 1024 * 1024;
            files.push(self.generate_file(&format!("scenes/scene_{:02}.ma", i), size)?);
        }

        // Small textures (100 × 1-10 KB)
        println!("  Creating small textures...");
        for i in 0..100 {
            let size: u64 = self.rng.gen_range(1..10) * 1024;
            files.push(self.generate_file(
                &format!("textures/small/tex_{:03}.png", i),
                size,
            )?);
        }

        // Medium textures (50 × 100 KB - 5 MB)
        println!("  Creating medium textures...");
        for i in 0..50 {
            let size: u64 = self.rng.gen_range(100..5000) * 1024;
            files.push(self.generate_file(
                &format!("textures/medium/tex_{:03}.exr", i),
                size,
            )?);
        }

        // Large textures (20 × 10-100 MB)
        println!("  Creating large textures...");
        for i in 0..20 {
            let size: u64 = self.rng.gen_range(10..100) * 1024 * 1024;
            files.push(self.generate_file(
                &format!("textures/large/tex_{:03}.exr", i),
                size,
            )?);
        }

        // Geometry caches (10 × 50-200 MB)
        println!("  Creating geometry caches...");
        for i in 0..10 {
            let size: u64 = self.rng.gen_range(50..200) * 1024 * 1024;
            files.push(self.generate_file(&format!("geo/cache_{:02}.abc", i), size)?);
        }

        // Simulation caches (5 × 200 MB - 1 GB)
        println!("  Creating simulation caches...");
        for i in 0..5 {
            let size: u64 = self.rng.gen_range(200..1000) * 1024 * 1024;
            files.push(self.generate_file(&format!("sim/sim_{:02}.vdb", i), size)?);
        }

        // Render outputs (20 × 5-50 MB)
        println!("  Creating render outputs...");
        for i in 0..20 {
            let size: u64 = self.rng.gen_range(5..50) * 1024 * 1024;
            files.push(self.generate_file(&format!("renders/frame_{:04}.exr", i), size)?);
        }

        // Config files (50 × 1-100 KB)
        println!("  Creating config files...");
        for i in 0..50 {
            let size: u64 = self.rng.gen_range(1..100) * 1024;
            files.push(self.generate_file(&format!("config/config_{:02}.json", i), size)?);
        }

        Ok(files)
    }

    /// Create duplicate files (same content, different names).
    ///
    /// # Arguments
    /// * `source_files` - Files to duplicate
    /// * `count` - Number of duplicates to create
    ///
    /// # Returns
    /// List of duplicate file paths.
    pub fn create_duplicates(
        &mut self,
        source_files: &[PathBuf],
        count: usize,
    ) -> std::io::Result<Vec<PathBuf>> {
        let mut duplicates: Vec<PathBuf> = Vec::new();

        for i in 0..count {
            let source: &PathBuf = &source_files[i % source_files.len()];
            let dest: PathBuf = self.root.join(format!(
                "duplicates/dup_{:02}_{}",
                i,
                source.file_name().unwrap().to_string_lossy()
            ));

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(source, &dest)?;
            duplicates.push(dest);
        }

        Ok(duplicates)
    }

    /// Generate a small test dataset for quick testing.
    ///
    /// # Returns
    /// List of generated file paths.
    pub fn generate_small_dataset(&mut self) -> std::io::Result<Vec<PathBuf>> {
        let mut files: Vec<PathBuf> = Vec::new();

        println!("Generating small dataset...");

        // 10 small files (1-10 KB)
        for i in 0..10 {
            let size: u64 = self.rng.gen_range(1..10) * 1024;
            files.push(self.generate_file(&format!("small/file_{:02}.txt", i), size)?);
        }

        // 5 medium files (100 KB - 1 MB)
        for i in 0..5 {
            let size: u64 = self.rng.gen_range(100..1000) * 1024;
            files.push(self.generate_file(&format!("medium/file_{:02}.bin", i), size)?);
        }

        Ok(files)
    }
}

// ============================================================================
// Benchmark Metrics
// ============================================================================

/// Metrics collected during benchmark.
#[derive(Debug, Clone, Default)]
pub struct BenchmarkMetrics {
    /// Total wall-clock time.
    pub total_time: Duration,
    /// Time spent hashing (if measurable separately).
    pub hash_time: Option<Duration>,
    /// Time spent uploading (if measurable separately).
    pub upload_time: Option<Duration>,
    /// Peak memory usage in bytes.
    pub peak_memory_bytes: u64,
    /// Total bytes processed.
    pub total_bytes: u64,
    /// Files processed.
    pub files_processed: u64,
    /// Files skipped (cache hits).
    pub files_skipped: u64,
    /// Effective throughput (bytes/sec).
    pub throughput_bytes_per_sec: f64,
}

impl BenchmarkMetrics {
    /// Print benchmark results in a formatted table.
    ///
    /// # Arguments
    /// * `name` - Name of the benchmark
    pub fn print(&self, name: &str) {
        println!("\n=== {} ===", name);
        println!("Total time:      {:?}", self.total_time);
        if let Some(hash_time) = self.hash_time {
            println!("  Hash time:     {:?}", hash_time);
        }
        if let Some(upload_time) = self.upload_time {
            println!("  Upload time:   {:?}", upload_time);
        }
        println!(
            "Peak memory:     {} MB",
            self.peak_memory_bytes / (1024 * 1024)
        );
        println!(
            "Total bytes:     {} MB",
            self.total_bytes / (1024 * 1024)
        );
        println!("Files processed: {}", self.files_processed);
        println!("Files skipped:   {}", self.files_skipped);
        println!(
            "Throughput:      {:.2} MB/s",
            self.throughput_bytes_per_sec / (1024.0 * 1024.0)
        );
    }
}

/// Get current process memory usage (Linux only).
///
/// # Returns
/// RSS memory in bytes, or 0 if unavailable.
#[cfg(target_os = "linux")]
fn get_memory_usage() -> u64 {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return kb * 1024;
                    }
                }
            }
        }
    }
    0
}

#[cfg(not(target_os = "linux"))]
fn get_memory_usage() -> u64 {
    0
}

// ============================================================================
// Cache Management
// ============================================================================

/// Clear all caches for clean benchmark runs.
fn clear_caches() {
    let cache_dir: PathBuf = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rusty-attachments");

    let hash_cache_path: PathBuf = cache_dir.join("hash_cache.db");
    let s3_check_cache_path: PathBuf = cache_dir.join("s3_check_cache.db");

    if hash_cache_path.exists() {
        if let Err(e) = std::fs::remove_file(&hash_cache_path) {
            eprintln!("Warning: Failed to remove hash cache: {}", e);
        } else {
            println!("Cleared hash cache: {}", hash_cache_path.display());
        }
    }

    if s3_check_cache_path.exists() {
        if let Err(e) = std::fs::remove_file(&s3_check_cache_path) {
            eprintln!("Warning: Failed to remove S3 check cache: {}", e);
        } else {
            println!("Cleared S3 check cache: {}", s3_check_cache_path.display());
        }
    }
}

// ============================================================================
// Manifest Building
// ============================================================================

/// Build a manifest from test directory.
///
/// # Arguments
/// * `test_dir` - Directory containing test files
///
/// # Returns
/// Tuple of (manifest, total_bytes).
fn build_manifest_from_dir(test_dir: &PathBuf) -> (Manifest, u64) {
    let mut files: Vec<v2025_12::ManifestFilePath> = Vec::new();
    let mut dirs: Vec<v2025_12::ManifestDirectoryPath> = Vec::new();
    let mut total_size: u64 = 0;

    // Walk directory
    for entry in walkdir::WalkDir::new(test_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path: &std::path::Path = entry.path();
        let relative_path: String = path
            .strip_prefix(test_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        if relative_path.is_empty() {
            continue;
        }

        if entry.file_type().is_dir() {
            dirs.push(v2025_12::ManifestDirectoryPath {
                path: relative_path,
                deleted: false,
            });
        } else if entry.file_type().is_file() {
            let metadata: std::fs::Metadata = entry.metadata().unwrap();
            let size: u64 = metadata.len();
            let mtime: i64 = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_micros() as i64)
                .unwrap_or(0);

            files.push(v2025_12::ManifestFilePath::file(
                relative_path,
                "", // hash will be filled in
                size,
                mtime,
            ));
            total_size += size;
        }
    }

    let manifest = Manifest::V2025_12(v2025_12::AssetManifest::snapshot(dirs, files));
    (manifest, total_size)
}

// ============================================================================
// Benchmark Runners
// ============================================================================

/// Run benchmark with S3 backend.
///
/// # Arguments
/// * `test_dir` - Directory containing test files
/// * `bucket` - S3 bucket name
/// * `prefix` - S3 key prefix for data storage
/// * `options` - Hash upload options
/// * `use_staged` - Whether to use the 3-stage pipeline
/// * `use_transfer_manager` - Whether to use the transfer manager client
async fn run_s3_benchmark(test_dir: &PathBuf, bucket: &str, prefix: &str, options: HashUploadOptions, use_staged: bool, use_transfer_manager: bool) -> BenchmarkMetrics {
    println!("\nBuilding manifest from {}...", test_dir.display());
    let (manifest, total_bytes): (Manifest, u64) = build_manifest_from_dir(test_dir);

    let file_count: usize = match &manifest {
        Manifest::V2023_03_03(m) => m.paths.len(),
        Manifest::V2025_12(m) => m.files.len(),
    };
    println!("Found {} files ({:.2} MB)", file_count, total_bytes as f64 / 1_000_000.0);

    // Create hash cache
    let cache_dir: PathBuf = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rusty-attachments");
    std::fs::create_dir_all(&cache_dir).ok();

    let sqlite_backend = SqliteHashCache::open(&cache_dir.join("hash_cache.db"), get_machine_id())
        .expect("Failed to open hash cache");
    let hash_cache: HashCache = HashCache::with_default_ttl(sqlite_backend);

    // Run pipelined hash+upload
    let pipeline_type: &str = if use_staged { "3-stage" } else { "single-stage" };
    let client_type: &str = if use_transfer_manager { "transfer-manager" } else { "standard" };
    println!("\nRunning {} pipelined hash+upload (S3: s3://{}/{}, client: {})...", pipeline_type, bucket, prefix, client_type);
    println!("  Max memory: {} GB", options.max_memory_bytes / (1024 * 1024 * 1024));
    if use_staged {
        println!("  Read concurrency: 16");
        println!("  Hash concurrency: 16");
        println!("  Upload concurrency: 32");
    } else {
        println!("  Max concurrency: {}", options.max_concurrency);
    }
    let start_memory: u64 = get_memory_usage();
    let mut peak_memory: u64 = start_memory;
    let start: Instant = Instant::now();

    let settings = StorageSettings::default();

    let result = if use_transfer_manager {
        // Use transfer manager client
        let client: Arc<TransferManagerClient> = Arc::new(
            TransferManagerClient::new(settings)
                .await
                .expect("Failed to create transfer manager client"),
        );
        let data_cache: OwnedS3DataCache<TransferManagerClient> =
            OwnedS3DataCache::new(client, bucket, prefix);

        if use_staged {
            hash_upload_abs_manifest_staged(
                manifest,
                test_dir.to_str().unwrap(),
                Arc::new(data_cache),
                Some(Arc::new(hash_cache)),
                options,
            )
            .await
            .expect("Hash+upload failed")
        } else {
            hash_upload_abs_manifest(
                manifest,
                test_dir.to_str().unwrap(),
                &data_cache,
                Some(&hash_cache),
                options,
            )
            .await
            .expect("Hash+upload failed")
        }
    } else {
        // Use standard CRT client
        let client: Arc<CrtStorageClient> = Arc::new(
            CrtStorageClient::new(settings)
                .await
                .expect("Failed to create S3 client"),
        );
        let data_cache: OwnedS3DataCache<CrtStorageClient> =
            OwnedS3DataCache::new(client, bucket, prefix);

        if use_staged {
            hash_upload_abs_manifest_staged(
                manifest,
                test_dir.to_str().unwrap(),
                Arc::new(data_cache),
                Some(Arc::new(hash_cache)),
                options,
            )
            .await
            .expect("Hash+upload failed")
        } else {
            hash_upload_abs_manifest(
                manifest,
                test_dir.to_str().unwrap(),
                &data_cache,
                Some(&hash_cache),
                options,
            )
            .await
            .expect("Hash+upload failed")
        }
    };

    let total_time: Duration = start.elapsed();
    peak_memory = peak_memory.max(get_memory_usage());

    let progress = result.progress;
    println!("\nCompleted:");
    println!("  Hashed: {} files ({:.2} MB)", progress.hashed_files, progress.hashed_bytes as f64 / 1_000_000.0);
    println!("  Hash skipped: {} files", progress.hash_skipped_files);
    println!("  Uploaded: {} files ({:.2} MB)", progress.uploaded_files, progress.uploaded_bytes as f64 / 1_000_000.0);
    println!("  Upload skipped: {} files", progress.upload_skipped_files);

    BenchmarkMetrics {
        total_time,
        hash_time: None,
        upload_time: None,
        peak_memory_bytes: peak_memory.saturating_sub(start_memory),
        total_bytes,
        files_processed: progress.hashed_files + progress.hash_skipped_files,
        files_skipped: progress.upload_skipped_files,
        throughput_bytes_per_sec: total_bytes as f64 / total_time.as_secs_f64(),
    }
}

/// Run benchmark with local filesystem backend.
///
/// # Arguments
/// * `test_dir` - Directory containing test files
/// * `options` - Hash upload options
/// * `use_staged` - Whether to use the 3-stage pipeline
async fn run_local_benchmark(test_dir: &PathBuf, options: HashUploadOptions, use_staged: bool) -> BenchmarkMetrics {
    println!("\nBuilding manifest from {}...", test_dir.display());
    let (manifest, total_bytes): (Manifest, u64) = build_manifest_from_dir(test_dir);

    let file_count: usize = match &manifest {
        Manifest::V2023_03_03(m) => m.paths.len(),
        Manifest::V2025_12(m) => m.files.len(),
    };
    println!("Found {} files ({:.2} MB)", file_count, total_bytes as f64 / 1_000_000.0);

    // Create local data cache
    let cache_dir: PathBuf = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rusty-attachments");
    std::fs::create_dir_all(&cache_dir).ok();

    let data_cache_dir: PathBuf = cache_dir.join("data_cache");
    std::fs::create_dir_all(&data_cache_dir).ok();

    let data_cache: FileSystemDataCache = FileSystemDataCache::new(data_cache_dir)
        .expect("Failed to create filesystem data cache");

    // Create hash cache
    let sqlite_backend = SqliteHashCache::open(&cache_dir.join("hash_cache.db"), get_machine_id())
        .expect("Failed to open hash cache");
    let hash_cache: HashCache = HashCache::with_default_ttl(sqlite_backend);

    // Run pipelined hash+upload
    let pipeline_type: &str = if use_staged { "3-stage" } else { "single-stage" };
    println!("\nRunning {} pipelined hash+upload (local)...", pipeline_type);
    println!("  Max memory: {} GB", options.max_memory_bytes / (1024 * 1024 * 1024));
    if use_staged {
        println!("  Read concurrency: 16");
        println!("  Hash concurrency: 16");
        println!("  Upload concurrency: 32");
    } else {
        println!("  Max concurrency: {}", options.max_concurrency);
    }
    let start_memory: u64 = get_memory_usage();
    let mut peak_memory: u64 = start_memory;
    let start: Instant = Instant::now();

    let result = if use_staged {
        hash_upload_abs_manifest_staged(
            manifest,
            test_dir.to_str().unwrap(),
            Arc::new(data_cache),
            Some(Arc::new(hash_cache)),
            options,
        )
        .await
        .expect("Hash+upload failed")
    } else {
        hash_upload_abs_manifest(
            manifest,
            test_dir.to_str().unwrap(),
            &data_cache,
            Some(&hash_cache),
            options,
        )
        .await
        .expect("Hash+upload failed")
    };

    let total_time: Duration = start.elapsed();
    peak_memory = peak_memory.max(get_memory_usage());

    let progress = result.progress;
    println!("\nCompleted:");
    println!("  Hashed: {} files ({:.2} MB)", progress.hashed_files, progress.hashed_bytes as f64 / 1_000_000.0);
    println!("  Hash skipped: {} files", progress.hash_skipped_files);
    println!("  Uploaded: {} files ({:.2} MB)", progress.uploaded_files, progress.uploaded_bytes as f64 / 1_000_000.0);
    println!("  Upload skipped: {} files", progress.upload_skipped_files);

    BenchmarkMetrics {
        total_time,
        hash_time: None,
        upload_time: None,
        peak_memory_bytes: peak_memory.saturating_sub(start_memory),
        total_bytes,
        files_processed: progress.hashed_files + progress.hash_skipped_files,
        files_skipped: progress.upload_skipped_files,
        throughput_bytes_per_sec: total_bytes as f64 / total_time.as_secs_f64(),
    }
}

// ============================================================================
// CLI
// ============================================================================

fn print_usage() {
    println!("Hash+Upload Benchmark Tool");
    println!();
    println!("USAGE:");
    println!("  bench_hash_upload <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("  generate    Generate test data");
    println!("  run         Run benchmark (requires S3 credentials)");
    println!("  help        Show this help message");
    println!();
    println!("GENERATE OPTIONS:");
    println!("  --test-dir <PATH>    Directory for test data (default: /tmp/hash_upload_bench)");
    println!("  --seed <NUMBER>      Random seed (default: 42)");
    println!("  --scenario <NAME>    Test scenario: vfx, small (default: small)");
    println!();
    println!("RUN OPTIONS:");
    println!("  --test-dir <PATH>    Directory with test data");
    println!("  --bucket <NAME>      S3 bucket name");
    println!("  --prefix <PATH>      S3 key prefix");
    println!("  --iterations <N>     Number of iterations (default: 1)");
    println!("  --max-memory-gb <N>  Maximum memory in GB (default: 5.0)");
    println!("  --max-concurrency <N> Max concurrent operations (default: 32, ignored with --staged)");
    println!("  --staged             Use 3-stage pipeline (read/hash/upload)");
    println!("  --transfer-manager   Use AWS S3 Transfer Manager (automatic multipart)");
    println!("  --no-clear-cache     Don't clear caches between iterations");
    println!("  --local              Use local filesystem instead of S3");
    println!();
    println!("EXAMPLES:");
    println!("  # Generate small test data");
    println!("  cargo run --release --example bench_hash_upload -- generate --test-dir /tmp/bench");
    println!();
    println!("  # Generate VFX dataset (~6GB)");
    println!("  cargo run --release --example bench_hash_upload -- generate --test-dir /tmp/bench --scenario vfx");
    println!();
    println!("  # Run with 3-stage pipeline");
    println!("  cargo run --release --example bench_hash_upload -- run --test-dir /tmp/bench --bucket mybucket --prefix test --staged");
    println!();
    println!("  # Run with transfer manager (automatic multipart uploads)");
    println!("  cargo run --release --example bench_hash_upload -- run --test-dir /tmp/bench --bucket mybucket --prefix test --transfer-manager");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "generate" => {
            let mut test_dir: PathBuf = PathBuf::from("/tmp/hash_upload_bench");
            let mut seed: u64 = 42;
            let mut scenario: String = "small".to_string();

            let mut i: usize = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--test-dir" => {
                        i += 1;
                        if i < args.len() {
                            test_dir = PathBuf::from(&args[i]);
                        }
                    }
                    "--seed" => {
                        i += 1;
                        if i < args.len() {
                            seed = args[i].parse().unwrap_or(42);
                        }
                    }
                    "--scenario" => {
                        i += 1;
                        if i < args.len() {
                            scenario = args[i].clone();
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            println!("Generating test data...");
            println!("  Directory: {}", test_dir.display());
            println!("  Seed: {}", seed);
            println!("  Scenario: {}", scenario);

            // Create directory
            if let Err(e) = std::fs::create_dir_all(&test_dir) {
                eprintln!("Failed to create directory: {}", e);
                return;
            }

            let mut generator = TestDataGenerator::new(test_dir, seed);

            let files: Vec<PathBuf> = match scenario.as_str() {
                "vfx" => generator.generate_vfx_dataset(),
                "small" => generator.generate_small_dataset(),
                _ => {
                    eprintln!("Unknown scenario: {}", scenario);
                    return;
                }
            }
            .unwrap_or_else(|e| {
                eprintln!("Failed to generate files: {}", e);
                Vec::new()
            });

            let total_size: u64 = files
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .sum();

            println!();
            println!("Generated {} files ({:.2} MB)", files.len(), total_size as f64 / 1_000_000.0);
        }
        "run" => {
            let mut test_dir: PathBuf = PathBuf::from("/tmp/hash_upload_bench");
            let mut bucket: Option<String> = None;
            let mut prefix: String = "bench".to_string();
            let mut iterations: u32 = 1;
            let mut clear_cache: bool = true;
            let mut use_s3: bool = true;
            let mut max_memory_gb: f64 = 5.0;
            let mut max_concurrency: usize = 32;
            let mut use_staged: bool = false;
            let mut use_transfer_manager: bool = false;

            let mut i: usize = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--test-dir" => {
                        i += 1;
                        if i < args.len() {
                            test_dir = PathBuf::from(&args[i]);
                        }
                    }
                    "--bucket" => {
                        i += 1;
                        if i < args.len() {
                            bucket = Some(args[i].clone());
                        }
                    }
                    "--prefix" => {
                        i += 1;
                        if i < args.len() {
                            prefix = args[i].clone();
                        }
                    }
                    "--iterations" => {
                        i += 1;
                        if i < args.len() {
                            iterations = args[i].parse().unwrap_or(1);
                        }
                    }
                    "--max-memory-gb" => {
                        i += 1;
                        if i < args.len() {
                            max_memory_gb = args[i].parse().unwrap_or(2.0);
                        }
                    }
                    "--max-concurrency" => {
                        i += 1;
                        if i < args.len() {
                            max_concurrency = args[i].parse().unwrap_or(64);
                        }
                    }
                    "--no-clear-cache" => {
                        clear_cache = false;
                    }
                    "--local" => {
                        use_s3 = false;
                    }
                    "--staged" => {
                        use_staged = true;
                    }
                    "--transfer-manager" => {
                        use_transfer_manager = true;
                    }
                    _ => {}
                }
                i += 1;
            }

            // Validate test directory
            if !test_dir.exists() {
                eprintln!("Error: Test directory does not exist: {}", test_dir.display());
                eprintln!("Generate test data first with:");
                eprintln!("  cargo run --release --example bench_hash_upload -- generate --test-dir {}", test_dir.display());
                return;
            }

            println!("Hash+Upload Benchmark");
            println!("=====================");
            println!("Test dir:       {}", test_dir.display());
            if use_s3 {
                println!("Bucket:         {}", bucket.as_deref().unwrap_or("(not set)"));
                println!("Prefix:         {}", prefix);
                println!("S3 Client:      {}", if use_transfer_manager { "transfer-manager (multipart)" } else { "standard" });
            } else {
                println!("Mode:           Local filesystem");
            }
            println!("Iterations:     {}", iterations);
            println!("Clear cache:    {}", clear_cache);
            println!("Max memory:     {:.1} GB", max_memory_gb);
            println!("Pipeline:       {}", if use_staged { "3-stage (read/hash/upload)" } else { "single-stage" });
            if !use_staged {
                println!("Max concurrency: {}", max_concurrency);
            }

            // Build options
            let max_memory_bytes: u64 = (max_memory_gb * 1024.0 * 1024.0 * 1024.0) as u64;
            let options = HashUploadOptions::default()
                .with_max_memory(max_memory_bytes)
                .with_max_concurrency(max_concurrency);

            // Run benchmark
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async {
                for iter in 1..=iterations {
                    println!("\n============================================================");
                    println!("Iteration {}/{}", iter, iterations);
                    println!("============================================================");

                    if clear_cache {
                        clear_caches();
                    }

                    let metrics: BenchmarkMetrics = if use_s3 {
                        let bucket_name: &str = bucket.as_deref().expect("--bucket is required for S3 mode");
                        run_s3_benchmark(&test_dir, bucket_name, &prefix, options.clone(), use_staged, use_transfer_manager).await
                    } else {
                        run_local_benchmark(&test_dir, options.clone(), use_staged).await
                    };

                    metrics.print(&format!("Iteration {}", iter));
                }
            });
        }
        "help" | "--help" | "-h" => {
            print_usage();
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
        }
    }
}
