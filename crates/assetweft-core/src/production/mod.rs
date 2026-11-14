//! Responsibility: bounded Production acceptance core.
//! Dependencies: stable Catalog values and core-owned ports; callers: compatibility facade/adapters.
//! Invariants: no storage or database types enter the core.
pub(crate) mod application;
pub mod domain;
pub(crate) mod initialization_application;
pub(crate) mod initialization_ports;
#[path = "../sqlite/initialization.rs"]
pub(crate) mod initialization_sqlite;
pub(crate) mod ports;
pub(crate) mod staging_application;
pub mod staging_domain;
pub(crate) mod staging_ports;
pub(crate) mod staging_sqlite;
pub(crate) mod workflow_application;
pub mod workflow_domain;
pub(crate) mod workflow_ports;
pub(crate) mod workflow_sqlite;

/// Pure generation declarations, validation policy and unexecuted recipe examples.
pub mod specification;
