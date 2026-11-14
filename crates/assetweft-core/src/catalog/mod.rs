//! Responsibility: Catalog ownership of immutable metadata, exact references and asset heads.
//! Dependencies: domain values and SQLite adapter; callers: Application and candidate orchestration.
//! Invariants: conditional head advance borrows the receipt transaction; no independent commit.
pub mod domain;
pub(crate) mod sqlite_store;

pub(crate) mod application;
pub(crate) mod ports;
pub(crate) mod sqlite_metadata;

pub(crate) mod feedback;
pub(crate) mod sqlite_feedback;

pub(crate) mod objects_application;
pub mod objects_domain;
pub(crate) mod objects_ports;
pub(crate) mod objects_sqlite;

pub(crate) mod backup_application;
pub mod backup_domain;
pub(crate) mod backup_ports;
pub(crate) mod backup_sqlite;
pub(crate) mod head_application;
pub(crate) mod head_ports;
pub(crate) mod head_sqlite;
