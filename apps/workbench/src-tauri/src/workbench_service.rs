use assetweft_core::{
    LockMode,
    application::Application,
    assurance::{
        RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits, ValidationReport, checks,
    },
    budget,
    candidate::{Acceptance, AcceptanceReport, Candidate, Current, FixtureContract, Proposal},
    execution::byo_gpu,
    history::{Decision, Feedback, FeedbackRecord, HistoryQuery},
    library::{AssetVersion, ComponentReference, ComponentRole, LibraryError, Presence},
    motion::{interaction, retarget},
    storage::{FileAvailability, StorageError, StoredObject},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const STATIC_NOTICE: &str = "Static Mesh Preview · Skin/Morph/Animation NOT_RUN";

type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Debug, Serialize)]
pub struct ServiceError {
    pub code: &'static str,
    pub message: String,
}

impl ServiceError {
    fn new(code: &'static str, message: impl ToString) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }

    fn input(message: impl ToString) -> Self {
        Self::new("INVALID_INPUT", message)
    }
}

impl From<LibraryError> for ServiceError {
    fn from(error: LibraryError) -> Self {
        let code = match error {
            LibraryError::Uninitialized => "INPUT_INCOMPLETE",
            LibraryError::FutureSchema(_) => "UNSUPPORTED",
            LibraryError::InvalidInput(_) | LibraryError::Constraint(_) => "INVALID_INPUT",
            LibraryError::Busy(_) => "BUSY",
            _ => "LIBRARY_ERROR",
        };
        Self::new(code, error)
    }
}

impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Library(error) => error.into(),
            StorageError::Invalid(message) => Self::input(message),
            StorageError::Busy => Self::new("BUSY", error),
            StorageError::Corrupt => Self::new("CORRUPT", error),
            StorageError::MissingPreviewSource => Self::new("MISSING", error),
            StorageError::Io(_) => Self::new("IO", error),
        }
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(error: std::io::Error) -> Self {
        Self::new("IO", error)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySessionDto {
    db_path: PathBuf,
    schema_version: i64,
    mode: &'static str,
    capabilities: Vec<Capability>,
}

#[derive(Serialize)]
struct Capability {
    id: &'static str,
    status: &'static str,
    description: &'static str,
}

fn capabilities() -> Vec<Capability> {
    [
        (
            "local_catalog",
            "READY",
            "Open, import, list and search local versions.",
        ),
        ("glb_static_preview", "READY", STATIC_NOTICE),
        ("png_wav_preview", "READY", "Local image and audio preview."),
        ("validation_report", "READY", "Core checks for GLB roles."),
        (
            "feedback_history",
            "READY",
            "Version-bound Accept and Reject history.",
        ),
        (
            "generation_incremental_ui",
            "READY",
            "Budget allowances, remote UNKNOWN reconciliation, BYO GPU opt-in execution, and incremental candidate acceptance. Paid remote APIs remain NOT_RUN.",
        ),
        (
            "professional_capability_views",
            "READY",
            "Local inspection with parameters for implemented checks.",
        ),
        (
            "visual_engine_playback",
            "NOT_RUN",
            "Unity, Unreal and Godot playback, GPU cloth solving and ASR playback are unavailable.",
        ),
        (
            "cloud_sync_and_login",
            "DISABLED",
            "Local-only SQLite. Zero login required.",
        ),
    ]
    .into_iter()
    .map(|(id, status, description)| Capability {
        id,
        status,
        description,
    })
    .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilitySpec {
    id: &'static str,
    label: &'static str,
    task_code: Option<&'static str>,
    implemented: bool,
    status: &'static str,
    allowed_params: &'static [&'static str],
    primary_label: Option<&'static str>,
    secondary_label: Option<&'static str>,
    secondary_required: bool,
    not_run: Vec<String>,
    reason: Option<&'static str>,
}

#[derive(Serialize)]
pub struct CapabilityOverviewDto {
    implemented: Vec<CapabilitySpec>,
    unimplemented: Vec<CapabilitySpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunCapabilityRequest {
    capability_id: String,
    primary_path: String,
    secondary_path: Option<String>,
    max_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRunResultDto {
    capability_id: &'static str,
    task_code: &'static str,
    status: &'static str,
    violations: Vec<String>,
    not_run: Vec<String>,
    digest: Option<String>,
    report: serde_json::Value,
}

const JSON_SPEC: &str = "JSON spec path";
const ENGINE: &str = "visual_engine_status";
const QUALITY: &str = "visual_quality_status";
const ASR: &str = "asr_playback_status";

impl WorkbenchService {
    pub fn capability_overview(&self) -> Result<CapabilityOverviewDto> {
        let implemented = [
            (
                "motion-retarget",
                "Motion retargeting",
                "AW-023:B",
                JSON_SPEC,
                None,
                ENGINE,
            ),
            (
                "motion-interaction",
                "Motion interaction",
                "AW-023:C",
                JSON_SPEC,
                None,
                ENGINE,
            ),
            (
                "sprite-inspect",
                "Sprite sheet",
                "AW-024:A",
                "PNG sheet path",
                Some((JSON_SPEC, true)),
                "semantic_quality_status",
            ),
            (
                "card-template-check",
                "Card template",
                "AW-024:B",
                "JSON template path",
                Some(("PNG illustration path", false)),
                QUALITY,
            ),
            (
                "face-attachment-inspect",
                "Face and attachments",
                "AW-025:A",
                "GLB path",
                Some((JSON_SPEC, true)),
                ENGINE,
            ),
            (
                "cloth-inspect",
                "Cloth",
                "AW-025:B",
                "GLB path",
                Some((JSON_SPEC, true)),
                ENGINE,
            ),
            (
                "vfx-inspect",
                "Visual effects",
                "AW-026:A",
                JSON_SPEC,
                Some(("PNG flipbook path", false)),
                ENGINE,
            ),
            (
                "action-presentation-inspect",
                "Action presentation",
                "AW-026:B",
                JSON_SPEC,
                None,
                "engine_playback_status",
            ),
            (
                "audio-inspect",
                "Audio",
                "AW-027:A",
                "WAV path",
                Some((JSON_SPEC, false)),
                ASR,
            ),
            (
                "static-kit-inspect",
                "Static kit",
                "AW-028:A",
                "GLB path",
                Some((JSON_SPEC, true)),
                QUALITY,
            ),
        ]
        .into_iter()
        .map(
            |(id, label, task_code, primary_label, secondary, boundary)| CapabilitySpec {
                id,
                label,
                task_code: Some(task_code),
                implemented: true,
                status: "READY",
                allowed_params: if secondary.is_some() {
                    &["primaryPath", "secondaryPath", "maxBytes"]
                } else {
                    &["primaryPath", "maxBytes"]
                },
                primary_label: Some(primary_label),
                secondary_label: secondary.map(|(label, _)| label),
                secondary_required: secondary.is_some_and(|(_, required)| required),
                not_run: vec![format!("{boundary}: NOT_RUN")],
                reason: None,
            },
        )
        .collect();

        let unimplemented = [
            (
                "unity_live_import",
                "Unity live import",
                Some("AW-029"),
                "Unity live import and playback are unavailable in this workspace.",
            ),
            (
                "unreal_live_import",
                "Unreal live import",
                Some("AW-030"),
                "Unreal live import and playback are unavailable in this workspace.",
            ),
            (
                "godot_live_import",
                "Godot live import",
                Some("AW-031"),
                "Godot live import and playback are unavailable in this workspace.",
            ),
            (
                "realtime_cloth_gpu_solver",
                "Real-time cloth solver",
                None,
                "The GPU cloth runtime is unavailable.",
            ),
            (
                "asr_voice_playback",
                "ASR voice playback",
                None,
                "The speech recognition playback runtime is unavailable.",
            ),
            (
                "rig_skin_live_deformer",
                "Live rig deformation",
                None,
                "The live skin, morph and animation runtime is unavailable.",
            ),
        ]
        .into_iter()
        .map(|(id, label, task_code, reason)| CapabilitySpec {
            id,
            label,
            task_code,
            implemented: false,
            status: "NOT_RUN",
            allowed_params: &[],
            primary_label: None,
            secondary_label: None,
            secondary_required: false,
            not_run: Vec::new(),
            reason: Some(reason),
        })
        .collect();

        Ok(CapabilityOverviewDto {
            implemented,
            unimplemented,
        })
    }

    pub fn run_capability_check(
        &self,
        req: RunCapabilityRequest,
    ) -> Result<CapabilityRunResultDto> {
        let spec = self
            .capability_overview()?
            .implemented
            .into_iter()
            .find(|spec| spec.id == req.capability_id)
            .ok_or_else(|| ServiceError::new("UNSUPPORTED", "Capability is NOT_RUN or unknown."))?;

        if !(1..=MAX_BYTES).contains(&req.max_bytes) {
            return Err(ServiceError::input(format!(
                "maxBytes must be 1..={MAX_BYTES}"
            )));
        }
        let primary = req.primary_path.trim();
        if primary.is_empty() {
            return Err(ServiceError::input("primaryPath is required"));
        }
        if req.secondary_path.is_some() && spec.secondary_label.is_none() {
            return Err(ServiceError::input(
                "secondaryPath is unsupported for this capability",
            ));
        }
        let primary = Path::new(primary);
        let secondary = req
            .secondary_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(Path::new);
        let required_spec =
            || secondary.ok_or_else(|| ServiceError::input("secondaryPath is required"));
        let limits = ValidationLimits {
            max_bytes: req.max_bytes,
        };
        let json = |result: std::result::Result<serde_json::Value, serde_json::Error>| {
            result.map_err(|error| ServiceError::new("INTERNAL", error))
        };

        let (report, violations, digest_key, applicable) = match spec.id {
            "motion-retarget" => {
                let report = retarget::assess(primary, limits).map_err(ServiceError::input)?;
                let violations = report.failed_stage.into_iter().map(str::to_owned).collect();
                (
                    json(serde_json::to_value(report))?,
                    violations,
                    "input_sha256",
                    true,
                )
            }
            "motion-interaction" => {
                let report = interaction::assess(primary, limits).map_err(ServiceError::input)?;
                let violations = report
                    .failure
                    .iter()
                    .map(|failure| format!("{failure:?}"))
                    .collect();
                (
                    json(serde_json::to_value(report))?,
                    violations,
                    "interaction_sha256",
                    true,
                )
            }
            "sprite-inspect" => {
                match checks::inspect_sprite_file(primary, required_spec()?, limits) {
                    Ok(report) => (json(serde_json::to_value(report))?, vec![], "", true),
                    Err(checks::SpriteError::Contract(detail)) => {
                        (serde_json::Value::Null, vec![detail.into()], "", true)
                    }
                    Err(checks::SpriteError::InvalidInput { field, detail }) => {
                        return Err(ServiceError::input(format!("{field}: {detail}")));
                    }
                }
            }
            "card-template-check" => {
                match checks::check_card_template_file(primary, secondary, limits) {
                    Ok(report) => (
                        json(serde_json::to_value(report))?,
                        vec![],
                        "template_layout_sha256",
                        true,
                    ),
                    Err(checks::CardLayoutError::Contract(violation)) => (
                        serde_json::Value::Null,
                        vec![format!("{violation:?}")],
                        "",
                        true,
                    ),
                    Err(checks::CardLayoutError::InvalidInput { field, detail }) => {
                        return Err(ServiceError::input(format!("{field}: {detail}")));
                    }
                }
            }
            "face-attachment-inspect" => {
                let report =
                    checks::inspect_face_attachment_file(primary, required_spec()?, limits)
                        .map_err(|e| ServiceError::input(format!("{}: {}", e.field, e.detail)))?;
                let mut violations: Vec<_> = report
                    .attachment_results
                    .iter()
                    .filter_map(|part| {
                        part.violation
                            .map(|detail| format!("{}: {detail}", part.attachment_node))
                    })
                    .collect();
                if report.facial_status == "FAIL" {
                    violations.push("Facial requirements are not playable".into());
                }
                let applicable =
                    report.facial_status != "INAPPLICABLE" || !report.attachment_results.is_empty();
                (
                    json(serde_json::to_value(report))?,
                    violations,
                    "semantic_hash",
                    applicable,
                )
            }
            "cloth-inspect" => {
                let report = checks::inspect_cloth_file(primary, required_spec()?, limits)
                    .map_err(|e| ServiceError::input(format!("{}: {}", e.field, e.detail)))?;
                let violations = report
                    .garments
                    .iter()
                    .flat_map(|garment| {
                        garment
                            .violations
                            .iter()
                            .map(|violation| format!("{}: {violation:?}", garment.node_name))
                    })
                    .collect();
                (
                    json(serde_json::to_value(report))?,
                    violations,
                    "semantic_hash",
                    true,
                )
            }
            "vfx-inspect" => {
                let report = checks::inspect_vfx_file(primary, secondary, limits)
                    .map_err(|e| ServiceError::input(format!("{}: {}", e.field, e.detail)))?;
                let violations = report
                    .violations
                    .iter()
                    .map(|v| format!("{}: {}", v.field, v.detail))
                    .collect();
                (
                    json(serde_json::to_value(report))?,
                    violations,
                    "vfx_spec_sha256",
                    true,
                )
            }
            "action-presentation-inspect" => {
                let report = checks::inspect_action_presentation_file(primary, limits)
                    .map_err(ServiceError::input)?;
                let violations = report
                    .violations
                    .iter()
                    .map(|v| format!("{}: {} ({})", v.field, v.detail, v.code))
                    .collect();
                (
                    json(serde_json::to_value(report))?,
                    violations,
                    "presentation_sha256",
                    true,
                )
            }
            "audio-inspect" => {
                let report = checks::inspect_audio_file(primary, secondary, limits)
                    .map_err(|e| ServiceError::input(format!("{}: {}", e.field, e.detail)))?;
                let violations = report.violations.iter().map(|v| (*v).to_owned()).collect();
                (
                    json(serde_json::to_value(report))?,
                    violations,
                    "pcm_sha256",
                    true,
                )
            }
            "static-kit-inspect" => {
                let report = checks::inspect_static_kit_file(primary, required_spec()?, limits)
                    .map_err(|e| ServiceError::input(format!("{}: {}", e.field, e.detail)))?;
                let violations = report
                    .violations
                    .iter()
                    .map(|v| format!("{}: {}", v.field, v.detail))
                    .collect();
                (json(serde_json::to_value(report))?, violations, "", true)
            }
            _ => {
                return Err(ServiceError::new(
                    "UNSUPPORTED",
                    "Capability is NOT_RUN or unknown.",
                ));
            }
        };
        let status = if !violations.is_empty() {
            "FAIL"
        } else if applicable {
            "PASS"
        } else {
            "INAPPLICABLE"
        };

        Ok(CapabilityRunResultDto {
            capability_id: spec.id,
            task_code: spec
                .task_code
                .expect("implemented capabilities have a task code"),
            status,
            violations,
            not_run: spec.not_run,
            digest: report
                .get(digest_key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            report,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Availability {
    Available,
    MetadataOnly,
    Missing,
    Corrupt,
}

fn availability(app: &mut Application, id: &str) -> Result<Availability> {
    Ok(match app.file_availability(id) {
        Ok(FileAvailability::Available) => Availability::Available,
        Ok(FileAvailability::MetadataOnly) => Availability::MetadataOnly,
        Ok(FileAvailability::Missing) => Availability::Missing,
        Ok(FileAvailability::UnknownVersion) => return Err(ServiceError::input("Unknown version")),
        Err(StorageError::Corrupt) => Availability::Corrupt,
        Err(error) => return Err(error.into()),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionRow {
    asset_id: String,
    version_id: String,
    source_format: String,
    availability: Availability,
    latest_decision: &'static str,
}

#[derive(Serialize)]
pub struct VersionListDto {
    matches: Vec<VersionRow>,
    truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDetailDto {
    asset_id: String,
    version_id: String,
    source_format: String,
    content_hash: String,
    source_class: &'static str,
    source: String,
    license_id: String,
    usage_scope: String,
    availability: Availability,
    byte_length: Option<u64>,
    references: Vec<ReferenceDto>,
    current: Option<Current>,
    latest_feedback: Option<FeedbackRecordDto>,
    history: Vec<FeedbackRecordDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceDto {
    role: &'static str,
    component_version_id: String,
}

impl From<ComponentReference> for ReferenceDto {
    fn from(reference: ComponentReference) -> Self {
        let role = match reference.role {
            ComponentRole::Mesh => "mesh",
            ComponentRole::Materials => "materials",
            ComponentRole::Rig => "rig",
            ComponentRole::Motion => "motion",
            ComponentRole::Vfx => "vfx",
            ComponentRole::Audio => "audio",
            ComponentRole::Dialogue => "dialogue",
        };
        Self {
            role,
            component_version_id: reference.component_version_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationDto {
    checker_version: &'static str,
    file_sha256: String,
    bytes_read: usize,
    format: &'static str,
    format_status: &'static str,
    role: ValidationRole,
    checks: Vec<ValidationCheck>,
    not_run: Vec<&'static str>,
    units: &'static str,
    source: ValidationSource,
}

#[derive(Serialize)]
struct ValidationRole {
    requirement: &'static str,
    satisfied: Option<bool>,
    present: Vec<&'static str>,
    missing: Vec<&'static str>,
    status: &'static str,
}

#[derive(Serialize)]
struct ValidationCheck {
    id: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationSource {
    class: &'static str,
    declared_license: Option<String>,
    verified: bool,
}

impl From<ValidationReport> for ValidationDto {
    fn from(report: ValidationReport) -> Self {
        Self {
            checker_version: report.checker_version,
            file_sha256: report.file_sha256,
            bytes_read: report.bytes_read,
            format: report.format,
            format_status: report.format_status.as_str(),
            role: ValidationRole {
                requirement: report.role.requirement.as_str(),
                satisfied: report.role.satisfied,
                present: report.role.present,
                missing: report.role.missing,
                status: report.role.status.as_str(),
            },
            checks: report
                .checks
                .into_iter()
                .map(|item| ValidationCheck {
                    id: item.id,
                    status: item.status.as_str(),
                    detail: item.detail,
                })
                .collect(),
            not_run: report.not_run,
            units: report.units,
            source: ValidationSource {
                class: report.source.class.as_str(),
                verified: report.source.verified_by_checker(),
                declared_license: report.source.declared_license,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum PreviewDto {
    Unavailable {
        version_id: String,
        availability: Availability,
        reason: &'static str,
    },
    Unsupported {
        version_id: String,
        source_format: String,
        availability: Availability,
        reason: &'static str,
    },
    Glb {
        version_id: String,
        byte_length: u64,
        content_hash: String,
        glb_base64: String,
        validation: ValidationDto,
        notice: &'static str,
    },
    Png {
        version_id: String,
        data_url: String,
        preview_kind: &'static str,
        note: Option<&'static str>,
    },
    Wav {
        version_id: String,
        data_url: String,
        byte_length: u64,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppendFeedbackRequest {
    feedback_id: String,
    asset_id: String,
    version_id: String,
    decision: String,
    reason: Option<String>,
}

fn feedback_decision(decision: Decision) -> &'static str {
    match decision {
        Decision::Accept => "ACCEPT",
        Decision::Reject => "REJECT",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackRecordDto {
    sequence: i64,
    feedback_id: String,
    asset_id: String,
    version_id: String,
    decision: &'static str,
    reason: Option<String>,
}

impl From<FeedbackRecord> for FeedbackRecordDto {
    fn from(record: FeedbackRecord) -> Self {
        Self {
            sequence: record.sequence,
            feedback_id: record.feedback.feedback_id,
            asset_id: record.feedback.asset_id,
            version_id: record.feedback.version_id,
            decision: feedback_decision(record.feedback.decision),
            reason: record.feedback.reason,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportLocalFileRequest {
    file_path: PathBuf,
    asset_id: String,
    version_id: String,
    source_format: String,
    license_id: String,
    usage_scope: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    version_id: String,
    content_hash: String,
    byte_length: u64,
}

struct OpenedLibrary {
    db_path: PathBuf,
    app: Application,
}

impl OpenedLibrary {
    fn version(&mut self, id: &str) -> Result<AssetVersion> {
        self.app
            .asset_version(id)?
            .ok_or_else(|| ServiceError::input("Unknown version"))
    }

    fn root(&self) -> Result<&Path> {
        self.db_path
            .parent()
            .ok_or_else(|| ServiceError::input("Database parent required"))
    }

    fn managed(&mut self, id: &str) -> Result<StoredObject> {
        self.app
            .managed_object(id)?
            .ok_or_else(|| ServiceError::new("INPUT_INCOMPLETE", "Managed bytes unavailable"))
    }

    fn validate(
        &self,
        path: &Path,
        version: &AssetVersion,
        role: RoleRequirement,
    ) -> Result<ValidationDto> {
        let source = SourceDeclaration {
            class: source_class(version),
            declared_license: Some(version.license_id.clone()),
        };
        let report = self
            .app
            .validate_file(path, role, source, ValidationLimits::default())
            .map_err(|error| ServiceError::new("VALIDATION_ERROR", error))?;
        if report.file_sha256 != version.content_hash {
            return Err(ServiceError::new(
                "CORRUPT",
                "Validation bytes differ from the stored hash",
            ));
        }
        Ok(report.into())
    }
}

fn source_class(version: &AssetVersion) -> SourceClass {
    SourceClass::parse(&version.source).unwrap_or(SourceClass::Unknown)
}

fn read_bounded(path: &Path) -> Result<(Vec<u8>, String)> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || !(1..=MAX_BYTES).contains(&metadata.len()) {
        return Err(ServiceError::input(
            "A regular nonempty file of at most 16 MiB is required",
        ));
    }
    let mut file = File::open(path)?.take(MAX_BYTES + 1);
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        digest.update(&buffer[..count]);
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(ServiceError::input("File length changed during read"));
    }
    Ok((
        bytes,
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    ))
}

#[derive(Default)]
pub struct WorkbenchService {
    opened: Option<OpenedLibrary>,
}

impl WorkbenchService {
    pub fn open_library(
        &mut self,
        db_path: &Path,
        create_if_missing: bool,
    ) -> Result<LibrarySessionDto> {
        if create_if_missing {
            match fs::symlink_metadata(db_path) {
                Ok(meta) if !meta.is_file() => {
                    return Err(ServiceError::input("Database must be a regular file"));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let mut app = if create_if_missing {
            Application::open(db_path)
        } else {
            Application::open_existing(db_path)
        }
        .map_err(|error| ServiceError::new("DATABASE_OPEN_FAILED", error))?;
        if create_if_missing {
            app.initialize_library()?;
        }
        let schema_version = app.current_library_schema_version()?;
        let db_path = db_path.canonicalize()?;
        self.opened = Some(OpenedLibrary {
            db_path: db_path.clone(),
            app,
        });
        Ok(LibrarySessionDto {
            db_path,
            schema_version,
            mode: "LOCAL_SQLITE_NO_LOGIN",
            capabilities: capabilities(),
        })
    }

    fn opened(&mut self) -> Result<&mut OpenedLibrary> {
        self.opened
            .as_mut()
            .ok_or_else(|| ServiceError::new("INPUT_INCOMPLETE", "Open a local library first"))
    }

    pub fn list_versions(&mut self, query: Option<&str>, limit: u16) -> Result<VersionListDto> {
        let app = &mut self.opened()?.app;
        if !(1..=100).contains(&limit) {
            return Err(ServiceError::input("List limit must be between 1 and 100"));
        }
        let mut results = match query.map(str::trim).filter(|query| !query.is_empty()) {
            Some(query) => app.search_versions(query)?,
            None => app.list_versions(limit)?,
        };
        let truncated = results.truncated || results.matches.len() > usize::from(limit);
        results.matches.truncate(usize::from(limit));
        let matches = results
            .matches
            .into_iter()
            .map(|row| {
                let availability = availability(app, &row.version_id)?;
                let latest_decision = app
                    .latest_feedback(&row.asset_id, Some(&row.version_id))?
                    .map_or("UNRATED", |record| {
                        feedback_decision(record.feedback.decision)
                    });
                Ok(VersionRow {
                    asset_id: row.asset_id,
                    version_id: row.version_id,
                    source_format: row.source_format,
                    availability,
                    latest_decision,
                })
            })
            .collect::<Result<_>>()?;
        Ok(VersionListDto { matches, truncated })
    }

    pub fn get_version_detail(&mut self, id: &str) -> Result<VersionDetailDto> {
        let opened = self.opened()?;
        let version = opened.version(id)?;
        let source_class = source_class(&version).as_str();
        let app = &mut opened.app;
        let availability = availability(app, id)?;
        let byte_length = if availability == Availability::Available {
            app.managed_object(id)?.map(|object| object.byte_length)
        } else {
            None
        };
        let references = app
            .component_references(id)?
            .into_iter()
            .map(Into::into)
            .collect();
        let current = app.candidate_current(&version.asset_id)?;
        let latest_feedback = app
            .latest_feedback(&version.asset_id, Some(id))?
            .map(Into::into);
        let history = app
            .feedback_history(HistoryQuery {
                asset_id: &version.asset_id,
                version_id: Some(id),
                before_sequence: None,
                limit: 20,
            })?
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(VersionDetailDto {
            asset_id: version.asset_id,
            version_id: version.version_id,
            source_format: version.source_format,
            content_hash: version.content_hash,
            source_class,
            source: version.source,
            license_id: version.license_id,
            usage_scope: version.usage_scope,
            availability,
            byte_length,
            references,
            current,
            latest_feedback,
            history,
        })
    }

    pub fn load_preview(&mut self, id: &str) -> Result<PreviewDto> {
        let opened = self.opened()?;
        let version = opened.version(id)?;
        let version_id = version.version_id.clone();
        let availability = availability(&mut opened.app, id)?;
        let reason = match availability {
            Availability::Available => None,
            Availability::MetadataOnly => Some("This version has metadata only."),
            Availability::Missing => Some("The managed file is missing."),
            Availability::Corrupt => Some("Core reported corrupt managed bytes."),
        };
        if let Some(reason) = reason {
            return Ok(PreviewDto::Unavailable {
                version_id,
                availability,
                reason,
            });
        }
        let format = version.source_format.to_ascii_uppercase();
        if !matches!(format.as_str(), "GLB" | "GLTF_GLB" | ".GLB" | "PNG" | "WAV") {
            return Ok(PreviewDto::Unsupported {
                version_id,
                source_format: version.source_format,
                availability,
                reason: "Preview supports GLB, PNG and WAV.",
            });
        }
        let object = opened.managed(id)?;
        let path = opened.root()?.join("objects").join(&object.content_hash);
        let (bytes, hash) = read_bounded(&path)?;
        if hash != object.content_hash {
            return Err(ServiceError::new(
                "CORRUPT",
                "Preview bytes differ from the stored hash",
            ));
        }
        Ok(match format.as_str() {
            "PNG" => {
                let (bytes, preview_kind, note) = match opened.app.preview(id) {
                    Ok(relative) => (
                        read_bounded(&opened.root()?.join(relative))?.0,
                        "CORE_THUMBNAIL",
                        None,
                    ),
                    Err(StorageError::Invalid(
                        note @ ("8-bit RGBA PNG required" | "unsupported PNG color declaration"),
                    )) => (bytes, "CAS_RAW", Some(note)),
                    Err(error) => return Err(error.into()),
                };
                PreviewDto::Png {
                    version_id,
                    data_url: format!("data:image/png;base64,{}", STANDARD.encode(bytes)),
                    preview_kind,
                    note,
                }
            }
            "WAV" => {
                opened.app.preview(id)?;
                PreviewDto::Wav {
                    version_id,
                    data_url: format!("data:audio/wav;base64,{}", STANDARD.encode(bytes)),
                    byte_length: object.byte_length,
                }
            }
            _ => {
                let validation = opened.validate(&path, &version, RoleRequirement::StaticMesh)?;
                PreviewDto::Glb {
                    version_id,
                    byte_length: object.byte_length,
                    content_hash: hash,
                    glb_base64: STANDARD.encode(bytes),
                    validation,
                    notice: STATIC_NOTICE,
                }
            }
        })
    }

    pub fn validate_version(&mut self, id: &str, role: &str) -> Result<ValidationDto> {
        let role = match role {
            "static-mesh" => RoleRequirement::StaticMesh,
            "rigged-character" => RoleRequirement::RiggedCharacter,
            "animated-character" => RoleRequirement::AnimatedCharacter,
            _ => return Err(ServiceError::input("Unknown validation role")),
        };
        let opened = self.opened()?;
        let version = opened.version(id)?;
        let object = opened.managed(id)?;
        let path = opened.root()?.join("objects").join(object.content_hash);
        opened.validate(&path, &version, role)
    }

    pub fn append_feedback(&mut self, request: AppendFeedbackRequest) -> Result<FeedbackRecordDto> {
        let decision = match request.decision.as_str() {
            "ACCEPT" => Decision::Accept,
            "REJECT" => Decision::Reject,
            _ => return Err(ServiceError::input("Decision must be ACCEPT or REJECT")),
        };
        if request.feedback_id.trim().is_empty() {
            return Err(ServiceError::input("Feedback ID is required"));
        }
        let reason = request
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .map(str::to_owned);
        Ok(self
            .opened()?
            .app
            .append_feedback(&Feedback {
                feedback_id: request.feedback_id,
                asset_id: request.asset_id,
                version_id: request.version_id,
                decision,
                reason,
            })?
            .into())
    }

    pub fn import_local_file(
        &mut self,
        request: ImportLocalFileRequest,
    ) -> Result<ImportResultDto> {
        let opened = self.opened()?;
        let (bytes, content_hash) = read_bounded(&request.file_path)?;
        let length = bytes.len() as u64;
        let version = AssetVersion {
            asset_id: request.asset_id,
            input_id: request.version_id.clone(),
            version_id: request.version_id,
            content_hash,
            source_format: request.source_format,
            license_id: request.license_id,
            usage_scope: request.usage_scope,
            source: "IMPORTED".into(),
            authorization: "local-user-import".into(),
            authorized: false,
            contract_version: "1.0.0".into(),
            presence: Presence::Unknown,
        };
        let stored =
            opened
                .app
                .commit_object(&version, &mut Cursor::new(bytes), length, MAX_BYTES)?;
        Ok(ImportResultDto {
            version_id: version.version_id,
            content_hash: stored.content_hash,
            byte_length: stored.byte_length,
        })
    }
}

const UNKNOWN_NOTICE: &str =
    "Remote state UNKNOWN: do NOT duplicate-submit; reconcile with provider receipt";
const CANCEL_NOTICE: &str = "Local cancel recorded; remote stop or refund is NOT claimed.";
const FIXTURE_CONTRACT: &str = "TEST_FIXTURE_GLB_STRUCTURE_V1";

// JSON numbers above JavaScript’s safe integer range would round in the webview.
fn exact_count<S: serde::Serializer>(
    n: &u64,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if *n <= 9_007_199_254_740_991 {
        serializer.serialize_u64(*n)
    } else {
        serializer.collect_str(n)
    }
}

fn optional_count<S: serde::Serializer>(
    n: &Option<u64>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    match n {
        Some(n) => exact_count(n, serializer),
        None => serializer.serialize_none(),
    }
}

#[derive(Serialize)]
struct BudgetUnitDto {
    label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decimals: Option<u8>,
}

impl From<budget::Unit> for BudgetUnitDto {
    fn from(unit: budget::Unit) -> Self {
        let mut dto = Self {
            label: "",
            currency: None,
            decimals: None,
        };
        dto.label = match unit {
            budget::Unit::Token => "Token",
            budget::Unit::Task => "Task",
            budget::Unit::Second => "Second",
            budget::Unit::CurrencyMinor { currency, decimals } => {
                dto.currency = Some(currency);
                dto.decimals = Some(decimals);
                "CurrencyMinor"
            }
        };
        dto
    }
}

#[derive(Serialize)]
pub struct BudgetAccountDto {
    id: String,
    unit: BudgetUnitDto,
    approval: &'static str,
    source: String,
    #[serde(serialize_with = "exact_count")]
    capacity: u64,
    #[serde(serialize_with = "exact_count")]
    reserved: u64,
    #[serde(serialize_with = "exact_count")]
    spent: u64,
    #[serde(serialize_with = "exact_count")]
    available: u64,
}

impl TryFrom<budget::Balance> for BudgetAccountDto {
    type Error = ServiceError;

    fn try_from(balance: budget::Balance) -> Result<Self> {
        let budget::Balance {
            allowance,
            reserved,
            spent,
        } = balance;
        let available = allowance
            .capacity
            .checked_sub(reserved)
            .and_then(|n| n.checked_sub(spent))
            .ok_or_else(|| ServiceError::new("CORRUPT", "Allowance totals exceed capacity"))?;
        Ok(Self {
            id: allowance.id,
            unit: allowance.unit.into(),
            approval: match allowance.approval {
                budget::Approval::ZeroAdditionalCost => "ZERO_ADDITIONAL_COST",
                budget::Approval::ExplicitBounded => "EXPLICIT_BOUNDED",
            },
            source: allowance.source,
            capacity: allowance.capacity,
            reserved,
            spent,
            available,
        })
    }
}

#[derive(Serialize)]
struct BudgetReceiptDto {
    id: String,
    source: String,
    resolution: ResolutionDto,
    usage: Vec<BudgetUsageDto>,
}

#[derive(Serialize)]
enum ResolutionDto {
    Unresolved,
    Confirmed {
        #[serde(serialize_with = "exact_count")]
        charge: u64,
    },
    AuthoritativeNoCharge,
}

#[derive(Serialize)]
struct BudgetUsageDto {
    unit: BudgetUnitDto,
    #[serde(serialize_with = "optional_count")]
    amount: Option<u64>,
    source: String,
}

impl From<budget::Receipt> for BudgetReceiptDto {
    fn from(receipt: budget::Receipt) -> Self {
        Self {
            id: receipt.id,
            source: receipt.source,
            resolution: match receipt.resolution {
                budget::Resolution::Unresolved => ResolutionDto::Unresolved,
                budget::Resolution::Confirmed { charge } => ResolutionDto::Confirmed { charge },
                budget::Resolution::AuthoritativeNoCharge => ResolutionDto::AuthoritativeNoCharge,
            },
            usage: receipt
                .usage
                .into_iter()
                .map(|usage| BudgetUsageDto {
                    unit: usage.unit.into(),
                    amount: usage.amount,
                    source: usage.source,
                })
                .collect(),
        }
    }
}

pub type ApproveBudgetRequest = budget::Allowance;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReserveCallRequest {
    id: String,
    account: String,
    input_fingerprint: String,
    unit: budget::Unit,
    quote: Option<u64>,
    reservation: u64,
    source: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileCallRequest {
    intent_id: String,
    receipt_id: String,
    source: String,
    resolution: budget::Resolution,
    #[serde(default)]
    usage: Vec<budget::Usage>,
}

#[derive(Serialize)]
pub struct BudgetOverviewDto {
    accounts: Vec<BudgetAccountDto>,
    intents: Vec<BudgetIntentDto>,
    profiles: serde_json::Value,
}

#[derive(Serialize)]
pub struct BudgetIntentDto {
    id: String,
    account: String,
    unit: BudgetUnitDto,
    #[serde(serialize_with = "optional_count")]
    quote: Option<u64>,
    #[serde(serialize_with = "exact_count")]
    reservation: u64,
    state: &'static str,
    cancelled: bool,
    #[serde(serialize_with = "optional_count")]
    charge: Option<u64>,
    receipts: Vec<BudgetReceiptDto>,
    warning: Option<String>,
}

impl BudgetIntentDto {
    fn from_core(intent: budget::Intent, app: &Application) -> Result<Self> {
        use budget::State;
        let state = match intent.state {
            State::Preparing => "PREPARING",
            State::Attempting => "ATTEMPTING",
            State::Unknown => "UNKNOWN",
            State::Confirmed => "CONFIRMED",
            State::Released => "RELEASED",
        };
        let warning = match (intent.state == State::Unknown, intent.cancelled) {
            (true, true) => Some(format!("{UNKNOWN_NOTICE}. {CANCEL_NOTICE}")),
            (true, false) => Some(UNKNOWN_NOTICE.into()),
            (false, true) => Some(CANCEL_NOTICE.into()),
            (false, false) => None,
        };
        let request = intent.request;
        Ok(Self {
            receipts: app
                .call_receipts(&request.id)?
                .into_iter()
                .map(Into::into)
                .collect(),
            id: request.id,
            account: request.account,
            unit: request.unit.into(),
            quote: request.quote,
            reservation: request.reservation,
            state,
            cancelled: intent.cancelled,
            charge: intent.charge,
            warning,
        })
    }

    fn read(app: &Application, id: &str) -> Result<Self> {
        let intent = app
            .call_intent(id)?
            .ok_or_else(|| ServiceError::input("Unknown call"))?;
        Self::from_core(intent, app)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunByoGpuRequest {
    budget_id: String,
    intent_id: String,
    profile: String,
    model: String,
    out_dir: PathBuf,
    image: Option<PathBuf>,
    prompt: Option<String>,
    bundle: Option<PathBuf>,
    adapter_bin: Option<PathBuf>,
    #[serde(default)]
    allow_remote_upload: bool,
    max_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByoGpuRunDto {
    status: &'static str,
    profile: &'static str,
    model: String,
    intent_id: String,
    invoked: bool,
    artifact_file: Option<&'static str>,
    artifact_sha256: Option<String>,
    cash_cost: Option<u64>,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitHeadRequest {
    version_id: String,
    max_component_bytes: u64,
    required_quality: Vec<String>,
}

#[derive(Deserialize)]
#[serde(remote = "ComponentRole", rename_all = "lowercase")]
enum EditableRoleDef {
    Mesh,
    Materials,
    Rig,
    Motion,
    Vfx,
    Audio,
    Dialogue,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct EditableRole(#[serde(with = "EditableRoleDef")] ComponentRole);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeCandidateRequest {
    id: String,
    base_version: String,
    candidate_version: String,
    expected_revision: i64,
    editable: Vec<EditableRole>,
    lock_mode: String,
    max_component_bytes: u64,
    required_quality: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateHeadDto {
    asset_id: String,
    version_id: String,
    revision: i64,
    contract_id: String,
}

impl From<Current> for CandidateHeadDto {
    fn from(current: Current) -> Self {
        Self {
            asset_id: current.asset_id,
            version_id: current.version_id,
            revision: current.revision,
            contract_id: current.contract.contract_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateAttemptDto {
    candidate_id: String,
    decision: &'static str,
    reason: Option<String>,
    current_revision: Option<i64>,
    report: Option<AcceptanceReport>,
}

impl From<Acceptance> for CandidateAttemptDto {
    fn from(result: Acceptance) -> Self {
        let (candidate, decision, reason, current, report) = match result {
            Acceptance::Accepted {
                candidate,
                current,
                report,
            } => (candidate, "ACCEPTED", None, Some(current), Some(report)),
            Acceptance::Conflict {
                candidate,
                current,
                report,
            } => (
                candidate,
                "CONFLICT",
                Some("Current head differs from the proposed base version or revision".into()),
                Some(current),
                Some(report),
            ),
            Acceptance::Rejected { candidate, report } => (
                candidate,
                "REJECTED",
                Some("Fixture structural checks rejected the candidate".into()),
                None,
                Some(report),
            ),
            Acceptance::NotRun {
                candidate,
                reason,
                report,
            } => (candidate, "NOT_RUN", Some(reason), None, report),
        };
        Self {
            candidate_id: candidate.id,
            decision,
            reason,
            current_revision: current.map(|head| head.revision),
            report,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateItemDto {
    id: String,
    asset_id: String,
    base_version: String,
    candidate_version: String,
    expected_revision: i64,
    editable: Vec<String>,
    lock_mode: &'static str,
    attempt: Option<CandidateAttemptDto>,
}

impl CandidateItemDto {
    fn from_core(candidate: Candidate, app: &mut Application) -> Result<Self> {
        let asset = app
            .asset_version(&candidate.base_version)?
            .ok_or_else(|| ServiceError::new("CORRUPT", "Candidate base version is missing"))?;
        Ok(Self {
            attempt: app.candidate_attempt(&candidate.id)?.map(Into::into),
            id: candidate.id,
            asset_id: asset.asset_id,
            base_version: candidate.base_version,
            candidate_version: candidate.candidate_version,
            expected_revision: candidate.expected_revision,
            editable: candidate.editable,
            lock_mode: LockMode::Byte.as_str(),
        })
    }
}

#[derive(Serialize)]
pub struct CandidateOverviewDto {
    heads: Vec<CandidateHeadDto>,
    candidates: Vec<CandidateItemDto>,
}

impl WorkbenchService {
    pub fn budget_overview(&mut self, limit: u16) -> Result<BudgetOverviewDto> {
        let app = &self.opened()?.app;
        Ok(BudgetOverviewDto {
            accounts: app
                .list_budgets(limit)?
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_>>()?,
            intents: app
                .list_budget_intents(None, limit)?
                .into_iter()
                .map(|intent| BudgetIntentDto::from_core(intent, app))
                .collect::<Result<_>>()?,
            profiles: serde_json::to_value(byo_gpu::profiles())
                .map_err(|err| ServiceError::new("INTERNAL", err))?,
        })
    }

    pub fn approve_budget(&mut self, req: ApproveBudgetRequest) -> Result<BudgetAccountDto> {
        let app = &mut self.opened()?.app;
        app.approve_budget(&req)?;
        app.budget_balance(&req.id)?.try_into()
    }

    pub fn reserve_call(&mut self, req: ReserveCallRequest) -> Result<BudgetIntentDto> {
        let app = &mut self.opened()?.app;
        let intent = app.reserve_call(&budget::Request {
            id: req.id,
            account: req.account,
            unit: req.unit,
            input_fingerprint: req.input_fingerprint,
            source: req.source,
            quote: req.quote,
            reservation: req.reservation,
        })?;
        BudgetIntentDto::from_core(intent, app)
    }

    pub fn cancel_call(&mut self, intent_id: &str) -> Result<BudgetIntentDto> {
        let app = &mut self.opened()?.app;
        app.cancel_call(intent_id)?;
        BudgetIntentDto::read(app, intent_id)
    }

    pub fn reconcile_call(&mut self, req: ReconcileCallRequest) -> Result<BudgetIntentDto> {
        let app = &mut self.opened()?.app;
        app.reconcile_call(
            &req.intent_id,
            &budget::Receipt {
                id: req.receipt_id,
                source: req.source,
                resolution: req.resolution,
                usage: req.usage,
            },
        )?;
        BudgetIntentDto::read(app, &req.intent_id)
    }

    pub fn run_byo_gpu(&mut self, req: RunByoGpuRequest) -> Result<ByoGpuRunDto> {
        let profile = byo_gpu::Profile::from_id(&req.profile)
            .ok_or_else(|| ServiceError::input("Unknown BYO GPU profile"))?;
        let run = byo_gpu::Run {
            budget_id: req.budget_id,
            intent_id: req.intent_id,
            profile,
            model: req.model,
            out_dir: req.out_dir,
            image: req.image,
            prompt: req.prompt,
            bundle: req.bundle,
            adapter_bin: req.adapter_bin,
            allow_remote_upload: req.allow_remote_upload,
            max_bytes: req.max_bytes,
        };
        let report = self
            .opened()?
            .app
            .run_byo_gpu(&run)
            .map_err(|error| match error {
                byo_gpu::RunError::Invalid(byo_gpu::Invalid(field, message)) => {
                    ServiceError::input(format!("{field} — {message}"))
                }
                byo_gpu::RunError::Credentials => {
                    ServiceError::new("INPUT_INCOMPLETE", "Kaggle runner credentials are required")
                }
                byo_gpu::RunError::Ledger(error) => error.into(),
            })?;
        let (status, artifact_file, artifact_sha256, message) = match report.outcome {
            byo_gpu::Outcome::Succeeded { sha256 } => {
                let message = if report.attempted {
                    "Generated artifact recorded. Format validation and candidate acceptance are separate actions."
                } else {
                    "Settled receipt replayed with invoked=false. Artifact hash is historical."
                };
                (
                    "SUCCEEDED",
                    Some(report.artifact),
                    Some(sha256),
                    message.into(),
                )
            }
            byo_gpu::Outcome::Unknown { diagnostic } => (
                "UNKNOWN",
                None,
                None,
                format!("{diagnostic}. {UNKNOWN_NOTICE}"),
            ),
            byo_gpu::Outcome::Released => (
                "RELEASED",
                None,
                None,
                "Local Task reservation was released.".into(),
            ),
        };
        Ok(ByoGpuRunDto {
            status,
            profile: profile.id(),
            model: run.model,
            intent_id: run.intent_id,
            invoked: report.attempted,
            artifact_file,
            artifact_sha256,
            cash_cost: None,
            message,
        })
    }

    pub fn candidate_overview(&mut self, limit: u16) -> Result<CandidateOverviewDto> {
        let app = &mut self.opened()?.app;
        let mut seen = std::collections::BTreeSet::new();
        let mut heads = Vec::new();
        let mut candidates = Vec::new();
        for candidate in app.list_candidates(limit)? {
            let item = CandidateItemDto::from_core(candidate, app)?;
            if seen.insert(item.asset_id.clone())
                && let Some(head) = app.candidate_current(&item.asset_id)?
            {
                heads.push(head.into());
            }
            candidates.push(item);
        }
        Ok(CandidateOverviewDto { heads, candidates })
    }

    pub fn initialize_candidate_head(&mut self, req: InitHeadRequest) -> Result<CandidateHeadDto> {
        Ok(self
            .opened()?
            .app
            .initialize_candidate_current(
                &req.version_id,
                &FixtureContract {
                    contract_id: FIXTURE_CONTRACT.into(),
                    max_component_bytes: req.max_component_bytes,
                    required_quality: req.required_quality,
                },
            )?
            .into())
    }

    pub fn propose_candidate(&mut self, req: ProposeCandidateRequest) -> Result<CandidateItemDto> {
        let app = &mut self.opened()?.app;
        let candidate = app.propose_candidate(&Proposal {
            id: req.id,
            base_version: req.base_version,
            candidate_version: req.candidate_version,
            expected_revision: req.expected_revision,
            editable: req.editable.into_iter().map(|role| role.0).collect(),
            lock_mode: LockMode::parse(&req.lock_mode).map_err(ServiceError::input)?,
            contract: Some(FixtureContract {
                contract_id: FIXTURE_CONTRACT.into(),
                max_component_bytes: req.max_component_bytes,
                required_quality: req.required_quality,
            }),
        })?;
        CandidateItemDto::from_core(candidate, app)
    }

    pub fn attempt_accept_candidate(&mut self, candidate_id: &str) -> Result<CandidateAttemptDto> {
        Ok(self
            .opened()?
            .app
            .attempt_accept_candidate(candidate_id)?
            .into())
    }
}
