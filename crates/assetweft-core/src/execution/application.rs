//! Responsibility: declared synchronous checked Execution use cases.
//! Dependencies: domain and Backend port; old executor path re-exports these SAME types.
//! Invariants: unsupported operations precede backend calls; precision remains 0..18.
use super::{domain::*, ports::Backend};
use crate::consumption::domain::Unit;

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
