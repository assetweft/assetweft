//! Responsibility: local Consumption context ownership.
//! Dependencies: pure core and concrete SQLite adapter; legacy budget composes them.
//! Invariants: local declarations never establish provider authorization.
pub(crate) mod application;
pub mod domain;
pub(crate) mod ports;
pub(crate) mod sqlite;
