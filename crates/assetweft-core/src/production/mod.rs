//! Responsibility: bounded Production acceptance core.
//! Dependencies: stable Catalog values and core-owned ports; callers: compatibility facade/adapters.
//! Invariants: no storage or database types enter the core.
pub(crate) mod application;
pub mod domain;
pub(crate) mod ports;
