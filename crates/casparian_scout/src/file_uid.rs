//! File identity utilities for scan/move detection.

use crate::types::SourceType;
use std::fs::Metadata;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileUidStrength {
    Strong,
    Weak,
}

#[derive(Debug, Clone)]
pub struct FileUid {
    pub value: String,
    pub strength: FileUidStrength,
}

pub fn compute_file_uid(source_type: &SourceType, full_path: &Path, metadata: &Metadata) -> FileUid {
    match source_type {
        SourceType::Local | SourceType::Smb { .. } => {
            if let Some(value) = strong_uid_from_metadata(metadata) {
                return FileUid {
                    value,
                    strength: FileUidStrength::Strong,
                };
            }

            weak_uid_from_path(full_path)
        }
        SourceType::S3 { .. } => weak_uid_from_path(full_path),
    }
}

pub fn weak_uid_from_path(path: &Path) -> FileUid {
    let normalized = normalize_path_for_uid(path);
    FileUid {
        value: format!("path:{}", normalized),
        strength: FileUidStrength::Weak,
    }
}

pub fn weak_uid_from_path_str(path: &str) -> String {
    weak_uid_from_path(Path::new(path)).value
}

pub fn s3_uid_from_version(bucket: &str, version_id: &str) -> FileUid {
    FileUid {
        value: format!("s3v:{}:{}", bucket, version_id),
        strength: FileUidStrength::Strong,
    }
}

pub fn s3_uid_from_etag(bucket: &str, etag: &str, size: u64) -> FileUid {
    FileUid {
        value: format!("s3e:{}:{}:{}", bucket, etag, size),
        strength: FileUidStrength::Weak,
    }
}

fn normalize_path_for_uid(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    if cfg!(windows) {
        path_str.replace('\\', "/")
    } else {
        path_str.into_owned()
    }
}

fn strong_uid_from_metadata(metadata: &Metadata) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let dev = metadata.dev();
        let ino = metadata.ino();
        return Some(format!("unix:{}:{}", dev, ino));
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let volume = metadata.volume_serial_number() as u64;
        let file_index = metadata.file_index();
        return Some(format!("win:{}:{}", volume, file_index));
    }

    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
