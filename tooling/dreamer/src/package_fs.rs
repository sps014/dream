use std::fs;
use std::io;
use std::path::Path;

pub(crate) fn remove_entry(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let directory = metadata.is_dir();
    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::FileTypeExt;
        // Windows directory links require directory removal, even when their target is missing.
        directory || metadata.file_type().is_symlink_dir()
    };
    if directory {
        // Rust's directory removal unlinks directory symlinks without traversing their targets.
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_the_selected_entry() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("package");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("source.dream"), "source").unwrap();
        let sibling = temp.path().join("other.dream");
        fs::write(&sibling, "keep").unwrap();
        remove_entry(&directory).unwrap();
        assert!(!directory.exists());
        assert_eq!(fs::read_to_string(&sibling).unwrap(), "keep");
        remove_entry(&sibling).unwrap();
        remove_entry(&sibling).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn removes_live_and_dangling_directory_links_without_touching_sources() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let link = temp.path().join("package");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("source.dream"), "keep").unwrap();
        for dangling in [false, true] {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&source, &link).unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(&source, &link).unwrap();
            if dangling {
                fs::remove_dir_all(&source).unwrap();
            }
            remove_entry(&link).unwrap();
            assert_eq!(
                fs::symlink_metadata(&link).unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
            if !dangling {
                assert_eq!(
                    fs::read_to_string(source.join("source.dream")).unwrap(),
                    "keep"
                );
            }
        }
    }
}
