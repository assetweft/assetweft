use super::domain::*;
use crate::consumption::domain::Request;

pub trait Backend {
    fn declaration(&self) -> Declaration;
    fn submit(&mut self, request: &SubmitRequest) -> Result<Observation, Error>;
    fn query(&mut self, job: &Reference) -> Result<Observation, Error>;
    fn download(&mut self, artifact: &Reference) -> Result<Artifact, Error>;
    fn cancel(&mut self, job: &Reference) -> Result<CancelResult, Error>;
}

pub trait BudgetOnce {
    type Error;
    fn reserve(&mut self, request: &Request) -> Result<(), Self::Error>;
    fn submit_once(&mut self, id: &str, callback: impl FnOnce()) -> Result<bool, Self::Error>;
}
