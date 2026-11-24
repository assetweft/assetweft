use serde::Deserialize;

use crate::assurance::{SourceClass, SourceDeclaration};
use crate::{LockMode, LockStatus};

pub const VALIDATOR_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 配方样例状态：只描述规格，**从未执行**。
pub const RECIPE_STATUS: &str = "NOT_EXECUTED";
pub const STATUS_DECLARATION_CONSISTENT: &str = "DECLARATION_CONSISTENT";
pub const STATUS_REJECTED: &str = "REJECTED";
pub const STATE_PENDING_CONFIRMATION: &str = "PENDING_CONFIRMATION";

/// 组件角色词表：与 `contracts/first-delivery-contract.schema.json` `definitions.role` 的 wire 值一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentRole {
    Mesh,
    Materials,
    Rig,
    Motion,
    Vfx,
    Audio,
    Dialogue,
}

impl ComponentRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Materials => "materials",
            Self::Rig => "rig",
            Self::Motion => "motion",
            Self::Vfx => "vfx",
            Self::Audio => "audio",
            Self::Dialogue => "dialogue",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "mesh" => Some(Self::Mesh),
            "materials" => Some(Self::Materials),
            "rig" => Some(Self::Rig),
            "motion" => Some(Self::Motion),
            "vfx" => Some(Self::Vfx),
            "audio" => Some(Self::Audio),
            "dialogue" => Some(Self::Dialogue),
            _ => None,
        }
    }
}

/// 参考用途词表（docs/CONTRACTS.md §3：外观/姿态/材质/风格/声音）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferencePurpose {
    Appearance,
    Pose,
    Material,
    Style,
    Sound,
}

impl ReferencePurpose {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Appearance => "APPEARANCE",
            Self::Pose => "POSE",
            Self::Material => "MATERIAL",
            Self::Style => "STYLE",
            Self::Sound => "SOUND",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "APPEARANCE" => Some(Self::Appearance),
            "POSE" => Some(Self::Pose),
            "MATERIAL" => Some(Self::Material),
            "STYLE" => Some(Self::Style),
            "SOUND" => Some(Self::Sound),
            _ => None,
        }
    }
}

/// 推断/补全所声称的状态。只有 `PendingConfirmation` 合法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceState {
    PendingConfirmation,
    Observed,
    KnownCorrect,
}

impl InferenceState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PendingConfirmation => "PENDING_CONFIRMATION",
            Self::Observed => "OBSERVED",
            Self::KnownCorrect => "KNOWN_CORRECT",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "PENDING_CONFIRMATION" => Some(Self::PendingConfirmation),
            "OBSERVED" => Some(Self::Observed),
            "KNOWN_CORRECT" => Some(Self::KnownCorrect),
            _ => None,
        }
    }
}

/// 错误类别；名称回指 docs/CONTRACTS.md §10.5（2/3/4/1/5），不是执行结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueCode {
    InvalidInput,
    InputIncomplete,
    LockViolation,
    Fail,
    Unsupported,
}

impl IssueCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "INVALID_INPUT",
            Self::InputIncomplete => "INPUT_INCOMPLETE",
            Self::LockViolation => "LOCK_VIOLATION",
            Self::Fail => "FAIL",
            Self::Unsupported => "UNSUPPORTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecIssue {
    pub code: IssueCode,
    pub field_path: String,
    pub message: String,
}

/// 验证结果。`DeclarationConsistent` 只表示声明范围一致，**不授予执行权、不证明真实素材/执行器**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationOutcome {
    pub status: &'static str,
    pub issues: Vec<SpecIssue>,
    pub source: SourceDeclaration,
    pub not_run: Vec<&'static str>,
}

impl ValidationOutcome {
    #[must_use]
    pub fn is_declaration_consistent(&self) -> bool {
        self.status == STATUS_DECLARATION_CONSISTENT
    }

    #[must_use]
    pub fn has_code(&self, code: IssueCode) -> bool {
        self.issues.iter().any(|i| i.code == code)
    }

    #[must_use]
    pub fn has_field_path(&self, path: &str) -> bool {
        self.issues.iter().any(|i| i.field_path == path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceUse {
    #[serde(default)]
    pub reference_id: Option<String>,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default)]
    pub hidden_regions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockClaim {
    /// Opaque predeclared algorithm identity; required for SEMANTIC, never executed here.
    #[serde(default)]
    pub canonicalization_id: Option<String>,
    /// Opaque version, compared verbatim; no normalization or semver inference.
    #[serde(default)]
    pub canonicalization_version: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub declaration: Option<String>,
}

/// 预算声明；unknown 费用必须 null，不自动填 0。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(default)]
    pub max_calls: Option<u64>,
    #[serde(default)]
    pub cost_status: Option<String>,
    #[serde(default)]
    pub cost_value: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionClaim {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationSpecRequest {
    /// 基础版本（S3 首条动作增量必需）。
    #[serde(default)]
    pub base_version: Option<String>,
    /// 输入引用（S3 首条动作增量必需）。
    #[serde(default)]
    pub input_refs: Vec<String>,
    #[serde(default)]
    pub references: Vec<ReferenceUse>,
    #[serde(default)]
    pub editable_components: Vec<String>,
    /// 请求锁定组件；必须与预声明合同对照，不能自删。
    #[serde(default)]
    pub locked_components: Vec<LockClaim>,
    #[serde(default)]
    pub output_contract: Option<String>,
    #[serde(default)]
    pub budget: Option<Budget>,
    #[serde(default)]
    pub allowed_environment: Option<String>,
    /// 调用方声称的授权（只记录，**不验证**）。
    #[serde(default)]
    pub authorized: Option<bool>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub declared_engine: Option<String>,
    #[serde(default)]
    pub declared_executor: Option<String>,
    /// 隐藏/补全/推断，禁止晋升真值。
    #[serde(default)]
    pub completion: Vec<CompletionClaim>,
}

/// 调用方可信上下文：来自**预声明合同**，不是从请求里读。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedContext {
    #[serde(default)]
    pub contract_id: Option<String>,
    #[serde(default)]
    pub declared_locks: Vec<LockClaim>,
    #[serde(default)]
    pub source_note: Option<String>,
}

impl TrustedContext {
    /// 供 UI/Agent 构造上下文；SEMANTIC identity is deliberately absent.
    /// Callers must explicitly set canonicalization_id/version from their predeclared contract.
    #[must_use]
    pub fn from_contract(contract_id: &str, locks: &[(ComponentRole, LockMode)]) -> Self {
        Self {
            contract_id: Some(contract_id.to_owned()),
            declared_locks: locks
                .iter()
                .map(|(role, mode)| LockClaim {
                    canonicalization_id: None,
                    canonicalization_version: None,
                    role: Some(role.as_str().to_owned()),
                    declaration: Some(mode.as_str().to_owned()),
                })
                .collect(),
            source_note: Some(
                "caller-provided pre-declared contract; NOT verified by generation_spec".to_owned(),
            ),
        }
    }
}

fn issue(code: IssueCode, field_path: impl Into<String>, message: impl Into<String>) -> SpecIssue {
    SpecIssue {
        code,
        field_path: field_path.into(),
        message: message.into(),
    }
}

fn nonblank(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|s| !s.trim().is_empty())
}

/// Declaration consistency grants no execution permission. Issues include stable field paths.
#[must_use]
pub fn validate(request: &GenerationSpecRequest, context: &TrustedContext) -> ValidationOutcome {
    let mut issues: Vec<SpecIssue> = Vec::new();

    if !nonblank(&request.base_version) {
        issues.push(issue(
            IssueCode::InputIncomplete,
            "base_version",
            "missing or empty",
        ));
    }
    if request.input_refs.is_empty() {
        issues.push(issue(
            IssueCode::InputIncomplete,
            "input_refs",
            "at least one input_refs is required",
        ));
    }
    for (i, reference) in request.input_refs.iter().enumerate() {
        if reference.trim().is_empty() {
            issues.push(issue(
                IssueCode::InputIncomplete,
                format!("input_refs[{i}]"),
                "missing or empty",
            ));
        }
    }
    if request.references.is_empty() {
        issues.push(issue(
            IssueCode::InputIncomplete,
            "references",
            "at least one reference with purpose is required",
        ));
    }
    for (i, r) in request.references.iter().enumerate() {
        if !nonblank(&r.reference_id) {
            issues.push(issue(
                IssueCode::InputIncomplete,
                format!("references[{i}].reference_id"),
                "missing or empty",
            ));
        }
        match &r.purpose {
            None => issues.push(issue(
                IssueCode::InputIncomplete,
                format!("references[{i}].purpose"),
                "purpose is required (外观/姿态/材质/风格/声音)",
            )),
            Some(p) if ReferencePurpose::parse(p).is_none() => issues.push(issue(
                IssueCode::InvalidInput,
                format!("references[{i}].purpose"),
                format!("unknown purpose {p:?}"),
            )),
            Some(_) => {}
        }
    }
    if !nonblank(&request.output_contract) {
        issues.push(issue(
            IssueCode::InputIncomplete,
            "output_contract",
            "missing or empty",
        ));
    }
    if !nonblank(&request.allowed_environment) {
        issues.push(issue(
            IssueCode::InputIncomplete,
            "allowed_environment",
            "missing or empty",
        ));
    }
    match &request.budget {
        None => issues.push(issue(
            IssueCode::InputIncomplete,
            "budget",
            "budget is required",
        )),
        Some(b) => {
            if b.max_calls.is_none() {
                issues.push(issue(
                    IssueCode::InputIncomplete,
                    "budget.max_calls",
                    "missing",
                ));
            }
            match b.cost_status.as_deref() {
                None => issues.push(issue(
                    IssueCode::InputIncomplete,
                    "budget.cost_status",
                    "missing",
                )),
                Some("unknown") => {
                    if b.cost_value.is_some() {
                        issues.push(issue(
                            IssueCode::InvalidInput,
                            "budget.cost_value",
                            "unknown cost must be null, not 0 or a number",
                        ));
                    }
                }
                Some("known") => {
                    if b.cost_value.is_none() {
                        issues.push(issue(
                            IssueCode::InputIncomplete,
                            "budget.cost_value",
                            "known cost requires a value",
                        ));
                    }
                }
                Some(other) => issues.push(issue(
                    IssueCode::InvalidInput,
                    "budget.cost_status",
                    format!("unknown cost_status {other:?}"),
                )),
            }
        }
    }
    if let Some(src) = &request.source
        && SourceClass::parse(src).is_err()
    {
        issues.push(issue(
            IssueCode::InvalidInput,
            "source",
            format!("unknown source class {src:?}"),
        ));
    }

    for (prefix, locks) in [
        ("locked_components", &request.locked_components),
        ("context.declared_locks", &context.declared_locks),
    ] {
        for (i, lock) in locks.iter().enumerate() {
            if lock.declaration.as_deref() == Some("SEMANTIC") {
                for (field, value) in [
                    ("canonicalization_id", &lock.canonicalization_id),
                    ("canonicalization_version", &lock.canonicalization_version),
                ] {
                    if !nonblank(value) {
                        issues.push(issue(
                            IssueCode::InputIncomplete,
                            format!("{prefix}[{i}].{field}"),
                            "SEMANTIC requires a nonblank predeclared identity and version",
                        ));
                    }
                }
            }
        }
    }

    let mut requested_locks: Vec<(usize, ComponentRole, LockStatus)> = Vec::new();
    let mut seen_lock_roles: Vec<ComponentRole> = Vec::new();
    for (i, l) in request.locked_components.iter().enumerate() {
        let role = match l.role.as_deref().and_then(ComponentRole::parse) {
            Some(r) => r,
            None => {
                issues.push(issue(
                    IssueCode::InvalidInput,
                    format!("locked_components[{i}].role"),
                    "missing or unknown component role",
                ));
                continue;
            }
        };
        if seen_lock_roles.contains(&role) {
            issues.push(issue(
                IssueCode::InvalidInput,
                format!("locked_components[{i}].role"),
                "duplicate locked component",
            ));
            continue;
        }
        seen_lock_roles.push(role);
        let decl = match &l.declaration {
            Some(d) => match LockStatus::parse(d) {
                Ok(s) => s,
                Err(_) => {
                    issues.push(issue(
                        IssueCode::InvalidInput,
                        format!("locked_components[{i}].declaration"),
                        format!(
                            "unknown declaration {d:?}; expected BYTE, SEMANTIC or NOT_DEFINED"
                        ),
                    ));
                    continue;
                }
            },
            None => {
                issues.push(issue(
                    IssueCode::InputIncomplete,
                    format!("locked_components[{i}].declaration"),
                    "missing",
                ));
                continue;
            }
        };
        if decl == LockStatus::NotDefined {
            issues.push(issue(IssueCode::Unsupported, format!("locked_components[{i}].declaration"), "NOT_DEFINED is a declaration state, not a lock_mode; no semantic normalizer is implemented"));
            continue;
        }
        requested_locks.push((i, role, decl));
    }
    for (i, e) in request.editable_components.iter().enumerate() {
        if ComponentRole::parse(e).is_none() {
            issues.push(issue(
                IssueCode::InvalidInput,
                format!("editable_components[{i}]"),
                format!("unknown component role {e:?}"),
            ));
        }
    }

    for (i, c) in request.completion.iter().enumerate() {
        if !nonblank(&c.path) {
            issues.push(issue(
                IssueCode::InputIncomplete,
                format!("completion[{i}].path"),
                "missing or empty",
            ));
        }
        match &c.state {
            None => issues.push(issue(
                IssueCode::InputIncomplete,
                format!("completion[{i}].state"),
                "missing",
            )),
            Some(s) => match InferenceState::parse(s) {
                Some(InferenceState::PendingConfirmation) => {}
                Some(InferenceState::Observed) | Some(InferenceState::KnownCorrect) => {
                    issues.push(issue(
                        IssueCode::InvalidInput,
                        format!("completion[{i}].state"),
                        "completion/inference must not be promoted to observed/known_correct truth",
                    ))
                }
                None => issues.push(issue(
                    IssueCode::InvalidInput,
                    format!("completion[{i}].state"),
                    format!("unknown inference state {s:?}"),
                )),
            },
        }
        if let Some(src) = &c.source
            && SourceClass::parse(src).is_err()
        {
            issues.push(issue(
                IssueCode::InvalidInput,
                format!("completion[{i}].source"),
                format!("unknown source class {src:?}"),
            ));
        }
    }

    if !nonblank(&context.contract_id) {
        issues.push(issue(
            IssueCode::InputIncomplete,
            "context.contract_id",
            "missing or empty",
        ));
    }
    if context.declared_locks.is_empty() {
        issues.push(issue(
            IssueCode::InputIncomplete,
            "context.declared_locks",
            "pre-declared contract must declare locked scope",
        ));
    }
    let mut declared = Vec::new();
    let mut seen_declared_roles = Vec::new();
    let mut ambiguous_roles = Vec::new();
    for (i, l) in context.declared_locks.iter().enumerate() {
        let role = l.role.as_deref().and_then(ComponentRole::parse);
        if let Some(role) = role {
            if seen_declared_roles.contains(&role) {
                issues.push(issue(
                    IssueCode::InvalidInput,
                    format!("context.declared_locks[{i}].role"),
                    "duplicate locked component",
                ));
                ambiguous_roles.push(role);
            } else {
                seen_declared_roles.push(role);
            }
        }
        let decl = l
            .declaration
            .as_deref()
            .and_then(|d| LockStatus::parse(d).ok());
        match (role, decl) {
            (Some(r), Some(s)) => declared.push((r, s, l)),
            _ => issues.push(issue(
                IssueCode::InvalidInput,
                format!("context.declared_locks[{i}]"),
                "pre-declared lock must have a known role and BYTE/SEMANTIC declaration",
            )),
        }
    }

    if request.locked_components.is_empty() && !declared.is_empty() {
        issues.push(issue(IssueCode::LockViolation, "locked_components", "request has no locked_components; untrusted spec must not drop all pre-declared locks to gain authorization"));
    }
    for (role, decl, trusted) in &declared {
        if ambiguous_roles.contains(role) {
            continue;
        }
        match requested_locks.iter().find(|(_, r, _)| r == role) {
            None => issues.push(issue(
                IssueCode::LockViolation,
                "locked_components",
                format!(
                    "pre-declared lock for {} is omitted from request",
                    role.as_str()
                ),
            )),
            Some((i, _, requested)) if requested != decl => issues.push(issue(
                IssueCode::LockViolation,
                format!("locked_components[{i}].declaration"),
                "lock declaration differs from pre-declared contract",
            )),
            Some((i, _, _)) => {
                if trusted.declaration.as_deref() == Some("SEMANTIC") {
                    let requested = &request.locked_components[*i];
                    for (field, actual, expected) in [
                        (
                            "canonicalization_id",
                            &requested.canonicalization_id,
                            &trusted.canonicalization_id,
                        ),
                        (
                            "canonicalization_version",
                            &requested.canonicalization_version,
                            &trusted.canonicalization_version,
                        ),
                    ] {
                        if nonblank(actual) && nonblank(expected) && actual != expected {
                            issues.push(issue(
                                IssueCode::LockViolation,
                                format!("locked_components[{i}].{field}"),
                                "semantic identity differs from pre-declared contract",
                            ));
                        }
                    }
                }
            }
        }
    }
    let declared_roles: Vec<ComponentRole> = declared.iter().map(|(r, _, _)| *r).collect();
    let requested_lock_roles: Vec<ComponentRole> =
        requested_locks.iter().map(|(_, r, _)| *r).collect();
    for (i, e) in request.editable_components.iter().enumerate() {
        if let Some(role) = ComponentRole::parse(e) {
            if declared_roles.contains(&role) {
                issues.push(issue(
                    IssueCode::LockViolation,
                    format!("editable_components[{i}]"),
                    format!(
                        "editable component {} is locked by pre-declared contract",
                        role.as_str()
                    ),
                ));
            }
            if requested_lock_roles.contains(&role) {
                issues.push(issue(
                    IssueCode::LockViolation,
                    format!("editable_components[{i}]"),
                    format!("conflict: {} is both editable and locked", role.as_str()),
                ));
            }
        }
    }

    let source_class = request
        .source
        .as_deref()
        .and_then(|s| SourceClass::parse(s).ok())
        .unwrap_or(SourceClass::Unknown);
    let source = SourceDeclaration::declared(source_class);
    debug_assert!(!source.verified_by_checker());

    let status = if issues.is_empty() {
        STATUS_DECLARATION_CONSISTENT
    } else {
        STATUS_REJECTED
    };
    ValidationOutcome {
        status,
        issues,
        source,
        not_run: vec![
            "real_authorization_verification",
            "executor_admission",
            "F0_input_supply",
            "engine_execution",
            "semantic_normalizer",
            "visual_quality_check",
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeExample {
    pub id: &'static str,
    pub status: &'static str,
    pub note: &'static str,
    pub request: GenerationSpecRequest,
    pub context: TrustedContext,
}

#[must_use]
pub fn recipe_examples() -> Vec<RecipeExample> {
    let mut context = TrustedContext::from_contract(
        "AW-CONTRACT-001",
        &[
            (ComponentRole::Mesh, LockMode::Byte),
            (ComponentRole::Materials, LockMode::Byte),
            (ComponentRole::Rig, LockMode::Semantic),
        ],
    );
    context.declared_locks[2].canonicalization_id = Some("TEST_FIXTURE:rig".to_owned());
    context.declared_locks[2].canonicalization_version = Some("TEST_FIXTURE:v1".to_owned());
    let base = GenerationSpecRequest {
        base_version: Some("ver:fixture-001".to_owned()),
        input_refs: vec!["metadata://fixture/mesh.bin".to_owned()],
        references: vec![ReferenceUse {
            reference_id: Some("ref:front".to_owned()),
            purpose: Some("APPEARANCE".to_owned()),
            hidden_regions: vec![],
        }],
        editable_components: vec!["motion".to_owned()],
        locked_components: context.declared_locks.clone(),
        output_contract: Some("TEST_FIXTURE:metadata-only".to_owned()),
        budget: Some(Budget {
            max_calls: Some(0),
            cost_status: Some("unknown".to_owned()),
            cost_value: None,
        }),
        allowed_environment: Some("METADATA_ONLY_NO_EXECUTION".to_owned()),
        authorized: Some(true),
        source: Some("TEST_FIXTURE".to_owned()),
        declared_engine: Some("UNKNOWN".to_owned()),
        declared_executor: Some("UNKNOWN".to_owned()),
        completion: vec![],
    };
    let mut with_inference = base.clone();
    with_inference.completion = vec![CompletionClaim {
        path: Some("references[0].hidden_back".to_owned()),
        state: Some(STATE_PENDING_CONFIRMATION.to_owned()),
        source: Some("TEST_FIXTURE".to_owned()),
    }];
    vec![
        RecipeExample {
            id: "single_motion_increment",
            status: RECIPE_STATUS,
            note: "单次动作增量：仅 motion 可写，mesh/materials/rig 按预声明锁定；TEST_FIXTURE metadata-only",
            request: base,
            context: context.clone(),
        },
        RecipeExample {
            id: "hidden_back_pending_confirmation",
            status: RECIPE_STATUS,
            note: "单图隐藏背面只登记待确认推断，不称已证实真值；TEST_FIXTURE metadata-only",
            request: with_inference,
            context,
        },
    ]
}
