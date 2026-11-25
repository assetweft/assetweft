use super::{canonical_f64, quantize_1e6, semantic_sha256_hex};
use crate::LockMode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum FaceMode {
    Required,
    Optional,
    NotApplicable,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    pub face_mode: FaceMode,
    pub expected_expressions: Vec<String>,
    pub eye_jaw_nodes: Option<EyeJawNodes>,
    pub rigid_attachments: Vec<Attachment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EyeJawNodes {
    left_eye: Option<String>,
    right_eye: Option<String>,
    jaw: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Attachment {
    pub socket_node: String,
    pub attachment_node: String,
    expected_offset_m: [f64; 3],
    max_drift_m: f64,
    lock_mode: String,
}

impl Spec {
    pub fn eye_jaw_names(&self) -> impl Iterator<Item = &str> {
        self.eye_jaw_nodes.iter().flat_map(|nodes| {
            [&nodes.left_eye, &nodes.right_eye, &nodes.jaw]
                .into_iter()
                .filter_map(|name| name.as_deref())
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        for attachment in &self.rigid_attachments {
            LockMode::parse(&attachment.lock_mode).map_err(|err| err.to_string())?;
            if canonical_f64(attachment.max_drift_m).map_err(|err| err.to_string())? < 0.0 {
                return Err("max_drift_m must be nonnegative".into());
            }
            for coordinate in attachment
                .expected_offset_m
                .into_iter()
                .chain([attachment.max_drift_m])
            {
                quantize_1e6(coordinate).map_err(|err| err.to_string())?;
            }
        }
        if self
            .expected_expressions
            .iter()
            .map(String::as_str)
            .chain(self.eye_jaw_names())
            .chain(
                self.rigid_attachments
                    .iter()
                    .flat_map(|part| [part.socket_node.as_str(), part.attachment_node.as_str()]),
            )
            .any(|name| name.trim().is_empty())
        {
            return Err("expression and node names must not be blank".into());
        }
        Ok(())
    }
}

pub(super) struct FaceFacts {
    pub morph_target_count: usize,
    pub expressions: BTreeSet<String>,
    pub playable_expressions: BTreeSet<String>,
    pub valid_eye_jaw_nodes: bool,
    pub playable: bool,
}

pub(super) struct AttachmentPlacement {
    pub socket_offset_m: [f64; 3],
    pub world_drift_m: f64,
    pub rigid: bool,
}

#[derive(Serialize)]
pub struct AttachmentResult {
    pub socket_node: String,
    pub attachment_node: String,
    pub lock_mode: String,
    pub expected_offset_m: [f64; 3],
    pub max_drift_m: f64,
    pub offset_m: Option<[f64; 3]>,
    pub drift_m: Option<f64>,
    pub world_drift_m: Option<f64>,
    pub status: &'static str,
    pub violation: Option<&'static str>,
}

impl Attachment {
    pub fn expected_offset(&self) -> [f64; 3] {
        self.expected_offset_m
            .map(|value| quantize_1e6(value).expect("validated spec"))
    }

    pub fn inspect(
        &self,
        placement: Option<AttachmentPlacement>,
    ) -> Result<AttachmentResult, String> {
        let expected_offset_m = self.expected_offset();
        let max_drift_m = quantize_1e6(self.max_drift_m).expect("validated spec");
        let mut report = AttachmentResult {
            socket_node: self.socket_node.clone(),
            attachment_node: self.attachment_node.clone(),
            lock_mode: self.lock_mode.clone(),
            expected_offset_m,
            max_drift_m,
            offset_m: None,
            drift_m: None,
            world_drift_m: None,
            status: "FAIL",
            violation: Some(
                "socket and attachment must be unique nodes in the same socket hierarchy",
            ),
        };
        if let Some(placement) = placement {
            let offset =
                super::canonical_vec3(placement.socket_offset_m).map_err(|err| err.to_string())?;
            let drift =
                quantize_1e6(distance(offset, expected_offset_m)).map_err(|err| err.to_string())?;
            let world_drift =
                quantize_1e6(placement.world_drift_m).map_err(|err| err.to_string())?;
            report.offset_m = Some(offset);
            report.drift_m = Some(drift);
            report.world_drift_m = Some(world_drift);
            report.violation = if !placement.rigid {
                Some(
                    "attachment subtree contains skinning, morph targets, or independently animated nodes",
                )
            } else if drift > max_drift_m || world_drift > max_drift_m {
                Some("attachment exceeds maximum drift")
            } else {
                report.status = "PASS";
                None
            };
        }
        Ok(report)
    }
}

pub(super) fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}

#[derive(Serialize)]
pub struct FaceAttachmentReport {
    pub morph_target_count: usize,
    pub matched_expressions: Vec<String>,
    pub missing_expressions: Vec<String>,
    pub unplayable_expressions: Vec<String>,
    pub eye_jaw_playable: bool,
    pub facial_status: &'static str,
    pub attachment_results: Vec<AttachmentResult>,
    pub semantic_hash: String,
    pub visual_engine_status: &'static str,
}

impl Spec {
    pub fn inspect(
        self,
        facts: FaceFacts,
        attachment_results: Vec<AttachmentResult>,
    ) -> Result<FaceAttachmentReport, String> {
        let has_eye_jaw_requirements = self.eye_jaw_names().next().is_some();
        let mut expressions = self.expected_expressions;
        expressions.sort_unstable();
        expressions.dedup();
        let inapplicable = self.face_mode == FaceMode::NotApplicable
            || (self.face_mode == FaceMode::Optional
                && expressions.is_empty()
                && !has_eye_jaw_requirements
                && facts.morph_target_count == 0);
        let (matched_expressions, missing_expressions): (Vec<_>, Vec<_>) = expressions
            .into_iter()
            .filter(|_| !inapplicable)
            .partition(|name| facts.expressions.contains(name));
        let unplayable_expressions: Vec<_> = matched_expressions
            .iter()
            .filter(|&name| !facts.playable_expressions.contains(name))
            .cloned()
            .collect();
        let facial_status = if inapplicable {
            "INAPPLICABLE"
        } else if facts.playable
            && facts.valid_eye_jaw_nodes
            && missing_expressions.is_empty()
            && unplayable_expressions.is_empty()
        {
            "PASS"
        } else {
            "FAIL"
        };
        let mut report = FaceAttachmentReport {
            morph_target_count: facts.morph_target_count,
            matched_expressions,
            missing_expressions,
            unplayable_expressions,
            eye_jaw_playable: facts.playable && !inapplicable,
            facial_status,
            attachment_results,
            semantic_hash: String::new(),
            visual_engine_status: "NOT_RUN",
        };
        let canonical = serde_json::to_vec(&report).map_err(|err| err.to_string())?;
        let digest: Vec<_> = Sha256::digest(canonical)
            .into_iter()
            .map(f64::from)
            .collect();
        report.semantic_hash = semantic_sha256_hex(&digest).map_err(|err| err.to_string())?;
        Ok(report)
    }
}
