//! Responsibility: produce and evict one reconstructible, version-addressed local export.
//! Depends on the Catalog managed-object verifier, root binding and storage lease.
//! Caller: Application. The private witness pins the copy's identity across process exits.
use super::{domain::validate_version_id, objects_domain::StoredObject, objects_sqlite};
use crate::{
    library::LibraryError,
    storage::{self, Guard, StorageError, redirected},
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub(crate) enum PreviewFormat {
    Png,
    Wav,
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn database_id(path: &Path) -> Option<FileId> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if redirected(&fs::symlink_metadata(parent).ok()?) {
        return None;
    }
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || redirected(&metadata) {
        return None;
    }
    identity(path).ok().map(|(id, _)| id)
}

// All components below the canonical library root are checked separately. In
// particular a bind mount can redirect a regular file or directory on the same device.
fn local_entry(
    root: &Path,
    path: &Path,
    expected: fn(&fs::Metadata) -> bool,
) -> Result<(), StorageError> {
    let meta = fs::symlink_metadata(path)?;
    check_local_entry(root, path, expected, &meta)
}

fn check_local_entry(
    root: &Path,
    path: &Path,
    expected: fn(&fs::Metadata) -> bool,
    meta: &fs::Metadata,
) -> Result<(), StorageError> {
    if !expected(meta) || redirected(meta) || path.canonicalize()? != path {
        return Err(StorageError::Invalid(
            "redirected or unexpected storage entry",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{AtFlags, CWD, StatxAttributes, StatxFlags, statx};
        let info = statx(
            CWD,
            path,
            AtFlags::NO_AUTOMOUNT | AtFlags::SYMLINK_NOFOLLOW,
            StatxFlags::empty(),
        )
        .map_err(io::Error::from)?;
        if !info
            .stx_attributes_mask
            .contains(StatxAttributes::MOUNT_ROOT)
            || info.stx_attributes.contains(StatxAttributes::MOUNT_ROOT)
        {
            return Err(StorageError::Invalid(
                "mounted storage entry or unavailable mount identity",
            ));
        }
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::os::unix::fs::MetadataExt;
        if fs::metadata(root)?.dev() != meta.dev() {
            return Err(StorageError::Invalid("mounted storage entry"));
        }
    }
    #[cfg(any(windows, target_os = "linux"))]
    let _ = root;
    Ok(())
}

fn preview_source_error(error: io::Error) -> StorageError {
    if error.kind() == io::ErrorKind::NotFound {
        StorageError::MissingPreviewSource
    } else {
        error.into()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileId {
    volume: u64,
    index: u64,
}

fn identity(path: &Path) -> io::Result<(FileId, u64)> {
    #[cfg(unix)]
    let (volume, index, links) = {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(path)?;
        (m.dev(), m.ino(), m.nlink())
    };
    #[cfg(windows)]
    let (volume, index, links) = {
        let m = winapi_util::file::information(File::open(path)?)?;
        (
            m.volume_serial_number(),
            m.file_index(),
            m.number_of_links(),
        )
    };
    if index == 0 {
        return Err(io::Error::other("unavailable file identity"));
    }
    Ok((FileId { volume, index }, links))
}

pub(crate) struct Export {
    _lease: Guard,
    database: FileId,
    root: PathBuf,
    name: String,
    source: PathBuf,
    object: StoredObject,
}

impl Export {
    pub(crate) fn open(
        conn: &Connection,
        root: &Path,
        database: Option<FileId>,
        id: &str,
    ) -> Result<Self, StorageError> {
        let session = Self::open_bound(conn, root, database, id)?;
        local_entry(root, &session.source, fs::Metadata::is_file)?;
        objects_sqlite::verify(
            &session.source,
            &session.object.content_hash,
            session.object.byte_length,
        )?;
        Ok(session)
    }

    pub(crate) fn open_bound(
        conn: &Connection,
        root: &Path,
        database: Option<FileId>,
        id: &str,
    ) -> Result<Self, StorageError> {
        validate_version_id(id).map_err(LibraryError::from)?;
        if root.canonicalize()? != root {
            return Err(StorageError::Invalid("redirected library root"));
        }
        let db = conn.path().ok_or(StorageError::Invalid("file database"))?;
        let database = database
            .filter(|expected| Some(*expected) == database_id(Path::new(db)))
            .ok_or(StorageError::Invalid("redirected or replaced database"))?;
        let db_path = Path::new(db).canonicalize()?;
        if db_path.parent() != Some(root) {
            return Err(StorageError::Invalid("database outside library"));
        }
        local_entry(root, &db_path, fs::Metadata::is_file)?;
        let lease = storage::lock(root)?;
        local_entry(root, &root.join(".aw-object-owner"), fs::Metadata::is_file)?;
        storage::check_binding(conn, root)?;
        local_entry(root, &root.join("objects"), fs::Metadata::is_dir)?;
        let object = objects_sqlite::managed_record(conn, id)?
            .ok_or(StorageError::Invalid("verified managed version required"))?;
        let source = root.join("objects").join(&object.content_hash);

        Ok(Self {
            _lease: lease,
            database,
            root: root.into(),
            name: hash(id.as_bytes()),
            source,
            object,
        })
    }

    pub(crate) fn preview(self, format: PreviewFormat) -> Result<PathBuf, StorageError> {
        let meta = fs::symlink_metadata(&self.source).map_err(preview_source_error)?;
        check_local_entry(&self.root, &self.source, fs::Metadata::is_file, &meta)?;
        let length = self.object.byte_length;
        if length > 16 * 1024 * 1024 {
            return Err(StorageError::Invalid(match format {
                PreviewFormat::Png => "PNG input exceeds 16 MiB",
                PreviewFormat::Wav => "WAV input exceeds 16 MiB",
            }));
        }
        let mut source = Vec::new();
        File::open(&self.source)
            .map_err(preview_source_error)?
            .take(length + 1)
            .read_to_end(&mut source)?;
        if source.len() as u64 != length || hash(&source) != self.object.content_hash {
            return Err(StorageError::Corrupt);
        }
        let (suffix, encoded) = match format {
            PreviewFormat::Png => ("v2.png", super::preview::thumbnail(&source)?),
            PreviewFormat::Wav => ("v1.wav", super::wave::audition(&source)?),
        };
        let dir = self.root.join("previews");
        match fs::create_dir(&dir) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            result => result?,
        }
        local_entry(&self.root, &dir, fs::Metadata::is_dir)?;
        let relative = Path::new("previews").join(format!("{}.{suffix}", self.name));
        let target = self.root.join(&relative);
        match local_entry(&self.root, &target, fs::Metadata::is_file) {
            Ok(()) => {
                objects_sqlite::verify(&target, &hash(&encoded), encoded.len() as u64)?;
            }
            Err(StorageError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
                temp.write_all(&encoded)?;
                temp.as_file().sync_all()?;
                temp.persist_noclobber(target).map_err(|e| e.error)?;
            }
            Err(e) => return Err(e),
        }
        Ok(relative)
    }

    fn witness(&self, identity: FileId) -> PathBuf {
        self.root.join(format!(
            ".aw-export-{}-{:x}-{:x}-{:x}-{:x}",
            self.name, self.database.volume, self.database.index, identity.volume, identity.index
        ))
    }

    fn owned(&self, target: &Path) -> Result<Option<PathBuf>, StorageError> {
        match local_entry(&self.root, target, fs::Metadata::is_file) {
            Err(StorageError::Io(e)) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            other => other?,
        }
        let (id, links) = identity(target)?;
        let witness = self.witness(id);
        if links != 2 {
            return Err(StorageError::Invalid("unowned export"));
        }
        match local_entry(&self.root, &witness, fs::Metadata::is_file) {
            Err(StorageError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                return Err(StorageError::Invalid("missing export ownership witness"));
            }
            other => other?,
        }
        if identity(&witness)? != (id, links) {
            return Err(StorageError::Invalid("export witness mismatch"));
        }
        objects_sqlite::verify(target, &self.object.content_hash, self.object.byte_length)?;
        Ok(Some(witness))
    }

    pub(crate) fn materialize(self) -> Result<PathBuf, StorageError> {
        let dir = self.root.join("exports");
        match fs::create_dir(&dir) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            outcome => outcome?,
        }
        local_entry(&self.root, &dir, fs::Metadata::is_dir)?;
        let relative = Path::new("exports").join(&self.name);
        let target = self.root.join(&relative);
        if self.owned(&target)?.is_none() {
            let mut temp = tempfile::NamedTempFile::new_in(&self.root)?;
            io::copy(
                &mut File::open(&self.source)?.take(self.object.byte_length + 1),
                &mut temp,
            )?;
            temp.as_file().sync_all()?;
            objects_sqlite::verify(
                temp.path(),
                &self.object.content_hash,
                self.object.byte_length,
            )?;
            let witness = self.witness(identity(temp.path())?.0);
            temp.persist_noclobber(&witness).map_err(|e| e.error)?;
            // Publish last. A crash here can leave an unreachable private witness.
            if let Err(error) = fs::hard_link(&witness, target) {
                // We just exclusively created this name under the same lease.
                fs::remove_file(witness)?;
                return Err(error.into());
            }
        }
        Ok(relative)
    }

    pub(crate) fn evict(self) -> Result<bool, StorageError> {
        let dir = self.root.join("exports");
        if fs::symlink_metadata(&dir).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
            return Ok(false);
        }
        local_entry(&self.root, &dir, fs::Metadata::is_dir)?;
        let target = dir.join(&self.name);
        let Some(witness) = self.owned(&target)? else {
            return Ok(false);
        };
        fs::remove_file(target)?;
        fs::remove_file(witness)?;
        Ok(true)
    }
}
