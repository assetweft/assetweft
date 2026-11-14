//! 职责: AW-013:A「可检查的用户需求」。给 UI/Agent 同一份 `GenerationSpecRequest` 数据规格与
//!        唯一可调用验证入口 `validate(request, context)`；只做声明一致性对照，外加少量线性
//!        `recipe_examples()`（明确 NOT_EXECUTED）。不是 CLI 包装、节点/DAG 平台、调度器或 LLM 解析器。
//! 依赖: std、serde 1.0.229 / serde_json 1.0.154（已锁的成熟 wire 解析，不是第二套验证引擎）；
//!        复用 crate 既有 `LockMode`/`LockStatus`（lib.rs）与 `SourceClass`/`SourceDeclaration`（Assurance）。
//! 调用方: CLI/桌面/Agent 共用此类型与 `validate`；本卡由 `tests/generation_spec.rs` 与 C1 调用。
//! 不变量:
//!  1. 请求来自不可信方；锁定范围以**调用方预声明**的 `TrustedContext.declared_locks` 为准，
//!     请求删空/省略/篡改/冲突 locked 范围一律 `LOCK_VIOLATION` 并附明确原因，绝不自删锁获得授权。
//!  2. 缺字段/非法声明返回稳定可定位的 `field_path`（如 `base_version`、`references[0].purpose`）。
//!  3. 隐藏/补全/推断只许 `PENDING_CONFIRMATION`；`OBSERVED`/`KNOWN_CORRECT` 一律拒绝，不自动晋升真值。
//!  4. `authorized=true`、engine/executor 等只是**声明**，本模块不验证真实素材许可或执行器准入；
//!     `DeclarationConsistent` 不等于执行 PASS，不提供任何 `can_execute` 就绪主张。
//!  5. unknown 费用允许 null，不自动填 0；`NOT_DEFINED` 是声明状态不是 lock_mode；
//!     组件词表沿用现行 mesh/materials/rig/motion…（回指 AW-003 schema `definitions.role` 的真实消费者），
//!     不把 GLB 物理 skin/animation 计数改名成厂商 schema，不造语义规范化器。
//!  6. 不修改既有 first-delivery schema/checker/fixtures；不引入第二套通用 JSON Schema 引擎或漂移常量。

use serde::Deserialize;

use crate::assurance::{SourceClass, SourceDeclaration};
use crate::{LockMode, LockStatus};

/// 本验证器版本（与 crate 版本一致）；报告只表示声明一致性，不表示能力验收。
pub const VALIDATOR_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 配方样例状态：只描述规格，**从未执行**。
pub const RECIPE_STATUS: &str = "NOT_EXECUTED";
/// 声明一致（仅在声明范围内；不是真实执行 PASS）。
pub const STATUS_DECLARATION_CONSISTENT: &str = "DECLARATION_CONSISTENT";
/// 被拒绝。
pub const STATUS_REJECTED: &str = "REJECTED";
/// 允许的推断/补全状态（唯一）。
pub const STATE_PENDING_CONFIRMATION: &str = "PENDING_CONFIRMATION";

/// 组件角色词表：与 `contracts/first-delivery-contract.schema.json` `definitions.role` 的 wire 值一致。
/// 真实消费者是 AW-003 checker；此处是该词表在 Rust 类型化执行路径的表示，不重写 JSON Schema。
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
    /// 合同 wire 文本。
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

    /// 逐字解析；未知值返回 None，由调用方给出带字段路径的错误。
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
    /// 合同 wire 文本。
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

    /// 逐字解析；未知值返回 None。
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
    /// 明确待确认（允许存在于正例，但绝不等于已证实真值）。
    PendingConfirmation,
    /// 声称已观察 —— 本模块拒绝。
    Observed,
    /// 声称已知正确 —— 本模块拒绝。
    KnownCorrect,
}

impl InferenceState {
    /// 合同 wire 文本。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PendingConfirmation => "PENDING_CONFIRMATION",
            Self::Observed => "OBSERVED",
            Self::KnownCorrect => "KNOWN_CORRECT",
        }
    }

    /// 逐字解析；未知值返回 None。
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
    /// 2 INVALID_INPUT：非法数值/声明不一致。
    InvalidInput,
    /// 3 INPUT_INCOMPLETE：缺字段。
    InputIncomplete,
    /// 4 LOCK_VIOLATION：锁定范围被绕过/省略/篡改/冲突。
    LockViolation,
    /// 1 FAIL：其它合同违反。
    Fail,
    /// 5 UNSUPPORTED：能力不足（规范化器未实现等）。
    Unsupported,
}

impl IssueCode {
    /// 稳定类别名。
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

/// 一条可定位的问题：稳定字段路径 + 明确原因。
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
    /// 来源声明（调用方声明，本模块恒不验证）。
    pub source: SourceDeclaration,
    /// 显式未执行项（逐项列出，不冒充 PASS）。
    pub not_run: Vec<&'static str>,
}

impl ValidationOutcome {
    /// 是否在声明范围内一致（不是执行 PASS）。
    #[must_use]
    pub fn is_declaration_consistent(&self) -> bool {
        self.status == STATUS_DECLARATION_CONSISTENT
    }

    /// 是否含有指定类别的问题。
    #[must_use]
    pub fn has_code(&self, code: IssueCode) -> bool {
        self.issues.iter().any(|i| i.code == code)
    }

    /// 是否含有指定字段路径的问题。
    #[must_use]
    pub fn has_field_path(&self, path: &str) -> bool {
        self.issues.iter().any(|i| i.field_path == path)
    }
}

/// 参考素材用途声明（§3）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceUse {
    #[serde(default)]
    pub reference_id: Option<String>,
    #[serde(default)]
    pub purpose: Option<String>,
    /// 单图隐藏区域：只登记位置，不推断内容。
    #[serde(default)]
    pub hidden_regions: Vec<String>,
}

/// 锁定声明；`declaration` 取值 BYTE/SEMANTIC/NOT_DEFINED，复用 `LockStatus::parse`。
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

/// 隐藏结构补全/推断；`state` 只允许 PENDING_CONFIRMATION。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionClaim {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    /// 来源分类声明（复用 validation::SourceClass 词表，不验证真伪）。
    #[serde(default)]
    pub source: Option<String>,
}

/// UI/Agent 提交的同一份用户规格（不可信输入）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationSpecRequest {
    /// 基础版本（S3 首条动作增量必需）。
    #[serde(default)]
    pub base_version: Option<String>,
    /// 输入引用（S3 首条动作增量必需）。
    #[serde(default)]
    pub input_refs: Vec<String>,
    /// 参考素材与用途（§3）。
    #[serde(default)]
    pub references: Vec<ReferenceUse>,
    /// 请求可写组件。
    #[serde(default)]
    pub editable_components: Vec<String>,
    /// 请求锁定组件；必须与预声明合同对照，不能自删。
    #[serde(default)]
    pub locked_components: Vec<LockClaim>,
    /// 输出合同。
    #[serde(default)]
    pub output_contract: Option<String>,
    /// 预算。
    #[serde(default)]
    pub budget: Option<Budget>,
    /// 允许执行环境。
    #[serde(default)]
    pub allowed_environment: Option<String>,
    /// 调用方声称的授权（只记录，**不验证**）。
    #[serde(default)]
    pub authorized: Option<bool>,
    /// 来源分类声明（复用 SourceClass 词表，不验证）。
    #[serde(default)]
    pub source: Option<String>,
    /// 引擎/执行器只是声明，不自动变真实运行 PASS。
    #[serde(default)]
    pub declared_engine: Option<String>,
    #[serde(default)]
    pub declared_executor: Option<String>,
    /// 隐藏/补全/推断，禁止晋升真值。
    #[serde(default)]
    pub completion: Vec<CompletionClaim>,
}

/// 调用方可信上下文：来自**预声明合同**，不是从请求里读。
///
/// 来源与未验证边界：由调用方（CLI/UI/Agent 上游）从已冻结合同提供；本模块只用它对照请求范围，
/// 不验证该合同本身的真实素材许可、执行器准入或来源真伪，也不因此声称 F0 已提供。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedContext {
    #[serde(default)]
    pub contract_id: Option<String>,
    /// 预声明锁定范围（权威）。正常正例锁 mesh/materials/rig、允许 motion。
    #[serde(default)]
    pub declared_locks: Vec<LockClaim>,
    /// 调用方对上下文来源的说明（只记录）。
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

/// 唯一可调用验证入口：UI/Agent 提交同一 `GenerationSpecRequest` + 同一 `TrustedContext`。
///
/// 返回 `ValidationOutcome`；`DeclarationConsistent` **仅**表示声明一致性，不表示真实执行 PASS，
/// 不授予任何权限。所有问题都带稳定 `field_path` 与明确原因。
#[must_use]
pub fn validate(request: &GenerationSpecRequest, context: &TrustedContext) -> ValidationOutcome {
    let mut issues: Vec<SpecIssue> = Vec::new();

    // --- 1) 缺字段 / 非法声明：稳定可定位的字段路径 ---
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

    // Required identity is checked independently on each wire side, preserving indices.
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

    // --- 2) 组件词表与锁定声明解析 ---
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
        // NOT_DEFINED 是声明状态，不是锁定模式；不得用来声明一个锁。
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

    // --- 3) 隐藏结构补全与真值分开 ---
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

    // --- 4) 可信上下文自身完整性 ---
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
    let mut declared: Vec<(ComponentRole, LockStatus)> = Vec::new();
    for (i, l) in context.declared_locks.iter().enumerate() {
        let role = l.role.as_deref().and_then(ComponentRole::parse);
        let decl = l
            .declaration
            .as_deref()
            .and_then(|d| LockStatus::parse(d).ok());
        match (role, decl) {
            (Some(r), Some(s)) => declared.push((r, s)),
            _ => issues.push(issue(
                IssueCode::InvalidInput,
                format!("context.declared_locks[{i}]"),
                "pre-declared lock must have a known role and BYTE/SEMANTIC declaration",
            )),
        }
    }

    // --- 5) 锁定范围对照：请求不能删空/省略/篡改/冲突预声明锁 ---
    if request.locked_components.is_empty() && !declared.is_empty() {
        issues.push(issue(IssueCode::LockViolation, "locked_components", "request has no locked_components; untrusted spec must not drop all pre-declared locks to gain authorization"));
    }
    for (role, decl) in &declared {
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
                if context.declared_locks.iter().any(|l| {
                    l.role.as_deref() == Some(role.as_str())
                        && l.declaration.as_deref() == Some("SEMANTIC")
                }) {
                    let trusted = context
                        .declared_locks
                        .iter()
                        .find(|l| l.role.as_deref() == Some(role.as_str()))
                        .expect("declared role exists");
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
    let declared_roles: Vec<ComponentRole> = declared.iter().map(|(r, _)| *r).collect();
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

    // --- 6) 声明但不验证：来源 / 授权 / 引擎 / 执行器 ---
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

/// 线性配方样例：只描述规格，**从未执行**（不造节点/DAG 平台）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeExample {
    pub id: &'static str,
    pub status: &'static str,
    pub note: &'static str,
    pub request: GenerationSpecRequest,
    pub context: TrustedContext,
}

/// 少量线性 recipe examples；全部 TEST_FIXTURE / metadata-only / NOT_EXECUTED。
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
