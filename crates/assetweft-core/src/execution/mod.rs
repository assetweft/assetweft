//! Responsibility: Execution protocol and local fixture context.
//! Dependencies: pure domain/use cases/ports, concrete Consumption adapter at assembly only.
//! Invariants: local declarations are not runtime/provider admission or settlement.
pub mod application;
pub(crate) mod consumption_adapter;
pub mod domain;
pub mod ports;
pub mod runtime_application;
