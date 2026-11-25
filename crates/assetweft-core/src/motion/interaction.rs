use super::digest;
use crate::assurance::{ValidationLimits, canonical_f64, read_bounded, semantic_sha256_hex};
use serde::{Deserialize, Serialize};
use std::{error::Error, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    scope: String,
    actors: Vec<Actor>,
    interaction: Interaction,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Actor {
    actor_id: String,
    timestamps_ms: Vec<u64>,
    root_positions_m: Vec<[f64; 3]>,
    anchor_positions_m: Vec<[f64; 3]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Interaction {
    actor_a: String,
    actor_b: String,
    contact_window_frames: [usize; 2],
    separation_window_frames: [usize; 2],
    max_contact_distance_m: f64,
    min_separation_distance_m: f64,
    forbidden_interpenetration_m: f64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    interaction_sha256: String,
    pub failure: Option<Failure>,
    visual_engine_status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "reason")]
pub enum Failure {
    ClockDesync {
        actor_id: String,
    },
    ContactMiss {
        frame: usize,
        distance_m: f64,
    },
    Interpenetration {
        frame: usize,
        distance_m: f64,
    },
    InsufficientSeparation {
        frame: usize,
        anchor_distance_m: f64,
        root_distance_m: f64,
    },
}

impl Spec {
    fn validate(&mut self) -> Result<(), Box<dyn Error>> {
        if self.scope != "shared_clock_kinematic" {
            return Err("scope must be shared_clock_kinematic".into());
        }
        self.actors
            .sort_unstable_by(|a, b| a.actor_id.cmp(&b.actor_id));
        if self.actors.len() < 2
            || self.actors.iter().any(|a| a.actor_id.trim().is_empty())
            || self
                .actors
                .windows(2)
                .any(|pair| pair[0].actor_id == pair[1].actor_id)
        {
            return Err("at least two distinct non-empty actor IDs are required".into());
        }
        for actor in &mut self.actors {
            let clock = &actor.timestamps_ms;
            if clock.is_empty()
                || clock.windows(2).any(|pair| pair[0] >= pair[1])
                || clock.len() != actor.root_positions_m.len()
                || clock.len() != actor.anchor_positions_m.len()
            {
                return Err(
                    "each actor requires an increasing clock and one root and anchor per timestamp"
                        .into(),
                );
            }
            for coordinate in actor
                .root_positions_m
                .iter_mut()
                .chain(&mut actor.anchor_positions_m)
                .flatten()
            {
                *coordinate = canonical_f64(*coordinate)?;
            }
        }
        let rule = &mut self.interaction;
        for bound in [
            &mut rule.max_contact_distance_m,
            &mut rule.min_separation_distance_m,
            &mut rule.forbidden_interpenetration_m,
        ] {
            *bound = canonical_f64(*bound)?;
            if *bound < 0.0 {
                return Err("distance bounds must be non-negative".into());
            }
        }
        if rule.forbidden_interpenetration_m > rule.max_contact_distance_m {
            return Err("minimum contact distance exceeds the maximum".into());
        }
        let [contact_start, contact_end] = rule.contact_window_frames;
        let [separation_start, separation_end] = rule.separation_window_frames;
        if contact_start > contact_end
            || separation_start > separation_end
            || (contact_start <= separation_end && separation_start <= contact_end)
        {
            return Err("frame windows must be ordered inclusive ranges and disjoint".into());
        }
        Ok(())
    }

    fn semantic_hash(&self) -> Result<String, Box<dyn Error>> {
        let rule = &self.interaction;
        let clocks: Vec<_> = self
            .actors
            .iter()
            .map(|a| (&a.actor_id, &a.timestamps_ms))
            .collect();
        let metadata = serde_json::to_vec(&(
            "assetweft:motion-interaction:1",
            &self.scope,
            clocks,
            &rule.actor_a,
            &rule.actor_b,
            rule.contact_window_frames,
            rule.separation_window_frames,
        ))?;
        // Hash the exact metadata before encoding its fixed-length digest as semantic values.
        let mut values: Vec<_> = digest(&metadata).bytes().map(f64::from).collect();
        values.extend([
            rule.max_contact_distance_m,
            rule.min_separation_distance_m,
            rule.forbidden_interpenetration_m,
        ]);
        for actor in &self.actors {
            values.extend(
                actor
                    .root_positions_m
                    .iter()
                    .chain(&actor.anchor_positions_m)
                    .flatten()
                    .copied(),
            );
        }
        Ok(semantic_sha256_hex(&values)?)
    }

    fn check(&self) -> Result<Option<Failure>, &'static str> {
        let rule = &self.interaction;
        if rule.actor_a == rule.actor_b {
            return Err("the interaction requires two distinct actors");
        }
        let actor = |id: &str| self.actors.iter().find(|actor| actor.actor_id == id);
        let a = actor(&rule.actor_a).ok_or("actor_a is not in actors")?;
        let b = actor(&rule.actor_b).ok_or("actor_b is not in actors")?;
        if let Some(actor) = self
            .actors
            .iter()
            .find(|actor| actor.timestamps_ms != a.timestamps_ms)
        {
            return Ok(Some(Failure::ClockDesync {
                actor_id: actor.actor_id.clone(),
            }));
        }
        let [contact_start, contact_end] = rule.contact_window_frames;
        let [separation_start, separation_end] = rule.separation_window_frames;
        if contact_end.max(separation_end) >= a.timestamps_ms.len() {
            return Err("frame windows must be within the shared timeline");
        }
        for frame in contact_start..=contact_end {
            let distance_m = distance(a.anchor_positions_m[frame], b.anchor_positions_m[frame]);
            if distance_m < rule.forbidden_interpenetration_m {
                return Ok(Some(Failure::Interpenetration { frame, distance_m }));
            }
            if distance_m > rule.max_contact_distance_m {
                return Ok(Some(Failure::ContactMiss { frame, distance_m }));
            }
        }
        for frame in separation_start..=separation_end {
            let anchor_distance_m =
                distance(a.anchor_positions_m[frame], b.anchor_positions_m[frame]);
            let root_distance_m = distance(a.root_positions_m[frame], b.root_positions_m[frame]);
            if anchor_distance_m.min(root_distance_m) < rule.min_separation_distance_m {
                return Ok(Some(Failure::InsufficientSeparation {
                    frame,
                    anchor_distance_m,
                    root_distance_m,
                }));
            }
        }
        Ok(None)
    }
}

fn distance([x, y, z]: [f64; 3], [a, b, c]: [f64; 3]) -> f64 {
    (x - a).hypot(y - b).hypot(z - c)
}

pub fn assess(path: &Path, limits: ValidationLimits) -> Result<Report, Box<dyn Error>> {
    let bytes = read_bounded(path, limits)?;
    let mut spec: Spec = serde_json::from_slice(&bytes)?;
    spec.validate()?;
    let interaction_sha256 = spec.semantic_hash()?;
    Ok(Report {
        interaction_sha256,
        failure: spec.check()?,
        visual_engine_status: "NOT_RUN",
    })
}
