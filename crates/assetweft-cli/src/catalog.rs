//! Responsibility: Catalog incoming adapter for exact stored metadata, references and fixture head.
//! Dependencies: shared CLI transport, public Application and Catalog compatibility values.
//! Callers: main catalog command dispatch; projections remain local to this consumer.
//! Invariants: one owning facade query per action; IDs/order/metadata preserved; no SQL, policy, CAS, retry or implicit init.

use crate::protocol::{envelope, error, existing_database, library_outcome};
use assetweft_core::{
    application::Application,
    library::{AssetVersion, ComponentReference, ComponentRole, Presence},
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

pub(super) fn query_response(
    wire: &str,
    call_id: Option<&String>,
    action: &str,
    args: &ArgMatches,
) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let id = args
        .get_one::<String>(if action == "current" {
            "asset-id"
        } else {
            "version-id"
        })
        .expect("required");
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
    let result = match action {
        "version" => app.asset_version(id).map(|version| {
            json!({"scope":"stored_local_asset_version_metadata","version":version.map(version_value)})
        }),
        "references" => app.component_references(id).map(|references| {
            json!({"scope":"stored_local_exact_component_references","references":references.into_iter().map(reference_value).collect::<Vec<_>>()})
        }),
        "current" => app.candidate_current(id).map(|current| {
            json!({"scope":"stored_local_candidate_current","current":current})
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
