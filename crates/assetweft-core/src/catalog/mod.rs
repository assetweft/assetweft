//! Responsibility: Catalog ownership of asset heads.
//! Dependencies: domain values and SQLite adapter; callers: Application and candidate orchestration.
//! Invariants: conditional head advance borrows the receipt transaction; no independent commit.
pub mod domain;
pub(crate) mod sqlite_store;
