//! Storage profile data structures.

use std::collections::HashMap;
use std::path::Path;

/// Classification of a file system location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemLocationType {
    /// Files are local to the submitter and must be uploaded.
    Local,
    /// Files are on shared storage accessible to workers (skip upload).
    Shared,
}

/// A named file system location with a root path and type.
#[derive(Debug, Clone)]
pub struct FileSystemLocation {
    /// Human-readable name for this location.
    pub name: String,
    /// Root path of this location.
    pub path: String,
    /// Whether this location is local or shared.
    pub location_type: FileSystemLocationType,
}

/// A storage profile containing multiple file system locations.
#[derive(Debug, Clone, Default)]
pub struct StorageProfile {
    /// List of file system locations in this profile.
    pub file_system_locations: Vec<FileSystemLocation>,
}

impl StorageProfile {
    /// Create a new empty storage profile.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a storage profile with the given locations.
    ///
    /// # Arguments
    /// * `locations` - File system locations to include
    pub fn with_locations(locations: Vec<FileSystemLocation>) -> Self {
        Self {
            file_system_locations: locations,
        }
    }

    /// Get all LOCAL type locations as a map of path -> name.
    pub fn local_locations(&self) -> HashMap<&str, &str> {
        self.file_system_locations
            .iter()
            .filter(|loc| loc.location_type == FileSystemLocationType::Local)
            .map(|loc| (loc.path.as_str(), loc.name.as_str()))
            .collect()
    }

    /// Get all SHARED type locations as a map of path -> name.
    pub fn shared_locations(&self) -> HashMap<&str, &str> {
        self.file_system_locations
            .iter()
            .filter(|loc| loc.location_type == FileSystemLocationType::Shared)
            .map(|loc| (loc.path.as_str(), loc.name.as_str()))
            .collect()
    }

    /// Check if a path is under any SHARED location.
    ///
    /// # Arguments
    /// * `path` - Path to check
    ///
    /// # Returns
    /// `true` if path is under a shared location.
    pub fn is_shared(&self, path: &Path) -> bool {
        self.file_system_locations
            .iter()
            .filter(|loc| loc.location_type == FileSystemLocationType::Shared)
            .any(|loc| path.starts_with(&loc.path))
    }

    /// Find the most specific LOCAL location containing a path.
    ///
    /// # Arguments
    /// * `path` - Path to find location for
    ///
    /// # Returns
    /// `(location_path, location_name)` or `None` if not under any LOCAL location.
    pub fn find_local_location(&self, path: &Path) -> Option<(&str, &str)> {
        self.file_system_locations
            .iter()
            .filter(|loc| loc.location_type == FileSystemLocationType::Local)
            .filter(|loc| path.starts_with(&loc.path))
            .max_by_key(|loc| loc.path.len())
            .map(|loc| (loc.path.as_str(), loc.name.as_str()))
    }
}

/// Operating system family for a storage profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageProfileOsFamily {
    Windows,
    Linux,
    Macos,
}

impl StorageProfileOsFamily {
    /// Get the OS family for the current host.
    #[cfg(target_os = "windows")]
    pub fn host() -> Self {
        Self::Windows
    }

    /// Get the OS family for the current host.
    #[cfg(target_os = "linux")]
    pub fn host() -> Self {
        Self::Linux
    }

    /// Get the OS family for the current host.
    #[cfg(target_os = "macos")]
    pub fn host() -> Self {
        Self::Macos
    }

    /// Check if this OS family uses POSIX-style paths.
    pub fn is_posix(&self) -> bool {
        matches!(self, Self::Linux | Self::Macos)
    }
}

/// Extended storage profile with identifier and OS family.
///
/// This matches the structure returned by Deadline Cloud APIs:
/// - `deadline.get_storage_profile()`
/// - `deadline.get_storage_profile_for_queue()`
#[derive(Debug, Clone)]
pub struct StorageProfileWithId {
    /// Unique identifier for this storage profile.
    pub storage_profile_id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Operating system family.
    pub os_family: StorageProfileOsFamily,
    /// File system locations in this profile.
    pub file_system_locations: Vec<FileSystemLocation>,
}

impl StorageProfileWithId {
    /// Convert to a basic StorageProfile (without ID/OS info).
    pub fn to_storage_profile(&self) -> StorageProfile {
        StorageProfile {
            file_system_locations: self.file_system_locations.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile() -> StorageProfile {
        StorageProfile::with_locations(vec![
            FileSystemLocation {
                name: "ProjectFiles".into(),
                path: "/mnt/projects".into(),
                location_type: FileSystemLocationType::Local,
            },
            FileSystemLocation {
                name: "SharedAssets".into(),
                path: "/mnt/shared".into(),
                location_type: FileSystemLocationType::Shared,
            },
            FileSystemLocation {
                name: "DeepLocal".into(),
                path: "/mnt/projects/deep/nested".into(),
                location_type: FileSystemLocationType::Local,
            },
        ])
    }

    #[test]
    fn test_local_locations() {
        let profile: StorageProfile = create_test_profile();
        let locals: HashMap<&str, &str> = profile.local_locations();

        assert_eq!(locals.len(), 2);
        assert_eq!(locals.get("/mnt/projects"), Some(&"ProjectFiles"));
        assert_eq!(locals.get("/mnt/projects/deep/nested"), Some(&"DeepLocal"));
    }

    #[test]
    fn test_shared_locations() {
        let profile: StorageProfile = create_test_profile();
        let shared: HashMap<&str, &str> = profile.shared_locations();

        assert_eq!(shared.len(), 1);
        assert_eq!(shared.get("/mnt/shared"), Some(&"SharedAssets"));
    }

    #[test]
    fn test_is_shared() {
        let profile: StorageProfile = create_test_profile();

        assert!(profile.is_shared(Path::new("/mnt/shared/file.txt")));
        assert!(profile.is_shared(Path::new("/mnt/shared/deep/file.txt")));
        assert!(!profile.is_shared(Path::new("/mnt/projects/file.txt")));
        assert!(!profile.is_shared(Path::new("/other/file.txt")));
    }

    #[test]
    fn test_find_local_location_most_specific() {
        let profile: StorageProfile = create_test_profile();

        // Should match the more specific nested location
        let result: Option<(&str, &str)> =
            profile.find_local_location(Path::new("/mnt/projects/deep/nested/file.txt"));
        assert_eq!(result, Some(("/mnt/projects/deep/nested", "DeepLocal")));

        // Should match the parent location
        let result: Option<(&str, &str)> =
            profile.find_local_location(Path::new("/mnt/projects/other/file.txt"));
        assert_eq!(result, Some(("/mnt/projects", "ProjectFiles")));

        // No match
        let result: Option<(&str, &str)> =
            profile.find_local_location(Path::new("/other/file.txt"));
        assert_eq!(result, None);
    }

    #[test]
    fn test_os_family_is_posix() {
        assert!(!StorageProfileOsFamily::Windows.is_posix());
        assert!(StorageProfileOsFamily::Linux.is_posix());
        assert!(StorageProfileOsFamily::Macos.is_posix());
    }
}
