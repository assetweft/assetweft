use assetweft_core::{
    LockMode,
    application::Application,
    candidate::{Acceptance, Candidate, Proposal},
    library::{AssetVersion, ComponentReference, ComponentRole, Presence},
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
fn put(a: &mut Application, id: &str, asset: &str, bytes: &[u8]) {
    let v = AssetVersion {
        asset_id: asset.into(),
        version_id: id.into(),
        content_hash: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        input_id: "fixture".into(),
        source: "TEST_FIXTURE".into(),
        authorization: "UNVERIFIED".into(),
        authorized: false,
        license_id: "UNKNOWN".into(),
        usage_scope: "TEST_ONLY".into(),
        source_format: "OPAQUE".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    };
    a.commit_object(&v, &mut &bytes[..], bytes.len() as u64, 1024)
        .unwrap();
}
fn reference(a: &mut Application, owner: &str, role: ComponentRole, version: &str) {
    a.add_component_reference(&ComponentReference {
        owner_version_id: owner.into(),
        role,
        component_version_id: version.into(),
    })
    .unwrap();
}
fn setup() -> (tempfile::TempDir, Application, Proposal) {
    let d = tempfile::tempdir().unwrap();
    let mut a = Application::open(&d.path().join("library.sqlite")).unwrap();
    a.initialize_library().unwrap();
    for (id, bytes) in [
        ("mesh", b"mesh".as_slice()),
        ("materials", b"materials"),
        ("rig", b"rig"),
        ("motion0", b"old motion"),
        ("motion1", b"new motion"),
    ] {
        put(&mut a, id, id, bytes);
    }
    put(&mut a, "base", "character", b"base manifest");
    put(&mut a, "proposed", "character", b"candidate manifest");
    for owner in ["base", "proposed"] {
        for (role, id) in [
            (ComponentRole::Mesh, "mesh"),
            (ComponentRole::Materials, "materials"),
            (ComponentRole::Rig, "rig"),
        ] {
            reference(&mut a, owner, role, id);
        }
        reference(
            &mut a,
            owner,
            ComponentRole::Motion,
            if owner == "base" {
                "motion0"
            } else {
                "motion1"
            },
        );
    }
    let p = Proposal {
        id: "candidate-1".into(),
        base_version: "base".into(),
        candidate_version: "proposed".into(),
        expected_revision: 17,
        editable: vec![ComponentRole::Motion],
        lock_mode: LockMode::Byte,
        contract: None,
    };
    (d, a, p)
}
#[test]
fn motion_only_snapshot_survives_reopen_without_acceptance() {
    let (d, mut a, p) = setup();
    let c = a.propose_candidate(&p).unwrap();
    for role in ["mesh", "materials", "rig"] {
        assert_eq!(
            c.base.iter().find(|s| s.role == role),
            c.proposed.iter().find(|s| s.role == role)
        );
    }
    assert_ne!(
        c.base.iter().find(|s| s.role == "motion"),
        c.proposed.iter().find(|s| s.role == "motion")
    );
    assert_eq!(c.expected_revision, 17);
    assert_eq!(c.source, "TEST_FIXTURE");
    assert!(c.not_run.iter().any(|s| s == "QUALITY"));
    assert_eq!(a.propose_candidate(&p).unwrap(), c);
    assert!(
        matches!(a.attempt_accept_candidate(&p.id).unwrap(),Acceptance::NotRun{candidate,..} if candidate==c)
    );
    let hash = c.base[0].measured_sha256.clone();
    assert_eq!(
        a.collect_object(&hash).unwrap(),
        assetweft_core::storage::Collection::Protected
    );
    a.close().unwrap();
    let mut b = Application::open(&d.path().join("library.sqlite")).unwrap();
    assert_eq!(b.candidate(&p.id).unwrap(), Some(c));
    for id in ["base", "proposed", "motion0", "motion1"] {
        assert!(b.managed_object(id).unwrap().is_some());
    }
}
#[test]
fn corrupted_locked_bytes_fail_despite_unchanged_claimed_hash() {
    let (d, mut a, p) = setup();
    let hash = a.asset_version("mesh").unwrap().unwrap().content_hash;
    fs::write(d.path().join("objects").join(hash), b"fake").unwrap();
    assert!(a.propose_candidate(&p).is_err());
    assert!(a.candidate(&p.id).unwrap().is_none());
}
#[test]
fn changed_locked_reference_is_rejected_even_when_bytes_identical() {
    let (d, mut a, p) = setup();
    put(&mut a, "mesh-copy", "separate-license", b"mesh");
    let raw = Connection::open(d.path().join("library.sqlite")).unwrap();
    raw.execute("UPDATE aw_component_references SET component_version_id='mesh-copy' WHERE owner_version_id='proposed' AND role='mesh'",[]).unwrap();
    assert!(a.propose_candidate(&p).is_err());
    assert!(a.candidate(&p.id).unwrap().is_none());
}
#[test]
fn missing_duplicate_roles_and_unmanaged_refs_reject() {
    for mode in [0, 1, 2] {
        let (d, mut a, p) = setup();
        let raw = Connection::open(d.path().join("library.sqlite")).unwrap();
        match mode {
            0 => {
                raw.execute("DELETE FROM aw_component_references WHERE owner_version_id='proposed' AND role='rig'",[]).unwrap();
            }
            1 => {
                reference(&mut a, "proposed", ComponentRole::Mesh, "motion1");
            }
            _ => {
                raw.execute(
                    "DELETE FROM aw_managed_versions WHERE version_id='motion1'",
                    [],
                )
                .unwrap();
            }
        }
        assert!(a.propose_candidate(&p).is_err());
        assert!(a.candidate(&p.id).unwrap().is_none());
    }
}
#[test]
fn semantic_overflow_reuse_and_frozen_refs_fail_closed() {
    let (_d, mut a, mut p) = setup();
    p.lock_mode = LockMode::Semantic;
    assert!(
        a.propose_candidate(&p)
            .unwrap_err()
            .to_string()
            .contains("SEMANTIC NOT_RUN")
    );
    p.lock_mode = LockMode::Byte;
    p.expected_revision = i64::MAX;
    assert!(a.propose_candidate(&p).is_err());
    p.expected_revision = 3;
    let c = a.propose_candidate(&p).unwrap();
    p.expected_revision = 4;
    assert!(a.propose_candidate(&p).is_err());
    assert_eq!(a.candidate(&p.id).unwrap(), Some(c));
    assert!(
        a.add_component_reference(&ComponentReference {
            owner_version_id: "base".into(),
            role: ComponentRole::Audio,
            component_version_id: "motion1".into()
        })
        .is_err()
    );
}
#[test]
fn failed_insert_rolls_back_without_descriptor_or_freezing_refs() {
    let (d, mut a, p) = setup();
    let raw = Connection::open(d.path().join("library.sqlite")).unwrap();
    raw.execute_batch("CREATE TRIGGER inject_candidate_failure BEFORE INSERT ON aw_candidates BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(a.propose_candidate(&p).is_err());
    assert!(a.candidate(&p.id).unwrap().is_none());
    reference(&mut a, "base", ComponentRole::Audio, "motion1");
}
#[test]
fn migration_005_rollback_preserves_v4_and_future_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(include_str!("../src/library/001_initial.sql"))
        .unwrap();
    raw.execute_batch(include_str!("../src/library/002_objects.sql"))
        .unwrap();
    raw.execute_batch(include_str!("../src/library/003_workflow.sql"))
        .unwrap();
    raw.execute_batch(include_str!("../src/library/004_budget.sql"))
        .unwrap();
    raw.execute_batch("CREATE TABLE aw_schema_version(singleton INTEGER PRIMARY KEY,version INTEGER); INSERT INTO aw_schema_version VALUES(1,4); CREATE TABLE aw_candidate_current(sentinel TEXT); INSERT INTO aw_candidate_current VALUES('keep'); INSERT INTO aw_assets VALUES('legacy');").unwrap();
    let mut a = Application::open(&path).unwrap();
    assert!(a.initialize_library().is_err());
    assert_eq!(
        raw.query_row("SELECT version FROM aw_schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        raw.query_row("SELECT sentinel FROM aw_candidate_current", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert_eq!(
        raw.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='aw_candidates'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        raw.query_row("SELECT asset_id FROM aw_assets", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "legacy"
    );
    raw.execute("UPDATE aw_schema_version SET version=7", [])
        .unwrap();
    assert!(Application::open(&path).is_err());
}

fn put_glb(a: &mut Application, id: &str, bytes: &[u8]) {
    let v = AssetVersion {
        asset_id: id.into(),
        version_id: id.into(),
        content_hash: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        input_id: "fixture".into(),
        source: "TEST_FIXTURE".into(),
        authorization: "UNKNOWN".into(),
        authorized: false,
        license_id: "UNKNOWN".into(),
        usage_scope: "TEST_ONLY".into(),
        source_format: "GLB".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    };
    a.commit_object(&v, &mut &bytes[..], bytes.len() as u64, 1024 * 1024)
        .unwrap();
}
fn glb_setup() -> (tempfile::TempDir, Application, Proposal) {
    let d = tempfile::tempdir().unwrap();
    let mut a = Application::open(&d.path().join("library.sqlite")).unwrap();
    a.initialize_library().unwrap();
    let bytes = include_bytes!("fixtures/validation/mesh_only.glb");
    put_glb(&mut a, "mesh0", bytes);
    put_glb(&mut a, "mesh1", bytes);
    put_glb(&mut a, "mesh2", bytes);
    for (owner, id) in [("base", "mesh0"), ("proposed", "mesh1"), ("other", "mesh2")] {
        put(&mut a, owner, "character", owner.as_bytes());
        reference(&mut a, owner, ComponentRole::Mesh, id);
    }
    let contract = assetweft_core::candidate::FixtureContract {
        contract_id: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
        max_component_bytes: 1024 * 1024,
        required_quality: vec![],
    };
    let current = a.initialize_candidate_current("base", &contract).unwrap();
    assert_eq!(current.revision, 0);
    let p = Proposal {
        id: "glb-1".into(),
        base_version: "base".into(),
        candidate_version: "proposed".into(),
        expected_revision: 0,
        editable: vec![ComponentRole::Mesh],
        lock_mode: LockMode::Byte,
        contract: Some(contract),
    };
    (d, a, p)
}
#[test]
fn glb_conditional_accept_stale_conflict_reopen_and_exact_history() {
    let (d, mut a, p) = glb_setup();
    a.propose_candidate(&p).unwrap();
    let mut stale = p.clone();
    stale.id = "glb-2".into();
    stale.candidate_version = "other".into();
    a.propose_candidate(&stale).unwrap();
    let result = a.attempt_accept_candidate(&p.id).unwrap();
    if let Acceptance::Accepted { current, .. } = &result {
        let head: &assetweft_core::catalog::domain::Current = current;
        let _: &assetweft_core::candidate::Current = head;
        let contract: &assetweft_core::catalog::domain::FixtureContract = &head.contract;
        let _: &assetweft_core::candidate::FixtureContract = contract;
    }
    assert!(
        matches!(&result,Acceptance::Accepted{current,report,..} if current.version_id=="proposed" && current.revision==1 && report.components[0].format_status=="PASS" && report.components[0].role_status=="PASS" && report.not_run.contains(&"QUALITY".into()))
    );
    let conflict = a.attempt_accept_candidate(&stale.id).unwrap();
    assert!(
        matches!(&conflict,Acceptance::Conflict{current,candidate,..} if current.revision==1 && candidate.candidate_version=="other")
    );
    assert_eq!(a.attempt_accept_candidate(&p.id).unwrap(), result);
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        1
    );
    a.close().unwrap();
    let mut b = Application::open(&d.path().join("library.sqlite")).unwrap();
    assert_eq!(b.candidate_attempt(&stale.id).unwrap(), Some(conflict));
    assert!(b.candidate(&stale.id).unwrap().is_some());
    for id in ["mesh0", "mesh1", "mesh2", "base", "proposed", "other"] {
        assert!(b.managed_object(id).unwrap().is_some());
    }
    assert!(
        b.initialize_candidate_current("base", p.contract.as_ref().unwrap())
            .is_err()
    );
}
#[test]
fn independent_apps_same_base_have_one_winner_and_durable_conflict() {
    use std::sync::{Arc, Barrier};
    let (d, mut a, p) = glb_setup();
    a.propose_candidate(&p).unwrap();
    let mut p2 = p.clone();
    p2.id = "glb-2".into();
    p2.candidate_version = "other".into();
    a.propose_candidate(&p2).unwrap();
    let b = Application::open(&d.path().join("library.sqlite")).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [(a, p.id), (b, p2.id)]
        .into_iter()
        .map(|(mut app, id)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                app.attempt_accept_candidate(&id).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Acceptance::Accepted { .. }))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Acceptance::Conflict { .. }))
            .count(),
        1
    );
    let app = Application::open(&d.path().join("library.sqlite")).unwrap();
    assert_eq!(
        app.candidate_current("character")
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    for id in ["glb-1", "glb-2"] {
        assert!(app.candidate(id).unwrap().is_some());
        assert!(app.candidate_attempt(id).unwrap().is_some());
    }
}
#[test]
fn required_quality_changed_contract_and_bad_glb_never_advance() {
    for mode in [0, 1, 2, 3] {
        let (d, mut a, mut p) = glb_setup();
        match mode {
            0 => p
                .contract
                .as_mut()
                .unwrap()
                .required_quality
                .push("visual".into()),
            1 => p.contract.as_mut().unwrap().max_component_bytes = 512 * 1024,
            2 => {
                put_glb(&mut a, "bad", b"invalid glb");
                let raw = Connection::open(d.path().join("library.sqlite")).unwrap();
                raw.execute("UPDATE aw_component_references SET component_version_id='bad' WHERE owner_version_id='proposed'",[]).unwrap();
            }
            _ => {
                let raw = Connection::open(d.path().join("library.sqlite")).unwrap();
                raw.execute("UPDATE aw_asset_versions SET source_format='UNSUPPORTED' WHERE version_id='mesh1'",[]).unwrap();
            }
        }
        a.propose_candidate(&p).unwrap();
        let result = a.attempt_accept_candidate(&p.id).unwrap();
        assert!(!matches!(result, Acceptance::Accepted { .. }));
        assert_eq!(
            a.candidate_current("character").unwrap().unwrap().revision,
            0
        );
    }
}
#[test]
fn receipt_failure_rolls_back_conditional_current_update() {
    let (d, mut a, p) = glb_setup();
    a.propose_candidate(&p).unwrap();
    let raw = Connection::open(d.path().join("library.sqlite")).unwrap();
    raw.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON aw_candidate_attempts BEGIN SELECT RAISE(ABORT,'receipt injection'); END;").unwrap();
    assert!(a.attempt_accept_candidate(&p.id).is_err());
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        0
    );
    assert!(a.candidate_attempt(&p.id).unwrap().is_none());
    assert!(a.candidate(&p.id).unwrap().is_some());
}
#[test]
fn corrupted_managed_bytes_after_staging_fail_acceptance() {
    let (d, mut a, p) = glb_setup();
    let c = a.propose_candidate(&p).unwrap();
    fs::write(
        d.path()
            .join("objects")
            .join(&c.proposed[0].measured_sha256),
        b"forged",
    )
    .unwrap();
    assert!(a.attempt_accept_candidate(&p.id).is_err());
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        0
    );
}

fn glb_from_json(json: &str) -> Vec<u8> {
    let mut js = json.as_bytes().to_vec();
    while !js.len().is_multiple_of(4) {
        js.push(b' ');
    }
    let bin = [0u8; 36];
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

fn glb_two_skins_animation(target_node: u32) -> Vec<u8> {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"skin":0}},{{"name":"j1"}},{{"name":"j2"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"skins":[{{"joints":[1]}},{{"joints":[2]}}],"animations":[{{"channels":[{{"sampler":0,"target":{{"node":{target_node},"path":"translation"}}}}],"samplers":[{{"input":1,"output":2}}]}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,1]}},{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]}},{{"bufferView":0,"byteOffset":8,"componentType":5126,"count":2,"type":"VEC3"}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"buffers":[{{"byteLength":36}}]}}"#
    );
    glb_from_json(&json)
}

#[test]
fn motion_glb_structure_checks_preserve_mesh_and_rig_and_reject_orphan_animation() {
    for target in [1, 2] {
        let d = tempfile::tempdir().unwrap();
        let mut a = Application::open(&d.path().join("library.sqlite")).unwrap();
        a.initialize_library().unwrap();
        put_glb(
            &mut a,
            "mesh",
            include_bytes!("fixtures/validation/mesh_only.glb"),
        );
        put_glb(&mut a, "rig", &glb_two_skins_animation(1));
        put_glb(&mut a, "motion0", &glb_two_skins_animation(1));
        put_glb(&mut a, "motion1", &glb_two_skins_animation(target));
        for (owner, motion) in [("base", "motion0"), ("next", "motion1")] {
            put(&mut a, owner, "character", owner.as_bytes());
            reference(&mut a, owner, ComponentRole::Mesh, "mesh");
            reference(&mut a, owner, ComponentRole::Rig, "rig");
            reference(&mut a, owner, ComponentRole::Motion, motion);
        }
        let contract = assetweft_core::candidate::FixtureContract {
            contract_id: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
            max_component_bytes: 1024 * 1024,
            required_quality: vec![],
        };
        a.initialize_candidate_current("base", &contract).unwrap();
        let p = Proposal {
            id: "motion-candidate".into(),
            base_version: "base".into(),
            candidate_version: "next".into(),
            expected_revision: 0,
            editable: vec![ComponentRole::Motion],
            lock_mode: LockMode::Byte,
            contract: Some(contract),
        };
        let staged = a.propose_candidate(&p).unwrap();
        for role in ["mesh", "rig"] {
            assert_eq!(
                staged.base.iter().find(|s| s.role == role),
                staged.proposed.iter().find(|s| s.role == role)
            );
        }
        let result = a.attempt_accept_candidate(&p.id).unwrap();
        if target == 1 {
            assert!(
                matches!(result,Acceptance::Accepted{report,..} if report.components.iter().find(|c|c.role=="motion").unwrap().requirement=="ANIMATED_CHARACTER")
            );
        } else {
            assert!(
                matches!(result,Acceptance::Rejected{report,..} if report.components.iter().find(|c|c.role=="motion").unwrap().format_status=="PASS" && report.components.iter().find(|c|c.role=="motion").unwrap().role_status=="FAIL")
            );
            assert_eq!(
                a.candidate_current("character").unwrap().unwrap().revision,
                0
            );
        }
    }
}
#[test]
fn same_base_wrong_revision_conflicts_and_staged_contract_cannot_be_weakened() {
    let (_d, mut a, mut p) = glb_setup();
    p.expected_revision = 1;
    let original = a.propose_candidate(&p).unwrap();
    let result = a.attempt_accept_candidate(&p.id).unwrap();
    assert!(
        matches!(result,Acceptance::Conflict{current,..} if current.revision==0 && current.version_id=="base")
    );
    p.expected_revision = 0;
    assert!(a.propose_candidate(&p).is_err());
    assert_eq!(a.candidate(&p.id).unwrap(), Some(original));
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        0
    );
}

#[test]
fn unsupported_contract_and_required_quality_cannot_initialize_current() {
    let (_d, mut a, p) = glb_setup();
    let mut c = p.contract.unwrap();
    c.contract_id = "PRODUCTION".into();
    assert!(a.initialize_candidate_current("base", &c).is_err());
    c.contract_id = "TEST_FIXTURE_GLB_STRUCTURE_V1".into();
    c.required_quality.push("skin.weights_numeric".into());
    assert!(a.initialize_candidate_current("base", &c).is_err());
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        0
    );
}

#[test]
fn brief_storage_lease_contention_does_not_skip_conditional_acceptance() {
    let (d, mut a, p) = glb_setup();
    a.propose_candidate(&p).unwrap();
    let lock = d.path().join(".aw-storage.lock");
    let lease = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(lease);
        fs::remove_file(lock).unwrap();
    });
    let result = a.attempt_accept_candidate(&p.id);
    release.join().unwrap();
    assert!(
        matches!(result, Ok(Acceptance::Accepted { .. })),
        "{result:?}"
    );
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        1
    );
}

#[test]
fn persistent_storage_lease_is_bounded_busy_without_advancing() {
    let (d, mut a, p) = glb_setup();
    a.propose_candidate(&p).unwrap();
    let _lease = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(d.path().join(".aw-storage.lock"))
        .unwrap();
    let start = std::time::Instant::now();
    assert!(matches!(
        a.attempt_accept_candidate(&p.id),
        Err(assetweft_core::library::LibraryError::Busy(_))
    ));
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        0
    );
    assert!(a.candidate(&p.id).unwrap().is_some());
    assert!(a.candidate_attempt(&p.id).unwrap().is_none());
}

fn candidate_receipts(path: &std::path::Path) -> Vec<(String, String)> {
    let raw = Connection::open(path).unwrap();
    let mut statement = raw
        .prepare("SELECT candidate_id,receipt FROM aw_candidate_attempts ORDER BY candidate_id")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<(String, String)>, _>>()
        .unwrap()
}

fn insert_restored_candidate(
    path: &std::path::Path,
    row_id: &str,
    embedded_id: &str,
    template: &Candidate,
) {
    let mut descriptor = serde_json::to_value(template).unwrap();
    descriptor["id"] = serde_json::Value::String(embedded_id.into());
    let encoded = serde_json::to_string(&descriptor).unwrap();
    let raw = Connection::open(path).unwrap();
    raw.execute(
        "INSERT INTO aw_candidates(id,base_version,candidate_version,descriptor) VALUES(?1,?2,?3,?4)",
        rusqlite::params![
            row_id,
            template.base_version,
            template.candidate_version,
            encoded
        ],
    )
    .unwrap();
}

fn identity_error() -> assetweft_core::library::LibraryError {
    assetweft_core::library::LibraryError::InvalidInput("candidate identity")
}

fn cap_contract(max_component_bytes: u64) -> assetweft_core::candidate::FixtureContract {
    assetweft_core::candidate::FixtureContract {
        contract_id: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
        max_component_bytes,
        required_quality: vec![],
    }
}

fn cap_setup() -> (tempfile::TempDir, Application, Proposal) {
    let d = tempfile::tempdir().unwrap();
    let mut a = Application::open(&d.path().join("library.sqlite")).unwrap();
    a.initialize_library().unwrap();
    let bytes = include_bytes!("fixtures/validation/mesh_only.glb");
    put_glb(&mut a, "mesh0", bytes);
    put_glb(&mut a, "mesh1", bytes);
    put(&mut a, "base", "character", b"base manifest");
    put(&mut a, "proposed", "character", b"candidate manifest");
    reference(&mut a, "base", ComponentRole::Mesh, "mesh0");
    reference(&mut a, "proposed", ComponentRole::Mesh, "mesh1");
    let p = Proposal {
        id: "cap-1".into(),
        base_version: "base".into(),
        candidate_version: "proposed".into(),
        expected_revision: 0,
        editable: vec![ComponentRole::Mesh],
        lock_mode: LockMode::Byte,
        contract: Some(cap_contract(bytes.len() as u64)),
    };
    (d, a, p)
}

fn grow_object(d: &tempfile::TempDir, a: &mut Application, id: &str, extra: u8) -> u64 {
    let hash = a.asset_version(id).unwrap().unwrap().content_hash;
    let path = d.path().join("objects").join(hash);
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(extra);
    fs::write(&path, &bytes).unwrap();
    bytes.len() as u64
}

#[test]
fn declared_cap_rejects_proposal_before_managed_hashing() {
    let (d, mut a, p) = cap_setup();
    let size = grow_object(&d, &mut a, "mesh1", b'X');
    assert_eq!(size, p.contract.as_ref().unwrap().max_component_bytes + 1);
    assert_eq!(
        a.propose_candidate(&p),
        Err(assetweft_core::library::LibraryError::InvalidInput(
            "component byte limit"
        ))
    );
    assert!(a.candidate(&p.id).unwrap().is_none());
}

#[test]
fn declared_cap_rejects_initialization_before_managed_hashing() {
    let (d, mut a, p) = cap_setup();
    let size = grow_object(&d, &mut a, "mesh0", b'X');
    let contract = p.contract.unwrap();
    assert_eq!(size, contract.max_component_bytes + 1);
    assert_eq!(
        a.initialize_candidate_current("base", &contract),
        Err(assetweft_core::library::LibraryError::InvalidInput(
            "component byte limit"
        ))
    );
    assert!(a.candidate_current("character").unwrap().is_none());
}

#[test]
fn declared_cap_rejects_attempt_before_managed_hashing_without_receipt() {
    let (d, mut a, p) = cap_setup();
    a.initialize_candidate_current("base", p.contract.as_ref().unwrap())
        .unwrap();
    let candidate = a.propose_candidate(&p).unwrap();
    let size = grow_object(&d, &mut a, "mesh1", b'Y');
    assert_eq!(
        size,
        candidate.contract.as_ref().unwrap().max_component_bytes + 1
    );
    assert_eq!(
        a.attempt_accept_candidate(&p.id),
        Err(assetweft_core::library::LibraryError::InvalidInput(
            "component byte limit"
        ))
    );
    assert!(a.candidate_attempt(&p.id).unwrap().is_none());
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        0
    );
}

#[test]
fn declared_cap_rejects_attempt_base_remeasure_before_managed_hashing() {
    let glb = include_bytes!("fixtures/validation/mesh_only.glb");
    let mut shifted = glb.to_vec();
    let json_len = u32::from_le_bytes(shifted[12..16].try_into().unwrap()) as usize;
    let total = u32::from_le_bytes(shifted[8..12].try_into().unwrap()) + 4;
    shifted.splice(20 + json_len..20 + json_len, [b' '; 4]);
    shifted[12..16].copy_from_slice(&((json_len + 4) as u32).to_le_bytes());
    shifted[8..12].copy_from_slice(&total.to_le_bytes());
    let d = tempfile::tempdir().unwrap();
    let mut a = Application::open(&d.path().join("library.sqlite")).unwrap();
    a.initialize_library().unwrap();
    put_glb(&mut a, "mesh0", glb);
    put_glb(&mut a, "mesh1", &shifted);
    put(&mut a, "base", "character", b"base manifest");
    put(&mut a, "proposed", "character", b"candidate manifest");
    reference(&mut a, "base", ComponentRole::Mesh, "mesh0");
    reference(&mut a, "proposed", ComponentRole::Mesh, "mesh1");
    let contract = cap_contract(shifted.len() as u64);
    a.initialize_candidate_current("base", &contract).unwrap();
    let p = Proposal {
        id: "cap-base".into(),
        base_version: "base".into(),
        candidate_version: "proposed".into(),
        expected_revision: 0,
        editable: vec![ComponentRole::Mesh],
        lock_mode: LockMode::Byte,
        contract: Some(contract),
    };
    a.propose_candidate(&p).unwrap();
    let hash = a.asset_version("mesh0").unwrap().unwrap().content_hash;
    let file = fs::OpenOptions::new()
        .write(true)
        .open(d.path().join("objects").join(hash))
        .unwrap();
    file.set_len(p.contract.as_ref().unwrap().max_component_bytes + 1)
        .unwrap();
    assert_eq!(
        a.attempt_accept_candidate(&p.id),
        Err(assetweft_core::library::LibraryError::InvalidInput(
            "component byte limit"
        ))
    );
    assert!(a.candidate_attempt(&p.id).unwrap().is_none());
    assert_eq!(
        a.candidate_current("character").unwrap().unwrap().revision,
        0
    );
}

#[test]
fn declared_cap_boundaries_and_contractless_managed_ceiling_are_retained() {
    let bytes = include_bytes!("fixtures/validation/mesh_only.glb");
    let exact = bytes.len() as u64;
    let (_d, mut a, p) = cap_setup();
    a.initialize_candidate_current("base", p.contract.as_ref().unwrap())
        .unwrap();
    a.propose_candidate(&p).unwrap();
    assert!(matches!(
        a.attempt_accept_candidate(&p.id).unwrap(),
        Acceptance::Accepted { .. }
    ));
    let (_d2, mut b, p2) = cap_setup();
    let mut under = p2.clone();
    under.contract = Some(cap_contract(exact - 1));
    assert_eq!(
        b.propose_candidate(&under),
        Err(assetweft_core::library::LibraryError::InvalidInput(
            "component byte limit"
        ))
    );
    assert!(b.candidate(&under.id).unwrap().is_none());
    let (d3, mut c, mut p3) = cap_setup();
    p3.contract = None;
    let hash = c.asset_version("mesh1").unwrap().unwrap().content_hash;
    let file = fs::OpenOptions::new()
        .write(true)
        .open(d3.path().join("objects").join(hash))
        .unwrap();
    file.set_len(16 * 1024 * 1024 + 1).unwrap();
    assert_eq!(
        c.propose_candidate(&p3),
        Err(assetweft_core::library::LibraryError::InvalidInput(
            "bounded managed regular file"
        ))
    );
    assert!(c.candidate(&p3.id).unwrap().is_none());
}

#[test]
fn persisted_descriptor_identity_is_bound_to_the_lookup_key() {
    let (d, mut a, p) = glb_setup();
    let path = d.path().join("library.sqlite");
    let candidate = a.propose_candidate(&p).unwrap();
    let durable = a.attempt_accept_candidate(&p.id).unwrap();
    assert!(matches!(&durable, Acceptance::Accepted { .. }));
    let head = a.candidate_current("character").unwrap();
    assert_eq!(head.as_ref().unwrap().revision, 1);
    let receipts = candidate_receipts(&path);
    assert_eq!(receipts.len(), 1);

    insert_restored_candidate(&path, "cand-a", &candidate.id, &candidate);
    insert_restored_candidate(&path, "cand-c", "cand-ghost", &candidate);
    let mut pending = p.clone();
    pending.id = "cand-b2".into();
    pending.candidate_version = "other".into();
    let staged = a.propose_candidate(&pending).unwrap();
    insert_restored_candidate(&path, "cand-d", &staged.id, &staged);

    for row in ["cand-a", "cand-c", "cand-d"] {
        assert_eq!(a.candidate(row), Err(identity_error()), "{row} read");
        assert_eq!(
            a.attempt_accept_candidate(row),
            Err(identity_error()),
            "{row} attempt"
        );
    }
    assert_eq!(a.candidate_current("character").unwrap(), head);
    assert_eq!(candidate_receipts(&path), receipts);
    for row in ["cand-a", "cand-c", "cand-d", "cand-ghost"] {
        assert!(a.candidate_attempt(row).unwrap().is_none(), "{row} receipt");
    }

    assert_eq!(a.candidate(&p.id).unwrap(), Some(candidate.clone()));
    assert_eq!(a.candidate_attempt(&p.id).unwrap(), Some(durable.clone()));
    assert_eq!(a.attempt_accept_candidate(&p.id).unwrap(), durable);

    let object = d
        .path()
        .join("objects")
        .join(&candidate.proposed[0].measured_sha256);
    fs::remove_file(&object).unwrap();
    assert_eq!(a.attempt_accept_candidate("cand-a"), Err(identity_error()));
    assert_eq!(a.candidate("cand-a"), Err(identity_error()));
    assert_eq!(a.candidate_current("character").unwrap(), head);
    assert_eq!(candidate_receipts(&path), receipts);
}
