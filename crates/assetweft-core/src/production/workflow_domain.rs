//! Responsibility: Production workflow graph, state values and pure transition policy.
//! Dependencies: std only; callers: workflow use cases and SQLite translation.
//! Invariants: ordinal ready selection, successful prerequisites, no unresolved replay.
use std::collections::{HashMap, HashSet};

/// Caller-declared stable step and its explicit prerequisite IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub dependencies: Vec<String>,
}
/// The driver's actual local outcome and optional evidence pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Success {
        raw: String,
        evidence: Option<String>,
    },
    Failure {
        raw: String,
        evidence: Option<String>,
    },
}
/// Running means claimed but unresolved and requires explicit reconciliation; it is never automatically rerun.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pending,
    Running,
    Succeeded,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRecord {
    pub id: String,
    pub state: State,
    pub outcome: Option<Outcome>,
}
/// Stable policy diagnostic, translated to the existing database category at composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyError(pub String);
fn invalid(s: &str) -> PolicyError {
    PolicyError(s.to_owned())
}
fn id_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}

pub(crate) fn validate_graph(run: &str, steps: &[Step]) -> Result<(), PolicyError> {
    if !id_ok(run) || steps.is_empty() || steps.len() > 64 {
        return Err(invalid("invalid run id or step count (1..=64)"));
    }
    let mut ids = HashMap::new();
    for (i, s) in steps.iter().enumerate() {
        if !id_ok(&s.id) || s.dependencies.len() > 64 || ids.insert(s.id.as_str(), i).is_some() {
            return Err(invalid("invalid or duplicate step id / dependency count"));
        }
    }
    let mut indegree = vec![0usize; steps.len()];
    let mut edges = vec![Vec::new(); steps.len()];
    for (i, s) in steps.iter().enumerate() {
        let mut seen = HashSet::new();
        for d in &s.dependencies {
            let j = *ids
                .get(d.as_str())
                .ok_or_else(|| invalid(&format!("unknown dependency {d:?} for {:?}", s.id)))?;
            if !seen.insert(j) {
                return Err(invalid("duplicate dependency"));
            }
            indegree[i] += 1;
            edges[j].push(i);
        }
    }
    let mut queue: Vec<usize> = (0..steps.len()).filter(|i| indegree[*i] == 0).collect();
    let mut n = 0;
    while let Some(i) = queue.pop() {
        n += 1;
        for &j in &edges[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                queue.push(j);
            }
        }
    }
    if n != steps.len() {
        return Err(invalid("dependency cycle"));
    }
    Ok(())
}

pub(crate) fn validate_outcome(outcome: &Outcome) -> Result<(), PolicyError> {
    let (Outcome::Success { raw, evidence } | Outcome::Failure { raw, evidence }) = outcome;
    if raw.len() > 1_048_576 || evidence.as_ref().is_some_and(|x| x.len() > 4096) {
        return Err(invalid("outcome exceeds bounds"));
    }
    Ok(())
}
/// Records are in declared ordinal order. Dependencies are stable step identities.
pub(crate) fn ready_step<'a>(
    cancelled: bool,
    records: &'a [StepRecord],
    dependencies: &[(String, String)],
) -> Option<&'a str> {
    if cancelled || records.iter().any(|r| r.state == State::Running) {
        return None;
    }
    records
        .iter()
        .find(|r| {
            r.state == State::Pending
                && dependencies
                    .iter()
                    .filter(|(step, _)| step == &r.id)
                    .all(|(_, dependency)| {
                        records
                            .iter()
                            .any(|p| &p.id == dependency && p.state == State::Succeeded)
                    })
        })
        .map(|r| r.id.as_str())
}

pub(crate) fn decode_record(
    id: String,
    s: &str,
    raw: Option<String>,
    evidence: Option<String>,
) -> Result<StepRecord, PolicyError> {
    let (state, outcome) = match (s, raw) {
        ("PENDING", None) if evidence.is_none() => (State::Pending, None),
        ("RUNNING", None) if evidence.is_none() => (State::Running, None),
        ("SUCCEEDED", Some(raw)) => (State::Succeeded, Some(Outcome::Success { raw, evidence })),
        ("FAILED", Some(raw)) => (State::Failed, Some(Outcome::Failure { raw, evidence })),
        _ => return Err(invalid("invalid persisted step state/result")),
    };
    Ok(StepRecord { id, state, outcome })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: &str, state: State) -> StepRecord {
        StepRecord {
            id: id.into(),
            state,
            outcome: None,
        }
    }
    #[test]
    fn ready_policy_preserves_ordinal_success_dependencies_and_unresolved_block() {
        let dependencies = vec![("b".into(), "a".into())];
        let mut records = vec![
            record("a", State::Pending),
            record("b", State::Pending),
            record("c", State::Pending),
        ];
        assert_eq!(ready_step(false, &records, &dependencies), Some("a"));
        records[0].state = State::Running;
        assert_eq!(ready_step(false, &records, &dependencies), None);
        records[0].state = State::Failed;
        assert_eq!(ready_step(false, &records, &dependencies), Some("c"));
        records[0].state = State::Succeeded;
        assert_eq!(ready_step(false, &records, &dependencies), Some("b"));
        assert_eq!(ready_step(true, &records, &dependencies), None);
    }
    #[test]
    fn state_result_policy_rejects_inconsistent_persisted_combinations() {
        assert!(decode_record("a".into(), "RUNNING", Some("raw".into()), None).is_err());
        assert!(decode_record("a".into(), "PENDING", None, Some("e".into())).is_err());
        assert!(decode_record("a".into(), "SUCCEEDED", None, None).is_err());
        assert_eq!(
            decode_record("a".into(), "FAILED", Some("raw".into()), Some("e".into()))
                .unwrap()
                .outcome,
            Some(Outcome::Failure {
                raw: "raw".into(),
                evidence: Some("e".into())
            })
        );
    }
}
