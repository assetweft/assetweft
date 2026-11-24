use assetweft_core::{
    assurance as val, backup::BackupError, library::LibraryError, sqlite::ProtoError,
    storage::StorageError,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn envelope(
    command: &str,
    call_id: Option<&String>,
    status: &str,
    errors: Value,
    results: Value,
) -> Value {
    json!({"schema_version":1,"command":command,"call_id":call_id,"status":status,
        "errors":errors,"results":results})
}
pub(super) fn error(path: &str, code: &str, message: impl ToString) -> Value {
    json!({"field_path":path,"code":code,"message":message.to_string()})
}
pub(super) fn read_json<T: DeserializeOwned>(
    path: &std::path::Path,
    limits: val::ValidationLimits,
    field: &str,
) -> Result<T, Value> {
    let bytes = val::read_bounded(path, limits).map_err(|e| error(field, "INVALID_INPUT", e))?;
    serde_json::from_slice(&bytes).map_err(|e| error(field, "INVALID_INPUT", e))
}
pub(super) fn source(s: &val::SourceDeclaration) -> Value {
    json!({"class":s.class.as_str(),"declared_license":s.declared_license,"verified_by_checker":s.verified_by_checker()})
}
pub(super) fn existing_database(path: &Path) -> Result<(), Value> {
    if path.as_os_str().is_empty() {
        return Err(error(
            "db",
            "INVALID_INPUT",
            "--db must name an existing regular file",
        ));
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(error(
            "db",
            "INVALID_INPUT",
            "--db must be a regular file, not a directory or special file",
        )),
        Err(e) => Err(error(
            "db",
            "INVALID_INPUT",
            format!("--db does not exist or cannot be read: {e}"),
        )),
    }
}
pub(super) fn database_open_error(
    command: &str,
    call_id: Option<&String>,
    failure: ProtoError,
) -> (i32, Value) {
    let (code, status) = match failure {
        ProtoError::Busy(_) | ProtoError::Io(_) => (6, "INTERNAL"),
        _ => (2, "INVALID_INPUT"),
    };
    (
        code,
        envelope(
            command,
            call_id,
            status,
            json!([error("db", status, failure)]),
            Value::Null,
        ),
    )
}

pub(super) fn library_outcome(failure: &LibraryError) -> (i32, &'static str) {
    match failure {
        LibraryError::InvalidInput(_) | LibraryError::Constraint(_) => (2, "INVALID_INPUT"),
        LibraryError::Uninitialized => (3, "INPUT_INCOMPLETE"),
        LibraryError::FutureSchema(_) => (5, "UNSUPPORTED"),
        LibraryError::Busy(_) | LibraryError::Io(_) => (6, "INTERNAL"),
        LibraryError::Database(_) | LibraryError::UnknownValue(_) => (1, "FAIL"),
    }
}

pub(super) fn backup_outcome(failure: &BackupError) -> (i32, &'static str) {
    match failure {
        BackupError::Library(e) | BackupError::Storage(StorageError::Library(e)) => {
            library_outcome(e)
        }
        BackupError::Invalid(_)
        | BackupError::Json(_)
        | BackupError::Limit
        | BackupError::Storage(StorageError::Invalid(_)) => (2, "INVALID_INPUT"),
        BackupError::Io(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (2, "INVALID_INPUT"),
        BackupError::Storage(StorageError::MissingPreviewSource) => (3, "INPUT_INCOMPLETE"),
        BackupError::Io(_)
        | BackupError::Busy
        | BackupError::Storage(StorageError::Busy | StorageError::Io(_)) => (6, "INTERNAL"),
        BackupError::Sql(_)
        | BackupError::Cancelled
        | BackupError::Storage(StorageError::Corrupt) => (1, "FAIL"),
    }
}
