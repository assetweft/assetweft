//! Responsibility: AW018:A synchronous declared executor contract, not runtime admission.
//! Dependencies: budget Unit/Usage; callers: future runtime and protocol contract tests.
//! Invariants: no IO, credentials resolution, retry, billing math or authorization here.
//! Correlation is opaque and never a provider idempotency guarantee. Artifact references
//! are declarations, not downloaded/verified bytes. Debug redaction is only a local property.
use crate::budget::{Unit, Usage};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidReference,
    Unsupported(Operation),
    InvalidUsage,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Opaque bounded reference. No serialization or raw Display is exposed.
#[derive(Clone, PartialEq, Eq)]
pub struct Reference(String);
impl Reference {
    pub fn new(value: &str) -> Result<Self, Error> {
        if value.is_empty() || value.len() > 256 || !value.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Error::InvalidReference);
        }
        Ok(Self(value.to_owned()))
    }
    /// For a trusted future adapter only; never log this value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Reference([REDACTED])")
    }
}
impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialReference {
    OfficialStore(Reference),
    SystemStore(Reference),
}
impl fmt::Display for CredentialReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("credential:[REDACTED]")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Submit,
    Query,
    Download,
    Cancel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelCapability {
    Unsupported,
    RequestOnly,
    Confirmation,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub backend: Reference,
    pub backend_version: Reference,
    pub profile: Reference,
    pub profile_version: Reference,
    pub submit: bool,
    pub query: bool,
    pub download: bool,
    pub cancel: CancelCapability,
}
/// A bounded specification reference avoids copying prompts or credential values.
/// It is not a validated GenerationSpecRequest or permission to execute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitRequest {
    pub correlation: Reference,
    pub specification: Reference,
    pub credential: Option<CredentialReference>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteState {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelResult {
    Requested,
    Confirmed,
    Unknown,
}
/// Neither Requested nor Confirmed means billing stopped.
#[derive(Clone, PartialEq, Eq)]
pub struct Observation {
    pub state: RemoteState,
    pub job: Option<Reference>,
    pub usage: Vec<Usage>,
}
/// Declared remote artifact reference only; no path, URL fetching or byte validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub reference: Reference,
}

/// Implementations must use stable typed errors, never raw provider diagnostics.
/// This contract supplies no network/process engine or authorization mechanism.
pub trait Backend {
    fn declaration(&self) -> Declaration;
    fn submit(&mut self, request: &SubmitRequest) -> Result<Observation, Error>;
    fn query(&mut self, job: &Reference) -> Result<Observation, Error>;
    fn download(&mut self, artifact: &Reference) -> Result<Artifact, Error>;
    fn cancel(&mut self, job: &Reference) -> Result<CancelResult, Error>;
}
/// One thin checked consumer interface. Snapshots the declaration, not admission.
pub struct Executor<B> {
    backend: B,
    declaration: Declaration,
}
impl<B: Backend> Executor<B> {
    pub fn new(backend: B) -> Self {
        let declaration = backend.declaration();
        Self {
            backend,
            declaration,
        }
    }
    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }
    fn require(&self, operation: Operation) -> Result<(), Error> {
        let supported = match operation {
            Operation::Submit => self.declaration.submit,
            Operation::Query => self.declaration.query,
            Operation::Download => self.declaration.download,
            Operation::Cancel => self.declaration.cancel != CancelCapability::Unsupported,
        };
        if !supported {
            return Err(Error::Unsupported(operation));
        }
        Ok(())
    }
    pub fn submit(&mut self, request: &SubmitRequest) -> Result<Observation, Error> {
        self.require(Operation::Submit)?;
        checked(self.backend.submit(request)?)
    }
    pub fn query(&mut self, job: &Reference) -> Result<Observation, Error> {
        self.require(Operation::Query)?;
        checked(self.backend.query(job)?)
    }
    pub fn download(&mut self, artifact: &Reference) -> Result<Artifact, Error> {
        self.require(Operation::Download)?;
        self.backend.download(artifact)
    }
    pub fn cancel(&mut self, job: &Reference) -> Result<CancelResult, Error> {
        self.require(Operation::Cancel)?;
        let result = self.backend.cancel(job)?;
        if result == CancelResult::Confirmed
            && self.declaration.cancel != CancelCapability::Confirmation
        {
            return Err(Error::Unsupported(Operation::Cancel));
        }
        Ok(result)
    }
}
fn checked(observation: Observation) -> Result<Observation, Error> {
    if observation.usage.len() > 32 {
        return Err(Error::InvalidUsage);
    }
    for usage in &observation.usage {
        if usage.source.is_empty()
            || usage.source.len() > 256
            || usage.source.chars().any(char::is_control)
        {
            return Err(Error::InvalidUsage);
        }
        if let Unit::CurrencyMinor { currency, decimals } = &usage.unit
            && (currency.len() != 3
                || !currency.bytes().all(|b| b.is_ascii_uppercase())
                || *decimals > 18)
        {
            return Err(Error::InvalidUsage);
        }
    }
    Ok(observation)
}

impl fmt::Debug for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Observation")
            .field("state", &self.state)
            .field("job", &self.job)
            .field("usage", &"[REDACTED]")
            .finish()
    }
}
