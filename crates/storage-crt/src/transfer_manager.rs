//! AWS S3 Transfer Manager client implementation.
//!
//! Uses the high-performance transfer manager for automatic multipart uploads
//! and parallel byte-range downloads.

use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::Path;

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3_transfer_manager::Client as TransferManager;
use aws_sdk_s3_transfer_manager::io::InputStream;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use rusty_attachments_storage::{
    ObjectInfo, ObjectMetadata, ProgressCallback, StorageClient, StorageError, StorageSettings,
};

/// StorageClient implementation using AWS S3 Transfer Manager.
///
/// This client provides high-performance S3 operations with automatic
/// multipart uploads and parallel byte-range downloads.
pub struct TransferManagerClient {
    /// The underlying S3 client for simple operations.
    s3_client: S3Client,
    /// The transfer manager for high-performance uploads/downloads.
    transfer_manager: TransferManager,
    /// Expected bucket owner for security validation.
    expected_bucket_owner: Option<String>,
}

impl TransferManagerClient {
    /// Create a new transfer manager client with default credential chain.
    ///
    /// # Arguments
    /// * `settings` - Storage settings including region and optional credentials
    ///
    /// # Returns
    /// A new transfer manager client.
    pub async fn new(settings: StorageSettings) -> Result<Self, StorageError> {
        let config_loader = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new(settings.region.clone()));

        let config_loader = if let Some(ref creds) = settings.credentials {
            let credentials = Credentials::new(
                &creds.access_key_id,
                &creds.secret_access_key,
                creds.session_token.clone(),
                None,
                "rusty-attachments",
            );
            config_loader.credentials_provider(credentials)
        } else {
            config_loader
        };

        let sdk_config = config_loader.load().await;
        let s3_client = S3Client::new(&sdk_config);

        // Create transfer manager from environment config
        let tm_config = aws_sdk_s3_transfer_manager::from_env().load().await;
        let transfer_manager = TransferManager::new(tm_config);

        Ok(Self {
            s3_client,
            transfer_manager,
            expected_bucket_owner: settings.expected_bucket_owner,
        })
    }
}

#[async_trait]
impl StorageClient for TransferManagerClient {
    fn expected_bucket_owner(&self) -> Option<&str> {
        self.expected_bucket_owner.as_deref()
    }

    async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<u64>, StorageError> {
        let mut request = self.s3_client.head_object().bucket(bucket).key(key);

        if let Some(ref owner) = self.expected_bucket_owner {
            request = request.expected_bucket_owner(owner);
        }

        match request.send().await {
            Ok(output) => Ok(output.content_length().map(|l| l as u64)),
            Err(err) => {
                let service_err = err.into_service_error();
                if service_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(StorageError::NetworkError {
                        message: service_err.to_string(),
                        retryable: false,
                    })
                }
            }
        }
    }

    async fn head_object_with_metadata(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<ObjectMetadata>, StorageError> {
        let mut request = self.s3_client.head_object().bucket(bucket).key(key);

        if let Some(ref owner) = self.expected_bucket_owner {
            request = request.expected_bucket_owner(owner);
        }

        match request.send().await {
            Ok(output) => {
                let user_metadata: HashMap<String, String> = output
                    .metadata()
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();

                let last_modified: Option<i64> = output
                    .last_modified()
                    .and_then(|dt| dt.to_millis().ok())
                    .map(|ms| ms / 1000);

                Ok(Some(ObjectMetadata {
                    size: output.content_length().map(|l| l as u64).unwrap_or(0),
                    last_modified,
                    content_type: output.content_type().map(|s| s.to_string()),
                    etag: output.e_tag().map(|s| s.to_string()),
                    user_metadata,
                }))
            }
            Err(err) => {
                let service_err = err.into_service_error();
                if service_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(StorageError::NetworkError {
                        message: service_err.to_string(),
                        retryable: false,
                    })
                }
            }
        }
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: &[u8],
        content_type: Option<&str>,
        _metadata: Option<&HashMap<String, String>>,
    ) -> Result<(), StorageError> {
        // Use transfer manager for uploads - it handles multipart automatically
        let stream = InputStream::from(bytes::Bytes::copy_from_slice(data));

        let mut upload = self
            .transfer_manager
            .upload()
            .bucket(bucket)
            .key(key)
            .body(stream);

        if let Some(ct) = content_type {
            upload = upload.content_type(ct);
        }

        // Note: Transfer manager doesn't support metadata directly in upload builder
        // For metadata support, we'd need to use the underlying S3 client

        let handle = upload
            .initiate()
            .map_err(|e| StorageError::NetworkError {
                message: format!("Failed to initiate upload: {}", e),
                retryable: true,
            })?;

        handle
            .join()
            .await
            .map_err(|e| StorageError::NetworkError {
                message: format!("Upload failed: {}", e),
                retryable: true,
            })?;

        Ok(())
    }

    async fn put_object_from_file(
        &self,
        bucket: &str,
        key: &str,
        file_path: &str,
        content_type: Option<&str>,
        _metadata: Option<&HashMap<String, String>>,
        _progress: Option<&dyn ProgressCallback>,
    ) -> Result<(), StorageError> {
        // Use transfer manager for file uploads - automatic multipart
        let stream = InputStream::from_path(file_path)
            .map_err(|e| StorageError::IoError {
                path: file_path.to_string(),
                message: e.to_string(),
            })?;

        let mut upload = self
            .transfer_manager
            .upload()
            .bucket(bucket)
            .key(key)
            .body(stream);

        if let Some(ct) = content_type {
            upload = upload.content_type(ct);
        }

        let handle = upload
            .initiate()
            .map_err(|e| StorageError::NetworkError {
                message: format!("Failed to initiate upload: {}", e),
                retryable: true,
            })?;

        handle
            .join()
            .await
            .map_err(|e| StorageError::NetworkError {
                message: format!("Upload failed: {}", e),
                retryable: true,
            })?;

        Ok(())
    }

    async fn put_object_from_file_range(
        &self,
        bucket: &str,
        key: &str,
        file_path: &str,
        offset: u64,
        length: u64,
        _progress: Option<&dyn ProgressCallback>,
    ) -> Result<(), StorageError> {
        // For range uploads, read the range and use transfer manager
        let mut file = File::open(file_path)
            .await
            .map_err(|e| StorageError::IoError {
                path: file_path.to_string(),
                message: e.to_string(),
            })?;

        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::IoError {
                path: file_path.to_string(),
                message: e.to_string(),
            })?;

        let mut buffer: Vec<u8> = vec![0u8; length as usize];
        file.read_exact(&mut buffer)
            .await
            .map_err(|e| StorageError::IoError {
                path: file_path.to_string(),
                message: e.to_string(),
            })?;

        let stream = InputStream::from(bytes::Bytes::from(buffer));

        let handle = self
            .transfer_manager
            .upload()
            .bucket(bucket)
            .key(key)
            .body(stream)
            .initiate()
            .map_err(|e| StorageError::NetworkError {
                message: format!("Failed to initiate upload: {}", e),
                retryable: true,
            })?;

        handle
            .join()
            .await
            .map_err(|e| StorageError::NetworkError {
                message: format!("Upload failed: {}", e),
                retryable: true,
            })?;

        Ok(())
    }

    async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, StorageError> {
        // Use transfer manager for downloads
        let mut handle = self
            .transfer_manager
            .download()
            .bucket(bucket)
            .key(key)
            .initiate()
            .map_err(|e| StorageError::NetworkError {
                message: format!("Failed to initiate download: {}", e),
                retryable: true,
            })?;

        let mut data: Vec<u8> = Vec::new();
        while let Some(chunk_result) = handle.body_mut().next().await {
            let chunk = chunk_result
                .map_err(|e| StorageError::NetworkError {
                    message: format!("Download chunk failed: {}", e),
                    retryable: true,
                })?;
            data.extend_from_slice(&chunk.data.into_bytes());
        }

        Ok(data)
    }

    async fn get_object_to_file(
        &self,
        bucket: &str,
        key: &str,
        file_path: &str,
        _progress: Option<&dyn ProgressCallback>,
    ) -> Result<(), StorageError> {
        // Create parent directories if needed
        if let Some(parent) = Path::new(file_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::IoError {
                    path: parent.display().to_string(),
                    message: e.to_string(),
                })?;
        }

        let mut file = File::create(file_path)
            .await
            .map_err(|e| StorageError::IoError {
                path: file_path.to_string(),
                message: e.to_string(),
            })?;

        let mut handle = self
            .transfer_manager
            .download()
            .bucket(bucket)
            .key(key)
            .initiate()
            .map_err(|e| StorageError::NetworkError {
                message: format!("Failed to initiate download: {}", e),
                retryable: true,
            })?;

        while let Some(chunk_result) = handle.body_mut().next().await {
            let chunk = chunk_result
                .map_err(|e| StorageError::NetworkError {
                    message: format!("Download chunk failed: {}", e),
                    retryable: true,
                })?;
            file.write_all(&chunk.data.into_bytes())
                .await
                .map_err(|e| StorageError::IoError {
                    path: file_path.to_string(),
                    message: e.to_string(),
                })?;
        }

        file.flush().await.map_err(|e| StorageError::IoError {
            path: file_path.to_string(),
            message: e.to_string(),
        })?;

        Ok(())
    }

    async fn get_object_to_file_offset(
        &self,
        bucket: &str,
        key: &str,
        file_path: &str,
        offset: u64,
        _progress: Option<&dyn ProgressCallback>,
    ) -> Result<(), StorageError> {
        // Create parent directories if needed
        if let Some(parent) = Path::new(file_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::IoError {
                    path: parent.display().to_string(),
                    message: e.to_string(),
                })?;
        }

        // Open file for writing at offset
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(file_path)
            .await
            .map_err(|e| StorageError::IoError {
                path: file_path.to_string(),
                message: e.to_string(),
            })?;

        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::IoError {
                path: file_path.to_string(),
                message: e.to_string(),
            })?;

        let mut handle = self
            .transfer_manager
            .download()
            .bucket(bucket)
            .key(key)
            .initiate()
            .map_err(|e| StorageError::NetworkError {
                message: format!("Failed to initiate download: {}", e),
                retryable: true,
            })?;

        while let Some(chunk_result) = handle.body_mut().next().await {
            let chunk = chunk_result
                .map_err(|e| StorageError::NetworkError {
                    message: format!("Download chunk failed: {}", e),
                    retryable: true,
                })?;
            file.write_all(&chunk.data.into_bytes())
                .await
                .map_err(|e| StorageError::IoError {
                    path: file_path.to_string(),
                    message: e.to_string(),
                })?;
        }

        file.flush().await.map_err(|e| StorageError::IoError {
            path: file_path.to_string(),
            message: e.to_string(),
        })?;

        Ok(())
    }

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let mut objects: Vec<ObjectInfo> = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .s3_client
                .list_objects_v2()
                .bucket(bucket)
                .prefix(prefix);

            if let Some(ref owner) = self.expected_bucket_owner {
                request = request.expected_bucket_owner(owner);
            }

            if let Some(ref token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|err| StorageError::NetworkError {
                    message: err.to_string(),
                    retryable: true,
                })?;

            if let Some(ref contents) = response.contents {
                for obj in contents {
                    let last_modified: Option<i64> = obj
                        .last_modified()
                        .and_then(|dt| dt.to_millis().ok())
                        .map(|ms| ms / 1000);

                    objects.push(ObjectInfo {
                        key: obj.key().unwrap_or_default().to_string(),
                        size: obj.size().map(|s| s as u64).unwrap_or(0),
                        last_modified,
                        etag: obj.e_tag().map(|s| s.to_string()),
                    });
                }
            }

            if response.is_truncated() == Some(true) {
                continuation_token = response.next_continuation_token.clone();
            } else {
                break;
            }
        }

        Ok(objects)
    }
}
