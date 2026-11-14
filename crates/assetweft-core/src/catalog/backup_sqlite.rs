//! Responsibility: concrete SQLite snapshot/CAS copy, protected lifetime and filesystem publication.
//! Dependencies: SQLite Backup API, existing storage lease, filesystem, clock and hashing.
//! Callers: backup composition; invariants: original IO/error order, lease before DB through publish.
use super::backup_domain::{Limits, Manifest, Object, Progress};
use super::backup_domain::{account_bytes, hash_ok, object_identity};
use super::backup_ports::{FreshRestore, ProtectedBackup};
use crate::backup::BackupError;
use crate::{library, storage};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions as FileOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
struct Run<'a> {
    limits: Limits,
    start: Instant,
    bytes: u64,
    progress: &'a mut dyn FnMut(Progress) -> bool,
}
impl Run<'_> {
    fn check(&mut self, progress: Progress) -> Result<(), BackupError> {
        if self.start.elapsed() >= self.limits.timeout {
            return Err(BackupError::Limit);
        }
        if !(self.progress)(progress) {
            return Err(BackupError::Cancelled);
        }
        Ok(())
    }
    fn stream(
        &mut self,
        path: &Path,
        mut target: Option<&mut File>,
    ) -> Result<Object, BackupError> {
        if !fs::symlink_metadata(path)?.file_type().is_file() {
            return Err(BackupError::Invalid("regular file required"));
        }
        let mut file = File::open(path)?;
        let mut hash = Sha256::new();
        let mut bytes = 0;
        let mut buf = [0; 65536];
        loop {
            self.check(Progress::Bytes(self.bytes))?;
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.bytes = account_bytes(self.bytes, n as u64, self.limits.max_bytes)?;
            bytes += n as u64;
            hash.update(&buf[..n]);
            if let Some(out) = target.as_mut() {
                out.write_all(&buf[..n])?;
            }
        }
        if let Some(out) = target {
            out.sync_all()?;
        }
        Ok(Object {
            hash: hash.finalize().iter().map(|b| format!("{b:02x}")).collect(),
            bytes,
        })
    }
}
fn safe_path(path: &Path) -> Result<(), BackupError> {
    let mut part = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(BackupError::Invalid("parent traversal"));
        }
        part.push(component);
        match fs::symlink_metadata(&part) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(BackupError::Invalid("symlink path"));
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn fresh(path: &Path) -> Result<(), BackupError> {
    safe_path(path)?;
    fs::create_dir(path)?;
    Ok(())
}
fn new_file(path: &Path) -> Result<File, BackupError> {
    Ok(FileOptions::new().write(true).create_new(true).open(path)?)
}
fn inspect(conn: &Connection, max: usize) -> Result<Vec<Object>, BackupError> {
    library::require_current(conn)?;
    let integrity: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if integrity != "ok"
        || conn
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(BackupError::Invalid("database integrity"));
    }
    let mismatch:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM aw_managed_versions m JOIN aw_asset_versions v USING(version_id) WHERE m.content_hash!=v.content_hash)",[],|r|r.get(0))?;
    if mismatch {
        return Err(BackupError::Invalid("managed reference mismatch"));
    }
    let mut stmt =
        conn.prepare("SELECT content_hash,byte_length FROM aw_objects ORDER BY content_hash")?;
    let mut rows = stmt.query([])?;
    let mut objects = Vec::new();
    while let Some(row) = rows.next()? {
        if objects.len() >= max {
            return Err(BackupError::Limit);
        }
        let hash: String = row.get(0)?;
        let bytes: i64 = row.get(1)?;
        object_identity(&hash, bytes)?;
        objects.push(Object {
            hash,
            bytes: bytes as u64,
        });
    }
    Ok(objects)
}
fn copy_objects(
    source: &Path,
    dest: &Path,
    objects: &[Object],
    run: &mut Run<'_>,
) -> Result<(), BackupError> {
    fs::create_dir(dest)?;
    if !objects.is_empty() {
        safe_path(source)?;
    }
    for object in objects {
        if !hash_ok(&object.hash) {
            return Err(BackupError::Invalid("object name"));
        }
        let actual = run.stream(
            &source.join(&object.hash),
            Some(&mut new_file(&dest.join(&object.hash))?),
        )?;
        if actual != *object {
            return Err(BackupError::Invalid("object hash/length"));
        }
    }
    run.check(Progress::ObjectsReady)
}

pub(crate) struct BackupSession<'a> {
    conn: &'a Connection,
    root: &'a Path,
    dest: &'a Path,
    run: Run<'a>,
    guard: Option<storage::Guard>,
}
impl<'a> BackupSession<'a> {
    pub(crate) fn new(
        conn: &'a Connection,
        root: &'a Path,
        dest: &'a Path,
        limits: Limits,
        progress: &'a mut dyn FnMut(Progress) -> bool,
    ) -> Self {
        Self {
            conn,
            root,
            dest,
            run: Run {
                limits,
                start: Instant::now(),
                bytes: 0,
                progress,
            },
            guard: None,
        }
    }
}
impl ProtectedBackup for BackupSession<'_> {
    type Error = BackupError;
    fn snapshot(&mut self) -> Result<(Object, Vec<Object>), BackupError> {
        let (conn, root, dest, limits) = (self.conn, self.root, self.dest, self.run.limits);
        self.guard = Some(storage::reference_guard(root)?);
        storage::bind(conn, root)?;
        library::require_current(conn)?;
        fresh(dest)?;
        let db = dest.join("library.sqlite");
        new_file(&db)?;
        let mut snapshot = Connection::open(&db)?;
        snapshot.busy_timeout(Duration::ZERO)?;
        {
            let backup = Backup::new(conn, &mut snapshot)?;
            loop {
                self.run.check(Progress::Snapshot)?;
                match backup.step(128)? {
                    StepResult::Done => break,
                    StepResult::More => (),
                    StepResult::Busy | StepResult::Locked => return Err(BackupError::Busy),
                    _ => return Err(BackupError::Invalid("backup step")),
                }
                if u64::try_from(backup.progress().pagecount).unwrap_or(u64::MAX)
                    > limits.max_bytes / 512
                {
                    return Err(BackupError::Limit);
                }
            }
        }
        let objects = inspect(&snapshot, limits.max_objects)?;
        // Close/checkpoint before hashing: the snapshot package never depends on a sidecar.
        snapshot.close().map_err(|(_, e)| BackupError::Sql(e))?;
        let database = self.run.stream(&db, None)?;
        Ok((database, objects))
    }
    fn copy_objects(&mut self, objects: &[Object]) -> Result<(), BackupError> {
        copy_objects(
            &self.root.join("objects"),
            &self.dest.join("objects"),
            objects,
            &mut self.run,
        )
    }
    fn publish(&mut self, manifest: &Manifest) -> Result<(), BackupError> {
        publish_manifest(self.dest, &serde_json::to_vec(manifest)?)
    }
}
pub(crate) struct RestoreSession<'a> {
    source: &'a Path,
    dest: &'a Path,
    run: Run<'a>,
    raw: Vec<u8>,
}
impl<'a> RestoreSession<'a> {
    pub(crate) fn new(
        source: &'a Path,
        dest: &'a Path,
        limits: Limits,
        progress: &'a mut dyn FnMut(Progress) -> bool,
    ) -> Result<Self, BackupError> {
        safe_path(source)?;
        safe_path(dest)?;
        Ok(Self {
            source,
            dest,
            run: Run {
                limits,
                start: Instant::now(),
                bytes: 0,
                progress,
            },
            raw: Vec::new(),
        })
    }
}
impl FreshRestore for RestoreSession<'_> {
    type Error = BackupError;
    fn read_manifest(&mut self) -> Result<Manifest, BackupError> {
        let source = self.source;
        let manifest_path = source.join("manifest.json");
        if !fs::symlink_metadata(&manifest_path)?.file_type().is_file() {
            return Err(BackupError::Invalid("manifest file"));
        }
        let mut raw = Vec::new();
        File::open(manifest_path)?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut raw)?;
        if raw.len() > 16 * 1024 * 1024 {
            return Err(BackupError::Limit);
        }
        let manifest: Manifest = serde_json::from_slice(&raw)?;
        self.raw = raw;
        Ok(manifest)
    }
    fn verify_snapshot(&mut self, manifest: &Manifest) -> Result<(), BackupError> {
        let (source, limits) = (self.source, self.run.limits);
        let source_db = source.join("library.sqlite");
        if self.run.stream(&source_db, None)? != manifest.database {
            return Err(BackupError::Invalid("database hash/length"));
        }
        // Refuse sidecars: only a closed single-file snapshot is a valid package.
        for suffix in ["-wal", "-journal", "-shm"] {
            if fs::symlink_metadata(source.join(format!("library.sqlite{suffix}"))).is_ok() {
                return Err(BackupError::Invalid("snapshot sidecar"));
            }
        }
        let snapshot = Connection::open_with_flags(&source_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if inspect(&snapshot, limits.max_objects)? != manifest.objects {
            return Err(BackupError::Invalid("snapshot object set"));
        }
        snapshot.close().map_err(|(_, e)| BackupError::Sql(e))?;
        Ok(())
    }
    fn copy_snapshot(&mut self, manifest: &Manifest) -> Result<(), BackupError> {
        let dest = self.dest;
        let source_db = self.source.join("library.sqlite");
        fresh(dest)?;
        let db = dest.join("library.sqlite");
        if self.run.stream(&source_db, Some(&mut new_file(&db)?))? != manifest.database {
            return Err(BackupError::Invalid("snapshot changed"));
        }
        Ok(())
    }
    fn copy_objects(&mut self, objects: &[Object]) -> Result<(), BackupError> {
        copy_objects(
            &self.source.join("objects"),
            &self.dest.join("objects"),
            objects,
            &mut self.run,
        )
    }
    fn publish_restored(&mut self) -> Result<(), BackupError> {
        let dest = self.dest;
        // Bind the existing relative database filename contract without inheriting a lease.
        let mut owner = new_file(&dest.join(".aw-object-owner"))?;
        owner.write_all(b"library.sqlite")?;
        owner.sync_all()?;
        publish_manifest(dest, &self.raw)?;
        Ok(())
    }
}
// Atomic no-clobber completion marker: partial writes never look like a completed package.
fn publish_manifest(dest: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    let mut temp = tempfile::NamedTempFile::new_in(dest)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(dest.join("manifest.json"))
        .map_err(|e| BackupError::Io(e.error))?;
    Ok(())
}
