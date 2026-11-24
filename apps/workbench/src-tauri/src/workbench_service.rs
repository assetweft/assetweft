use assetweft_core::{
    application::Application,
    assurance::{
        RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits, ValidationReport,
    },
    candidate::Current,
    history::{Decision, Feedback, FeedbackRecord, HistoryQuery},
    library::{AssetVersion, ComponentReference, ComponentRole, LibraryError, Presence},
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
            "NOT_RUN",
            "Planned in AW-032:B. Use CLI assetweft byo-gpu / plan / candidate.",
        ),
        (
            "rig_morph_animation_editor",
            "NOT_RUN",
            "Planned in AW-032:C.",
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
