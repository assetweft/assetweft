//! Responsibility: legacy workflow public type paths and concrete composition only.
//! Dependencies: Production use cases and SQLite adapter; caller: Application.
//! Invariants: one policy implementation; callbacks run only after claim commits.
pub use crate::production::workflow_domain::{Outcome, State, Step, StepRecord};
use crate::{
    library::LibraryError,
    production::{workflow_application as app, workflow_sqlite::SqliteWorkflowRuns},
};
use rusqlite::Connection;
pub(crate) fn create(conn: &mut Connection, run: &str, steps: &[Step]) -> Result<(), LibraryError> {
    app::create(&mut SqliteWorkflowRuns(conn), run, steps)
}
pub(crate) fn claim(conn: &mut Connection, run: &str) -> Result<Option<String>, LibraryError> {
    app::claim(&mut SqliteWorkflowRuns(conn), run)
}
pub(crate) fn finish(
    conn: &mut Connection,
    run: &str,
    id: &str,
    outcome: &Outcome,
) -> Result<(), LibraryError> {
    app::finish(&mut SqliteWorkflowRuns(conn), run, id, outcome)
}
pub(crate) fn cancel(conn: &mut Connection, run: &str) -> Result<(), LibraryError> {
    app::cancel(&mut SqliteWorkflowRuns(conn), run)
}
pub(crate) fn run(
    conn: &mut Connection,
    run: &str,
    driver: impl FnOnce(&str) -> Outcome,
) -> Result<bool, LibraryError> {
    app::run(&mut SqliteWorkflowRuns(conn), run, driver)
}
pub(crate) fn read(conn: &Connection, run: &str) -> Result<Vec<StepRecord>, LibraryError> {
    app::read(&SqliteWorkflowRuns(conn), run)
}
pub(crate) fn is_cancelled(conn: &Connection, run: &str) -> Result<bool, LibraryError> {
    app::is_cancelled(&SqliteWorkflowRuns(conn), run)
}
