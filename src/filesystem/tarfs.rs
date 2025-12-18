use super::{FileSystem, FileSystemError};
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Arc;
use tar::Archive;
use tracing as log;

struct TarEntry {
    data: Vec<u8>,
    is_dir: bool,
}

pub struct TarFileSystem {
    entries: Arc<HashMap<String, TarEntry>>,
}

impl TarFileSystem {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, FileSystemError> {
        let file = File::open(path.as_ref()).map_err(FileSystemError::Io)?;
        let decoder = GzDecoder::new(BufReader::new(file));
        let mut archive = Archive::new(decoder);

        let mut entries = HashMap::new();

        for entry_result in archive
            .entries()
            .map_err(|e| FileSystemError::Archive(e.to_string()))?
        {
            let mut entry = entry_result.map_err(|e| FileSystemError::Archive(e.to_string()))?;

            let path = entry
                .path()
                .map_err(|e| FileSystemError::Archive(e.to_string()))?
                .to_string_lossy()
                .to_string();

            // Normalize path (remove leading ./ and handle directory entries)
            let normalized_path = path.trim_start_matches("./").to_string();

            let header = entry.header();
            let entry_type = header.entry_type();

            if entry_type.is_dir() {
                // Store directory entry
                let dir_path = if normalized_path.ends_with('/') {
                    normalized_path
                } else {
                    format!("{}/", normalized_path)
                };
                entries.insert(
                    dir_path.clone(),
                    TarEntry {
                        data: Vec::new(),
                        is_dir: true,
                    },
                );
            } else if entry_type.is_file() {
                // Read file content
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .map_err(|e| FileSystemError::Archive(e.to_string()))?;

                entries.insert(
                    normalized_path.clone(),
                    TarEntry {
                        data,
                        is_dir: false,
                    },
                );
            }
        }

        log::debug!(
            "Raw entries before stripping: {:?}",
            entries.keys().collect::<Vec<_>>()
        );

        // Detect and strip common root directory
        let entries = Self::strip_root_directory(entries);

        log::debug!(
            "Final entries after stripping: {:?}",
            entries.keys().collect::<Vec<_>>()
        );
        log::debug!("Loaded {} entries from tar.gz", entries.len());

        Ok(TarFileSystem {
            entries: Arc::new(entries),
        })
    }

    /// Detect if all entries are under a single root directory and strip it
    fn strip_root_directory(entries: HashMap<String, TarEntry>) -> HashMap<String, TarEntry> {
        if entries.is_empty() {
            return entries;
        }

        // Find all top-level directories
        let mut top_level_dirs = std::collections::HashSet::new();
        let mut has_top_level_files = false;

        for path in entries.keys() {
            if let Some(first_component) = path.split('/').next() {
                if first_component.is_empty() {
                    continue;
                }
                // Check if this is a top-level file or directory
                if path.contains('/') {
                    top_level_dirs.insert(first_component.to_string());
                } else {
                    has_top_level_files = true;
                }
            }
        }

        log::debug!(
            "Root detection: top_level_dirs={:?}, has_top_level_files={}",
            top_level_dirs,
            has_top_level_files
        );

        // Only strip if:
        // 1. There's exactly one top-level directory
        // 2. There are no top-level files
        if top_level_dirs.len() == 1 && !has_top_level_files {
            let root_dir = top_level_dirs.iter().next().unwrap();
            let prefix = format!("{}/", root_dir);

            log::info!(
                "Detected single root directory '{}', stripping it from paths",
                root_dir
            );

            let mut new_entries = HashMap::new();
            for (path, entry) in entries {
                if let Some(stripped) = path.strip_prefix(&prefix) {
                    // Skip the root directory entry itself
                    if !stripped.is_empty() {
                        log::debug!("Stripping '{}' -> '{}'", path, stripped);
                        new_entries.insert(stripped.to_string(), entry);
                    }
                } else if path == *root_dir || path == prefix.trim_end_matches('/') {
                    // Skip the root directory entry
                    log::debug!("Skipping root directory entry: '{}'", path);
                    continue;
                } else {
                    // This shouldn't happen, but keep the entry just in case
                    log::warn!("Path '{}' doesn't match expected pattern", path);
                    new_entries.insert(path, entry);
                }
            }
            new_entries
        } else {
            log::debug!(
                "Not stripping root: found {} top-level dirs, has_top_level_files={}",
                top_level_dirs.len(),
                has_top_level_files
            );
            // Multiple top-level directories or top-level files exist, don't strip
            entries
        }
    }

    fn normalize_path(&self, path: &str) -> String {
        path.trim_start_matches('/').to_string()
    }
}

#[async_trait::async_trait]
impl FileSystem for TarFileSystem {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, FileSystemError> {
        let normalized = self.normalize_path(path);

        match self.entries.get(&normalized) {
            Some(entry) if !entry.is_dir => Ok(entry.data.clone()),
            Some(_) => Err(FileSystemError::NotFound(format!(
                "{} is a directory",
                path
            ))),
            None => Err(FileSystemError::NotFound(path.to_string())),
        }
    }

    async fn exists(&self, path: &str) -> bool {
        let normalized = self.normalize_path(path);
        self.entries.contains_key(&normalized)
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<String>, FileSystemError> {
        let normalized = self.normalize_path(path);
        let prefix = if normalized.is_empty() {
            String::new()
        } else if normalized.ends_with('/') {
            normalized
        } else {
            format!("{}/", normalized)
        };

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for entry_path in self.entries.keys() {
            if entry_path.starts_with(&prefix) {
                let relative = entry_path.strip_prefix(&prefix).unwrap();

                // Get the first component (file or subdirectory name)
                let first_component = relative.split('/').next().unwrap();

                if !seen.contains(first_component) {
                    seen.insert(first_component.to_string());

                    // Check if it's a directory by looking for entries with this prefix
                    let sub_prefix = format!("{}{}/", prefix, first_component);
                    let is_dir = self.entries.keys().any(|k| k.starts_with(&sub_prefix));

                    entries.push(if is_dir {
                        format!("{}/", first_component)
                    } else {
                        first_component.to_string()
                    });
                }
            }
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::fs::File;
    use tar::Builder;
    use tempfile::TempDir;

    fn create_test_tar(temp_dir: &TempDir) -> std::path::PathBuf {
        let tar_path = temp_dir.path().join("test.tar.gz");
        let file = File::create(&tar_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_path("test.txt").unwrap();
        header.set_size(12);
        header.set_cksum();
        tar.append(&header, &b"test content"[..]).unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("dir/").unwrap();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_cksum();
        tar.append(&header, &[][..]).unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("dir/file.txt").unwrap();
        header.set_size(8);
        header.set_cksum();
        tar.append(&header, &b"dir file"[..]).unwrap();

        let encoder = tar.into_inner().unwrap();
        let _file = encoder.finish().unwrap();
        tar_path
    }

    #[tokio::test]
    async fn test_tar_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        let tar_file = create_test_tar(&temp_dir);
        let fs = TarFileSystem::new(&tar_file).unwrap();

        assert!(fs.exists("test.txt").await);
        assert!(!fs.exists("nonexistent.txt").await);

        let content = fs.read_file("test.txt").await.unwrap();
        assert_eq!(content, b"test content");
    }

    #[tokio::test]
    async fn test_tar_directory_listing() {
        let temp_dir = TempDir::new().unwrap();
        let tar_file = create_test_tar(&temp_dir);
        let fs = TarFileSystem::new(&tar_file).unwrap();

        let entries = fs.list_dir("").await.unwrap();
        assert!(entries.contains(&"test.txt".to_string()));
        assert!(entries.iter().any(|e| e.starts_with("dir")));

        let dir_entries = fs.list_dir("dir").await.unwrap();
        assert!(dir_entries.contains(&"file.txt".to_string()));
    }

    fn create_test_tar_with_root_dir(temp_dir: &TempDir) -> std::path::PathBuf {
        let tar_path = temp_dir.path().join("test_with_root.tar.gz");
        let file = File::create(&tar_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(encoder);

        // Create a tar with a single root directory
        let mut header = tar::Header::new_gnu();
        header.set_path("myapp/").unwrap();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_cksum();
        tar.append(&header, &[][..]).unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("myapp/config.txt").unwrap();
        header.set_size(6);
        header.set_cksum();
        tar.append(&header, &b"config"[..]).unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("myapp/subdir/").unwrap();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_cksum();
        tar.append(&header, &[][..]).unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("myapp/subdir/file.txt").unwrap();
        header.set_size(4);
        header.set_cksum();
        tar.append(&header, &b"data"[..]).unwrap();

        let encoder = tar.into_inner().unwrap();
        let _file = encoder.finish().unwrap();
        tar_path
    }

    #[tokio::test]
    async fn test_tar_strip_root_directory() {
        let temp_dir = TempDir::new().unwrap();
        let tar_file = create_test_tar_with_root_dir(&temp_dir);
        let fs = TarFileSystem::new(&tar_file).unwrap();

        // The root directory should be stripped, so we can access files directly
        assert!(fs.exists("config.txt").await);
        assert!(!fs.exists("myapp/config.txt").await);

        let content = fs.read_file("config.txt").await.unwrap();
        assert_eq!(content, b"config");

        // Test subdirectory access
        assert!(fs.exists("subdir/file.txt").await);
        let content = fs.read_file("subdir/file.txt").await.unwrap();
        assert_eq!(content, b"data");

        // Test directory listing
        let entries = fs.list_dir("").await.unwrap();
        assert!(entries.contains(&"config.txt".to_string()));
        assert!(entries.iter().any(|e| e.starts_with("subdir")));

        let subdir_entries = fs.list_dir("subdir").await.unwrap();
        assert!(subdir_entries.contains(&"file.txt".to_string()));
    }
}
