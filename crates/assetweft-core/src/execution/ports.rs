//! Responsibility: Execution backend and durable once-only budget capabilities.
//! Dependencies: domain values only; checked/runtime use cases consume these ports.
//! Invariants: once callback follows durable claim, never replayed; no DB handles escape.
use super::domain::*;
use crate::consumption::domain::Request;

/// Implementations must use stable typed errors, never raw provider diagnostics.
/// This contract supplies no network/process engine or authorization mechanism.
pub trait Backend {
    fn declaration(&self) -> Declaration;
    fn submit(&mut self, request: &SubmitRequest) -> Result<Observation, Error>;
    fn query(&mut self, job: &Reference) -> Result<Observation, Error>;
    fn download(&mut self, artifact: &Reference) -> Result<Artifact, Error>;
    fn cancel(&mut self, job: &Reference) -> Result<CancelResult, Error>;
}

/// Local fixture reservation and once-only submission, supplied by Consumption.
/// Implementations preserve committed-claim -> callback -> UNKNOWN ordering.
pub trait BudgetOnce {
    type Error;
    fn reserve(&mut self, request: &Request) -> Result<(), Self::Error>;
    fn submit_once(&mut self, id: &str, callback: impl FnOnce()) -> Result<bool, Self::Error>;
}
