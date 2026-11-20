//! Responsibility: Catalog incoming adapter for immutable metadata, exact references, version feedback and stored queries.
//! Dependencies: shared CLI transport, public Application and Catalog compatibility values.
//! Callers: main catalog command dispatch; projections remain local to this consumer.
//! Invariants: one owning facade call per action; strict wire shapes; submitted write acknowledgements and stored queries preserve values; no SQL, policy, CAS, retry or implicit init.

use crate::protocol::{envelope, error, existing_database, library_outcome, read_json};
use assetweft_core::{
    application::Application,
    assurance as val,
    catalog::domain::{Decision, Feedback, FeedbackRecord},
    library::{AssetVersion, ComponentReference, ComponentRole, Presence},
    storage::{FileAvailability, StorageError},
};
use clap::ArgMatches;
use serde_json::{Value, json};
use std::path::PathBuf;

fn version_value(version: AssetVersion) -> Value {
    let AssetVersion {
        asset_id,
        version_id,
        content_hash,
        input_id,
        source,
        authorization,
        authorized,
        license_id,
        usage_scope,
        source_format,
        contract_version,
        presence,
    } = version;
    let presence = match presence {
        Presence::Unknown => "UNKNOWN",
        Presence::Missing => "MISSING",
    };
    json!({"asset_id":asset_id,"version_id":version_id,"content_hash":content_hash,
        "input_id":input_id,"source":source,"authorization":authorization,"authorized":authorized,
        "license_id":license_id,"usage_scope":usage_scope,"source_format":source_format,
        "contract_version":contract_version,"presence":presence})
}
fn reference_value(reference: ComponentReference) -> Value {
    let ComponentReference {
        owner_version_id,
        role,
        component_version_id,
    } = reference;
    let role = match role {
        ComponentRole::Mesh => "mesh",
        ComponentRole::Materials => "materials",
        ComponentRole::Rig => "rig",
        ComponentRole::Motion => "motion",
        ComponentRole::Vfx => "vfx",
        ComponentRole::Audio => "audio",
        ComponentRole::Dialogue => "dialogue",
    };
    json!({"owner_version_id":owner_version_id,"role":role,"component_version_id":component_version_id})
}

fn feedback_value(record: FeedbackRecord) -> Value {
    let FeedbackRecord { sequence, feedback } = record;
    let decision = match feedback.decision {
        Decision::Accept => "ACCEPT",
        Decision::Reject => "REJECT",
    };
    json!({"sequence":sequence,"feedback_id":feedback.feedback_id,"asset_id":feedback.asset_id,
        "version_id":feedback.version_id,"decision":decision,"reason":feedback.reason})
}

pub(super) fn query_response(
    wire: &str,
    call_id: Option<&String>,
    action: &str,
    args: &ArgMatches,
) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let id = |name| args.get_one::<String>(name).expect("required");
    let finish =
        |code, status, errors, results| (code, envelope(wire, call_id, status, errors, results));
    if let Err(e) = existing_database(db) {
        return finish(2, "INVALID_INPUT", json!([e]), Value::Null);
    }
    let opened = if action == "file-availability" {
        Application::open_read_only(db)
    } else {
        Application::open(db)
    };
    let mut app = match opened {
        Ok(app) => app,
        Err(e) => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error("db", "INVALID_INPUT", e)]),
                Value::Null,
            );
        }
    };
    if action == "file-availability" {
        let version_id = id("version-id");
        return match app.file_availability(version_id) {
            Ok(observed) => {
                let availability = match observed {
                    FileAvailability::UnknownVersion => "UNKNOWN_VERSION",
                    FileAvailability::MetadataOnly => "METADATA_ONLY",
                    FileAvailability::Missing => "MISSING",
                    FileAvailability::Available => "AVAILABLE",
                };
                finish(
                    0,
                    "PASS",
                    json!([]),
                    json!({"scope":"local_managed_version_bytes","version_id":version_id,"availability":availability}),
                )
            }
            Err(e) => {
                let (code, status) = storage_outcome(&e);
                finish(
                    code,
                    status,
                    json!([error("catalog", status, e)]),
                    Value::Null,
                )
            }
        };
    }
    let result = match action {
        "version" => app.asset_version(id("version-id")).map(|version| {
            json!({"scope":"stored_local_asset_version_metadata","version":version.map(version_value)})
        }),
        "references" => app.component_references(id("version-id")).map(|references| {
            json!({"scope":"stored_local_exact_component_references","references":references.into_iter().map(reference_value).collect::<Vec<_>>()})
        }),
        "current" => app.candidate_current(id("asset-id")).map(|current| {
            json!({"scope":"stored_local_candidate_current","current":current})
        }),
        "feedback-latest" => app
            .latest_feedback(
                id("asset-id"),
                args.get_one::<String>("version-id").map(String::as_str),
            )
            .map(|record| {
                json!({"scope":"stored_local_version_feedback","feedback":record.map(feedback_value)})
            }),
        _ => unreachable!("clap grammar"),
    };
    match result {
        Ok(value) => finish(0, "PASS", json!([]), value),
        Err(e) => {
            let (code, status) = library_outcome(&e);
            finish(
                code,
                status,
                json!([error("catalog", status, e)]),
                Value::Null,
            )
        }
    }
}

fn storage_outcome(failure: &StorageError) -> (i32, &'static str) {
    match failure {
        StorageError::Library(e) => library_outcome(e),
        StorageError::Invalid(_) => (2, "INVALID_INPUT"),
        StorageError::Corrupt => (1, "FAIL"),
        StorageError::Busy | StorageError::Io(_) => (6, "INTERNAL"),
    }
}

// Transport enums are deliberately local: no domain identity, license or contract policy.
struct PresenceInput(Presence);
impl<'de> serde::Deserialize<'de> for PresenceInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        match text.as_str() {
            "UNKNOWN" => Ok(Self(Presence::Unknown)),
            "MISSING" => Ok(Self(Presence::Missing)),
            _ => Err(serde::de::Error::unknown_variant(
                &text,
                &["UNKNOWN", "MISSING"],
            )),
        }
    }
}
struct RoleInput(ComponentRole);
impl<'de> serde::Deserialize<'de> for RoleInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        let role = match text.as_str() {
            "mesh" => ComponentRole::Mesh,
            "materials" => ComponentRole::Materials,
            "rig" => ComponentRole::Rig,
            "motion" => ComponentRole::Motion,
            "vfx" => ComponentRole::Vfx,
            "audio" => ComponentRole::Audio,
            "dialogue" => ComponentRole::Dialogue,
            _ => {
                return Err(serde::de::Error::unknown_variant(
                    &text,
                    &[
                        "mesh",
                        "materials",
                        "rig",
                        "motion",
                        "vfx",
                        "audio",
                        "dialogue",
                    ],
                ));
            }
        };
        Ok(Self(role))
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionInput {
    asset_id: String,
    version_id: String,
    content_hash: String,
    input_id: String,
    source: String,
    authorization: String,
    authorized: bool,
    license_id: String,
    usage_scope: String,
    source_format: String,
    contract_version: String,
    presence: PresenceInput,
}
struct VersionObject(AssetVersion);
impl<'de> serde::Deserialize<'de> for VersionObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct VersionVisitor;
        impl<'de> serde::de::Visitor<'de> for VersionVisitor {
            type Value = VersionObject;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an asset version metadata object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                let input = <VersionInput as serde::Deserialize>::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(VersionObject(AssetVersion {
                    asset_id: input.asset_id,
                    version_id: input.version_id,
                    content_hash: input.content_hash,
                    input_id: input.input_id,
                    source: input.source,
                    authorization: input.authorization,
                    authorized: input.authorized,
                    license_id: input.license_id,
                    usage_scope: input.usage_scope,
                    source_format: input.source_format,
                    contract_version: input.contract_version,
                    presence: input.presence.0,
                }))
            }
        }
        deserializer.deserialize_map(VersionVisitor)
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceInput {
    owner_version_id: String,
    role: RoleInput,
    component_version_id: String,
}
struct ReferenceObject(ComponentReference);
impl<'de> serde::Deserialize<'de> for ReferenceObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ReferenceVisitor;
        impl<'de> serde::de::Visitor<'de> for ReferenceVisitor {
            type Value = ReferenceObject;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an exact component reference object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                let input = <ReferenceInput as serde::Deserialize>::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(ReferenceObject(ComponentReference {
                    owner_version_id: input.owner_version_id,
                    role: input.role.0,
                    component_version_id: input.component_version_id,
                }))
            }
        }
        deserializer.deserialize_map(ReferenceVisitor)
    }
}
/// One owning mutation; success projects its acknowledgement without readback.
pub(super) fn write_response(
    wire: &str,
    call_id: Option<&String>,
    action: &str,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let finish =
        |code, status, errors, results| (code, envelope(wire, call_id, status, errors, results));
    if let Err(e) = existing_database(db) {
        return finish(2, "INVALID_INPUT", json!([e]), Value::Null);
    }
    let mut app = match Application::open(db) {
        Ok(app) => app,
        Err(e) => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error("db", "INVALID_INPUT", e)]),
                Value::Null,
            );
        }
    };
    let storage_error = |e: StorageError| {
        let (code, status) = storage_outcome(&e);
        finish(
            code,
            status,
            json!([error("catalog", status, e)]),
            Value::Null,
        )
    };
    if matches!(action, "materialize-export" | "evict-export") {
        let id = args.get_one::<String>("version-id").expect("required");
        let result = if action == "materialize-export" {
            app.materialize_export(id).map(
                |path| json!({"scope":"local_version_export","version_id":id,"relative_path":path}),
            )
        } else {
            app.evict_export(id).map(
                |removed| json!({"scope":"local_version_export","version_id":id,"removed":removed}),
            )
        };
        return match result {
            Ok(value) => finish(0, "PASS", json!([]), value),
            Err(e) => storage_error(e),
        };
    }
    if action == "commit-object" {
        let path = args.get_one::<PathBuf>("version").expect("required");
        let VersionObject(version) = match read_json(path, limits, "version") {
            Ok(input) => input,
            Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
        };
        let source = args.get_one::<PathBuf>("file").expect("required");
        // Admission is for private local files; path checks cannot prevent replacement races.
        let open = || -> std::io::Result<std::fs::File> {
            if !std::fs::metadata(source)?.is_file() {
                return Err(std::io::Error::other("--file must name a regular file"));
            }
            let file = std::fs::File::open(source)?;
            if !file.metadata()?.is_file() {
                return Err(std::io::Error::other("--file must name a regular file"));
            }
            Ok(file)
        };
        let mut file = match open() {
            Ok(file) => file,
            Err(e) => {
                return finish(
                    2,
                    "INVALID_INPUT",
                    json!([error("file", "INVALID_INPUT", e)]),
                    Value::Null,
                );
            }
        };
        let expected = *args.get_one::<u64>("expected-len").expect("required");
        return match app.commit_object(&version, &mut file, expected, limits.max_bytes) {
            Ok(stored) => finish(
                0,
                "PASS",
                json!([]),
                json!({"scope":"committed_local_managed_object","version_id":version.version_id,"content_hash":stored.content_hash,"byte_length":stored.byte_length}),
            ),
            Err(e) => storage_error(e),
        };
    }
    let result = match action {
        "register-version" => {
            let path = args.get_one::<PathBuf>("version").expect("required");
            let VersionObject(version) = match read_json(path, limits, "version") {
                Ok(input) => input,
                Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
            };
            app.add_asset_version(&version).map(|()| {
                json!({"scope":"registered_local_asset_version_metadata","version":version_value(version)})
            })
        }
        "add-reference" => {
            let path = args.get_one::<PathBuf>("reference").expect("required");
            let ReferenceObject(reference) = match read_json(path, limits, "reference") {
                Ok(input) => input,
                Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
            };
            app.add_component_reference(&reference).map(|()| {
                json!({"scope":"registered_local_exact_component_reference","reference":reference_value(reference)})
            })
        }
        "feedback-add" => {
            let required = |name| args.get_one::<String>(name).expect("required");
            let decision = match required("decision").as_str() {
                "ACCEPT" => Decision::Accept,
                "REJECT" => Decision::Reject,
                _ => unreachable!("clap grammar"),
            };
            app.append_feedback(&Feedback {
                feedback_id: required("feedback-id").clone(),
                asset_id: required("asset-id").clone(),
                version_id: required("version-id").clone(),
                decision,
                reason: args.get_one::<String>("reason").cloned(),
            })
            .map(|record| {
                json!({"scope":"appended_local_version_feedback","feedback":feedback_value(record)})
            })
        }
        _ => unreachable!("clap grammar"),
    };
    match result {
        Ok(value) => finish(0, "PASS", json!([]), value),
        Err(e) => {
            let (code, status) = library_outcome(&e);
            finish(
                code,
                status,
                json!([error("catalog", status, e)]),
                Value::Null,
            )
        }
    }
}
