//! Atomic file replacement shared by stores with different lifetimes. Callers
//! retain their transaction/attempt locks and decide their durability contract.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

#[derive(Clone, Copy)]
pub(crate) enum Durability {
    ReplaceOnly,
    FileAndDirectory,
}

pub(crate) fn replace(path: &Path, bytes: &[u8], durability: Durability) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("missing storage directory"))?;
    let temporary = directory.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        {
            let _measurement =
                crate::performance::span(crate::performance::Phase::FileWrite).bytes(bytes.len());
            file.write_all(bytes)?;
        }
        if matches!(durability, Durability::FileAndDirectory) {
            let _measurement = crate::performance::span(crate::performance::Phase::FileSync);
            file.sync_all()?;
        }
        {
            let _measurement = crate::performance::span(crate::performance::Phase::FileRename);
            fs::rename(&temporary, path)?;
        }
        if matches!(durability, Durability::FileAndDirectory) {
            let _measurement = crate::performance::span(crate::performance::Phase::DirectorySync);
            File::open(directory)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_keeps_unrelated_files_and_cleans_failed_temporary_write() {
        let root = std::env::temp_dir().join(format!("dustroute-atomic-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let saved = root.join("record.json");
        replace(&saved, b"first", Durability::ReplaceOnly).unwrap();
        replace(&saved, b"second", Durability::FileAndDirectory).unwrap();
        assert_eq!(fs::read(&saved).unwrap(), b"second");
        let blocked = root.join("directory");
        fs::create_dir(&blocked).unwrap();
        assert!(
            replace(
                &blocked,
                b"cannot replace directory",
                Durability::FileAndDirectory
            )
            .is_err()
        );
        assert_eq!(fs::read(&saved).unwrap(), b"second");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }
}
