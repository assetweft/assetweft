//! Responsibility: legacy SAME-type public protocol facade.
//! Dependencies: Execution core; callers: existing protocol/runtime clients.
//! Invariants: all protocol policy lives in Execution, no duplicate implementation.
pub use crate::execution::{
    application::Executor,
    domain::{
        Artifact, CancelCapability, CancelResult, CredentialReference, Declaration, Error,
        Observation, Operation, Reference, RemoteState, SubmitRequest,
    },
    ports::Backend,
};
