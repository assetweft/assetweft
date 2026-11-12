//! 职责: AW-013:A 缺陷型回归——真实 JSON parse→validate、锁定范围不可绕过、缺字段路径、
//!        补全不得冒真值、授权/引擎只声明不验证、配方 NOT_EXECUTED。
//! 依赖: assetweft_core::generation_spec（Rust 类型与唯一验证入口）、serde_json（已锁 wire 解析）。
//! 调用方: C1（cargo test）；不写数据库、不联网、不执行模型/引擎、不读凭据。
//! 不变量: 样例全部 TEST_FIXTURE/metadata-only；期望断言只在本文件（不设可弱化 golden）；
//!        正例必须通过、锁绕过必须明确拒绝、缺字段须给具体路径、补全不得晋升真值。

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use assetweft_core::generation_spec::{
    ComponentRole, IssueCode, RECIPE_STATUS, STATUS_REJECTED, TrustedContext, validate,
};
use assetweft_core::generation_spec::{GenerationSpecRequest, recipe_examples};
use assetweft_core::{LockMode, LockStatus};

#[derive(Debug, Deserialize)]
struct CasesFile {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    fixture: FixtureMeta,
    context: TrustedContext,
    request: GenerationSpecRequest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureMeta {
    marker: String,
    metadata_only: bool,
    #[allow(dead_code)]
    note: Option<String>,
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generation_spec/cases.json")
}

/// 真实 wire 解析：JSON → Rust 类型；同时校验 TEST_FIXTURE 溯源与 id 唯一（信封前置）。
fn load_cases() -> Vec<Case> {
    let text = fs::read_to_string(fixture_path()).expect("cases.json must be readable");
    let parsed: CasesFile =
        serde_json::from_str(&text).expect("cases.json must parse into Rust types");
    assert!(!parsed.cases.is_empty(), "fixture set must not be empty");
    let mut ids = HashSet::new();
    for c in &parsed.cases {
        assert_eq!(c.fixture.marker, "TEST_FIXTURE", "case {} marker", c.id);
        assert!(
            c.fixture.metadata_only,
            "case {} must be metadata-only",
            c.id
        );
        assert!(ids.insert(c.id.clone()), "duplicate case id {}", c.id);
    }
    parsed.cases
}

fn case(id: &str) -> Case {
    load_cases()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("missing case {id}"))
}

/// 目标 1（正例）：正常允许 motion、锁 mesh/materials/rig 的请求必须通过；声明一致≠执行 PASS。
#[test]
fn positive_cases_are_declaration_consistent() {
    for id in ["valid_single_motion_increment", "valid_pending_inference"] {
        let c = case(id);
        let out = validate(&c.request, &c.context);
        assert!(out.is_declaration_consistent(), "{id}: {:?}", out.issues);
        assert!(out.issues.is_empty(), "{id}: no issues expected");
        assert!(
            !out.source.verified_by_checker(),
            "{id}: authorization must never be verified by this checker"
        );
        assert!(out.not_run.contains(&"executor_admission"));
        assert!(out.not_run.contains(&"engine_execution"));
    }
}

/// 目标 1（反例）：删空/省略/冲突锁定范围一律 LOCK_VIOLATION 且附明确原因。
#[test]
fn lock_scope_bypass_is_rejected_with_explicit_reason() {
    for id in [
        "invalid_lock_omitted",
        "invalid_lock_tampered",
        "invalid_lock_conflict",
    ] {
        let c = case(id);
        let out = validate(&c.request, &c.context);
        assert_eq!(out.status, STATUS_REJECTED, "{id}");
        assert!(
            out.has_code(IssueCode::LockViolation),
            "{id}: {:?}",
            out.issues
        );
        assert!(
            out.issues
                .iter()
                .any(|i| i.code == IssueCode::LockViolation && !i.message.is_empty()),
            "{id}: LOCK_VIOLATION must carry an explicit reason"
        );
    }
    // 删空特例：必须出现"不能自删锁获得授权"的明确原因。
    let omitted = case("invalid_lock_omitted");
    let out = validate(&omitted.request, &omitted.context);
    assert!(
        out.issues
            .iter()
            .any(|i| i.message.contains("drop all pre-declared locks")),
        "empty locked_components must be called out explicitly"
    );
}

/// 目标 2：输入不完整返回稳定、可定位的具体字段路径。
#[test]
fn incomplete_input_returns_specific_field_paths() {
    let c = case("invalid_missing_base_version");
    let out = validate(&c.request, &c.context);
    assert!(out.has_field_path("base_version"), "{:?}", out.issues);
    assert!(out.has_code(IssueCode::InputIncomplete));

    let c2 = case("invalid_missing_reference_purpose");
    let out2 = validate(&c2.request, &c2.context);
    assert!(
        out2.has_field_path("references[0].purpose"),
        "{:?}",
        out2.issues
    );
    assert_eq!(out2.status, STATUS_REJECTED);
}

/// 未知费用允许 null，不自动填 0。
#[test]
fn unknown_cost_must_not_be_zero_filled() {
    let c = case("invalid_unknown_cost_zero");
    let out = validate(&c.request, &c.context);
    assert!(out.has_field_path("budget.cost_value"), "{:?}", out.issues);
    assert!(out.has_code(IssueCode::InvalidInput));
}

/// 目标 3：隐藏结构补全/推断不得晋升为 observed/known_correct 真值；待确认推断正例仍可通过。
#[test]
fn completion_cannot_be_promoted_to_truth() {
    let bad = case("invalid_completion_observed");
    let bad_out = validate(&bad.request, &bad.context);
    assert!(
        bad_out.has_field_path("completion[0].state"),
        "{:?}",
        bad_out.issues
    );
    assert!(bad_out.has_code(IssueCode::InvalidInput));

    let good = case("valid_pending_inference");
    let good_out = validate(&good.request, &good.context);
    assert!(
        good_out.is_declaration_consistent(),
        "{:?}",
        good_out.issues
    );
    assert_eq!(
        good.request.completion[0].state.as_deref(),
        Some("PENDING_CONFIRMATION")
    );
}

/// fixture 写 authorized=true / engine=UNKNOWN 只是声明，不宣称真的有授权或执行器准入。
#[test]
fn authorization_and_engine_fields_are_declared_not_verified() {
    let c = case("valid_single_motion_increment");
    assert_eq!(c.request.authorized, Some(true));
    assert_eq!(c.request.declared_engine.as_deref(), Some("UNKNOWN"));
    assert_eq!(c.request.declared_executor.as_deref(), Some("UNKNOWN"));
    let out = validate(&c.request, &c.context);
    assert!(!out.source.verified_by_checker());
    assert!(out.not_run.contains(&"real_authorization_verification"));
    assert!(out.not_run.contains(&"executor_admission"));
    assert!(out.not_run.contains(&"engine_execution"));
    assert!(out.not_run.contains(&"semantic_normalizer"));
}

/// NOT_DEFINED 是声明状态不是 lock_mode；复用既有 LockStatus/LockMode，不混作模式。
#[test]
fn not_defined_is_a_declaration_state_not_a_lock_mode() {
    let mut c = case("valid_single_motion_increment");
    c.request.locked_components[0].declaration = Some("NOT_DEFINED".to_owned());
    let out = validate(&c.request, &c.context);
    assert!(out.has_code(IssueCode::Unsupported), "{:?}", out.issues);
    assert!(out.has_field_path("locked_components[0].declaration"));
    assert_eq!(LockStatus::parse("NOT_DEFINED"), Ok(LockStatus::NotDefined));
    assert_eq!(LockMode::parse("BYTE"), Ok(LockMode::Byte));
}

/// 线性 recipe examples 明确 NOT_EXECUTED，但其规格本身可通过声明一致性验证。
#[test]
fn recipe_examples_are_not_executed_but_validate() {
    let recipes = recipe_examples();
    assert_eq!(recipes.len(), 2);
    for r in &recipes {
        assert_eq!(r.status, RECIPE_STATUS, "{}", r.id);
        assert!(r.note.contains("TEST_FIXTURE"), "{}", r.id);
        let out = validate(&r.request, &r.context);
        assert!(
            out.is_declaration_consistent(),
            "{}: {:?}",
            r.id,
            out.issues
        );
    }
}

/// wire 上下文与 UI/Agent 在 Rust 内构造的上下文给出同一验证结果（同一入口、无漂移）。
#[test]
fn wire_and_programmatic_context_agree() {
    let c = case("valid_single_motion_increment");
    let wire = validate(&c.request, &c.context);
    let mut built = TrustedContext::from_contract(
        "AW-CONTRACT-001",
        &[
            (ComponentRole::Mesh, LockMode::Byte),
            (ComponentRole::Materials, LockMode::Byte),
            (ComponentRole::Rig, LockMode::Semantic),
        ],
    );
    built.declared_locks[2].canonicalization_id =
        c.context.declared_locks[2].canonicalization_id.clone();
    built.declared_locks[2].canonicalization_version =
        c.context.declared_locks[2].canonicalization_version.clone();
    let programmatic = validate(&c.request, &built);
    assert_eq!(wire.status, programmatic.status);
    assert_eq!(wire.issues, programmatic.issues);
}

/// 非法 JSON/未知字段不得被静默接受（类型/JSON 只验证声明一致性）。
#[test]
fn malformed_wire_is_rejected_at_parse() {
    let bad = "{\"cases\":[{\"id\":\"x\",\"fixture\":{\"marker\":\"TEST_FIXTURE\",\"metadata_only\":true},\"context\":{},\"request\":{},\"unexpected\":1}]}";
    assert!(
        serde_json::from_str::<CasesFile>(bad).is_err(),
        "unknown fields must not be silently accepted"
    );
}

/// Every input reference must be present; report its wire array index without resolving files.
#[test]
fn blank_input_reference_reports_numeric_path() {
    for refs in [vec!["   "], vec!["metadata://fixture/mesh.bin", "\t\n"]] {
        let mut c = case("valid_single_motion_increment");
        c.request.input_refs = refs.iter().map(|s| (*s).to_owned()).collect();
        let out = validate(&c.request, &c.context);
        assert_eq!(out.status, STATUS_REJECTED, "{:?}", out.issues);
        let path = format!("input_refs[{}]", refs.len() - 1);
        assert!(
            out.issues
                .iter()
                .any(|i| i.code == IssueCode::InputIncomplete && i.field_path == path),
            "{:?}",
            out.issues
        );
    }
}

/// Known cost needs a value; unknown/null and explicitly known zero remain valid declarations.
#[test]
fn known_cost_requires_value_without_inventing_amount() {
    let mut c = case("valid_single_motion_increment");
    assert!(validate(&c.request, &c.context).is_declaration_consistent());
    c.request.budget.as_mut().unwrap().cost_status = Some("known".to_owned());
    let out = validate(&c.request, &c.context);
    assert_eq!(out.status, STATUS_REJECTED, "{:?}", out.issues);
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == IssueCode::InputIncomplete && i.field_path == "budget.cost_value"),
        "{:?}",
        out.issues
    );
    c.request.budget.as_mut().unwrap().cost_value = Some(0);
    assert!(validate(&c.request, &c.context).is_declaration_consistent());
}

/// Reordering request locks preserves valid scope and identifies conflicts by actual wire index.
#[test]
fn reordered_lock_conflict_reports_actual_numeric_index() {
    for rig_index in [0, 1, 2] {
        let mut c = case("valid_single_motion_increment");
        c.request.locked_components.swap(2, rig_index);
        assert!(validate(&c.request, &c.context).is_declaration_consistent());
        c.request.locked_components[rig_index].declaration = Some("BYTE".to_owned());
        let out = validate(&c.request, &c.context);
        assert_eq!(out.status, STATUS_REJECTED);
        let path = format!("locked_components[{rig_index}].declaration");
        assert!(
            out.issues
                .iter()
                .any(|i| i.code == IssueCode::LockViolation && i.field_path == path),
            "{:?}",
            out.issues
        );
    }
}

/// Check each required free-text declaration independently, retaining exact wire bytes.
fn required_text_regression(path: &str, set: impl Fn(&mut Case, &str)) {
    let mut positive = case("valid_pending_inference");
    assert!(validate(&positive.request, &positive.context).is_declaration_consistent());
    // Surrounding whitespace on a nonempty declaration must not be normalized away.
    set(&mut positive, " \tfixture:declared-value\n");
    let before = format!("{:?}{:?}", positive.request, positive.context);
    assert!(validate(&positive.request, &positive.context).is_declaration_consistent());
    assert_eq!(
        format!("{:?}{:?}", positive.request, positive.context),
        before
    );
    for blank in [" ", "\t\r\n", "\u{2003}\u{00a0}"] {
        let mut negative = case("valid_pending_inference");
        set(&mut negative, blank);
        let out = validate(&negative.request, &negative.context);
        assert_eq!(out.status, STATUS_REJECTED, "{path}: {:?}", out.issues);
        assert_eq!(out.issues.len(), 1, "{path}: {:?}", out.issues);
        assert_eq!(out.issues[0].code, IssueCode::InputIncomplete, "{path}");
        assert_eq!(out.issues[0].field_path, path);
    }
}

#[test]
fn required_base_version_rejects_whitespace() {
    required_text_regression("base_version", |c, value| {
        c.request.base_version = Some(value.into())
    });
}

#[test]
fn required_reference_id_rejects_whitespace() {
    required_text_regression("references[0].reference_id", |c, value| {
        c.request.references[0].reference_id = Some(value.into())
    });
}

#[test]
fn required_output_contract_rejects_whitespace() {
    required_text_regression("output_contract", |c, value| {
        c.request.output_contract = Some(value.into())
    });
}

#[test]
fn required_environment_rejects_whitespace() {
    required_text_regression("allowed_environment", |c, value| {
        c.request.allowed_environment = Some(value.into())
    });
}

#[test]
fn required_completion_path_rejects_whitespace() {
    required_text_regression("completion[0].path", |c, value| {
        c.request.completion[0].path = Some(value.into())
    });
}

#[test]
fn required_trusted_contract_id_rejects_whitespace() {
    required_text_regression("context.contract_id", |c, value| {
        c.context.contract_id = Some(value.into())
    });
}

#[test]
fn old_semantic_without_identity_is_incomplete() {
    // Frozen pre-strengthening wire fixture: this regression remains the old request/context.
    let old: CasesFile = serde_json::from_str(include_str!(
        "fixtures/generation_spec/legacy_missing_identity.json"
    ))
    .unwrap();
    let c = &old.cases[0];
    let result = validate(&c.request, &c.context);
    assert!(
        result
            .issues
            .iter()
            .any(|i| i.code == IssueCode::InputIncomplete
                && i.field_path == "locked_components[2].canonicalization_id"),
        "{result:?}"
    );
}

#[test]
fn semantic_identity_contract_matrix() {
    let c = case("valid_single_motion_increment");
    assert!(validate(&c.request, &c.context).is_declaration_consistent());
    for field in ["canonicalization_id", "canonicalization_version"] {
        for value in [None, Some(""), Some(" \t\n"), Some("\u{2003}\u{3000}")] {
            let mut wire = serde_json::json!({"role":"rig","declaration":"SEMANTIC",
                "canonicalization_id":"TEST_FIXTURE:rig","canonicalization_version":"TEST_FIXTURE:v1"});
            wire[field] = value.into();
            let lock = serde_json::from_value(wire).unwrap();
            let mut req = c.request.clone();
            req.locked_components[2] = lock;
            let out = validate(&req, &c.context);
            assert!(
                out.issues
                    .iter()
                    .any(|i| i.code == IssueCode::InputIncomplete
                        && i.field_path == format!("locked_components[2].{field}"))
            );
            let mut ctx = c.context.clone();
            ctx.declared_locks[2] = req.locked_components[2].clone();
            let out = validate(&c.request, &ctx);
            assert!(
                out.issues
                    .iter()
                    .any(|i| i.code == IssueCode::InputIncomplete
                        && i.field_path == format!("context.declared_locks[2].{field}"))
            );
        }
        let mut req = c.request.clone();
        req.locked_components.swap(0, 2);
        if field == "canonicalization_id" {
            req.locked_components[0].canonicalization_id = Some(" TEST_FIXTURE:rig ".into());
        } else {
            req.locked_components[0].canonicalization_version = Some("other".into());
        }
        let snapshot = req.clone();
        let out = validate(&req, &c.context);
        assert!(out.issues.iter().any(|i| i.code == IssueCode::LockViolation
            && i.field_path == format!("locked_components[0].{field}")));
        assert_eq!(req, snapshot);
    }
    let mut req = c.request.clone();
    let mut ctx = c.context.clone();
    req.locked_components[2].declaration = Some("BYTE".into());
    ctx.declared_locks[2].declaration = Some("BYTE".into());
    req.locked_components[2].canonicalization_id = None;
    req.locked_components[2].canonicalization_version = None;
    ctx.declared_locks = req.locked_components.clone();
    assert!(validate(&req, &ctx).is_declaration_consistent());
    // Unknown opaque identities are declaration-only and never normalize/exe cute.
    req.locked_components[2].declaration = Some("SEMANTIC".into());
    req.locked_components[2].canonicalization_id = Some(" unknown algorithm ".into());
    req.locked_components[2].canonicalization_version = Some("opaque version".into());
    ctx.declared_locks = req.locked_components.clone();
    let out = validate(&req, &ctx);
    assert!(out.is_declaration_consistent());
    assert!(out.not_run.contains(&"semantic_normalizer"));
    assert!(
        serde_json::from_str::<assetweft_core::generation_spec::LockClaim>(
            r#"{"role":"rig","unrecognized_identity":1}"#
        )
        .is_err()
    );
}
