use finiky::filesystem::directory::DirectoryFileSystem;
use finiky::filesystem::tarfs::TarFileSystem;
use finiky::filesystem::FileSystem;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_directory_filesystem_read() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, b"Hello, World!").unwrap();

    let fs = DirectoryFileSystem::new(temp_dir.path()).unwrap();
    let content = fs.read_file("test.txt").await.unwrap();
    assert_eq!(content, b"Hello, World!");
}

#[tokio::test]
async fn test_directory_filesystem_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let fs = DirectoryFileSystem::new(temp_dir.path()).unwrap();

    assert!(!fs.exists("nonexistent.txt").await);
    assert!(fs.read_file("nonexistent.txt").await.is_err());
}

#[tokio::test]
async fn test_directory_filesystem_listing() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("file1.txt"), b"").unwrap();
    fs::write(temp_dir.path().join("file2.txt"), b"").unwrap();
    fs::create_dir(temp_dir.path().join("subdir")).unwrap();

    let fs = DirectoryFileSystem::new(temp_dir.path()).unwrap();
    let entries = fs.list_dir("").await.unwrap();

    assert!(entries.contains(&"file1.txt".to_string()));
    assert!(entries.contains(&"file2.txt".to_string()));
    assert!(entries.contains(&"subdir".to_string()));
}

#[tokio::test]
async fn test_tar_filesystem_read() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::Builder;

    let temp_dir = TempDir::new().unwrap();
    let tar_path = temp_dir.path().join("test.tar.gz");
    let file = std::fs::File::create(&tar_path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    let mut header = tar::Header::new_gnu();
    header.set_path("test.txt").unwrap();
    header.set_size(13);
    header.set_cksum();
    tar.append(&header, &b"Hello, World!"[..]).unwrap();
    let encoder = tar.into_inner().unwrap();
    let _file = encoder.finish().unwrap();

    let fs = TarFileSystem::new(&tar_path).unwrap();
    let content = fs.read_file("test.txt").await.unwrap();
    assert_eq!(content, b"Hello, World!");
}

#[tokio::test]
async fn test_tar_filesystem_directory() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::Builder;

    let temp_dir = TempDir::new().unwrap();
    let tar_path = temp_dir.path().join("test.tar.gz");
    let file = std::fs::File::create(&tar_path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    // Add a top-level file to prevent strip_root_directory from stripping "dir"
    let mut header = tar::Header::new_gnu();
    header.set_path("root.txt").unwrap();
    header.set_size(4);
    header.set_cksum();
    tar.append(&header, &b"root"[..]).unwrap();

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

    let fs = TarFileSystem::new(&tar_path).unwrap();
    assert!(fs.exists("dir/").await);
    assert!(fs.exists("dir").await);
    let entries = fs.list_dir("dir").await.unwrap();
    assert!(entries.contains(&"file.txt".to_string()));
}
