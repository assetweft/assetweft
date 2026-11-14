//! Responsibility: validation values, declarations and role/report policy.
//! Dependencies: std only.
//! Callers: Assurance use cases and legacy type reexports.
//! Invariants: format failure leaves role NOT_RUN; source is declaration only.

use std::fmt;

/// 本检查器版本（与 crate 版本一致）。
pub const CHECKER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 本卡声明的支持范围：只做 GLB 2.0 容器与结构解析，不解析外部 URI/图片。
pub const SUPPORTED_FORMAT: &str =
    "GLB 2.0 container via gltf 1.4.1; external URIs/images not resolved";

/// 调用者可配置的读取上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationLimits {
    /// 允许读取的最大字节数。
    pub max_bytes: u64,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
        }
    }
}

/// 来源分类（与 docs/CONTRACTS.md §6 的词表一致）。调用者声明，本模块不验证。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceClass {
    /// 合成测试样本。
    TestFixture,
    /// 程序化生成。
    Procedural,
    /// AI 生成（调用者声明）。
    AiGenerated,
    /// 人工编辑。
    ManuallyEdited,
    /// 未声明或未知。
    Unknown,
}

impl SourceClass {
    /// 合同词表文本。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TestFixture => "TEST_FIXTURE",
            Self::Procedural => "PROCEDURAL",
            Self::AiGenerated => "AI_GENERATED",
            Self::ManuallyEdited => "MANUALLY_EDITED",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// 逐字解析调用者声明；未知值报错，不默认成 UNKNOWN。
    ///
    /// # Errors
    /// 输入不是五个规范值之一时返回 [`UnknownSourceClass`]。
    pub fn parse(value: &str) -> Result<Self, UnknownSourceClass> {
        match value {
            "TEST_FIXTURE" => Ok(Self::TestFixture),
            "PROCEDURAL" => Ok(Self::Procedural),
            "AI_GENERATED" => Ok(Self::AiGenerated),
            "MANUALLY_EDITED" => Ok(Self::ManuallyEdited),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(UnknownSourceClass),
        }
    }
}

/// 来源分类不是合同词表值时返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownSourceClass;

impl fmt::Display for UnknownSourceClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown source class; expected TEST_FIXTURE, PROCEDURAL, AI_GENERATED, MANUALLY_EDITED or UNKNOWN")
    }
}

impl std::error::Error for UnknownSourceClass {}

/// 调用者声明的来源；`verified` 永远为 false —— 本模块不验证许可或生成方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDeclaration {
    /// 声明的分类。
    pub class: SourceClass,
    /// 调用者声明的许可文本（仅记录，不验证）。
    pub declared_license: Option<String>,
}

impl SourceDeclaration {
    /// 只记录声明，不推断、不验证。
    #[must_use]
    pub fn declared(class: SourceClass) -> Self {
        Self {
            class,
            declared_license: None,
        }
    }

    /// 声明是否经过本模块验证；恒为 false。
    #[must_use]
    pub const fn verified_by_checker(&self) -> bool {
        false
    }
}

/// 所选角色合同：决定缺哪些组件时算「角色不满足」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleRequirement {
    /// 只要求一个静态 mesh。
    StaticMesh,
    /// 要求 mesh + skin(rig)。
    RiggedCharacter,
    /// 要求 mesh + skin + animation。
    AnimatedCharacter,
}

impl RoleRequirement {
    /// 该角色合同要求的组件名。
    #[must_use]
    pub const fn required(self) -> &'static [&'static str] {
        match self {
            Self::StaticMesh => &["mesh"],
            Self::RiggedCharacter => &["mesh", "skin"],
            Self::AnimatedCharacter => &["mesh", "skin", "animation"],
        }
    }

    /// 合同词表文本。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaticMesh => "STATIC_MESH",
            Self::RiggedCharacter => "RIGGED_CHARACTER",
            Self::AnimatedCharacter => "ANIMATED_CHARACTER",
        }
    }
}

/// 单项检查状态。`NOT_RUN` 不是 `FAIL`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// 在声明范围内通过。
    Pass,
    /// 在声明范围内失败。
    Fail,
    /// 未执行/未实现；不得当作 PASS。
    NotRun,
}

impl CheckStatus {
    /// 合同词表文本。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotRun => "NOT_RUN",
        }
    }
}

/// 一条逐项检查结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckItem {
    /// 稳定的检查 ID。
    pub id: &'static str,
    /// 状态。
    pub status: CheckStatus,
    /// 人类可读细节（不含推断值）。
    pub detail: String,
}

/// 角色合同结果：与格式结果分开报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleResult {
    /// 所选角色合同。
    pub requirement: RoleRequirement,
    /// 是否满足；格式不可解析时为 None。
    pub satisfied: Option<bool>,
    /// 实际存在的组件。
    pub present: Vec<&'static str>,
    /// 缺失的必需组件。
    pub missing: Vec<&'static str>,
    /// 该结果自身的状态。
    pub status: CheckStatus,
}

/// 最小 ValidationReport（docs/CONTRACTS.md §8）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// 检查器版本。
    pub checker_version: &'static str,
    /// 实际读取字节的 SHA-256。
    pub file_sha256: String,
    /// 实际读取的字节数。
    pub bytes_read: usize,
    /// 支持范围声明。
    pub format: &'static str,
    /// 格式/结构解析状态。
    pub format_status: CheckStatus,
    /// 角色合同结果（与格式分开）。
    pub role: RoleResult,
    /// 全部逐项结果。
    pub checks: Vec<CheckItem>,
    /// 未执行项（显式）。
    pub not_run: Vec<&'static str>,
    /// 来源声明（未验证）。
    pub source: SourceDeclaration,
    /// 单位/坐标声明：输入未声明时记 UNKNOWN，不从字节推断。
    pub units: &'static str,
}

/// 本卡明确未执行、也不得被当作 PASS 的检查项。
pub const NOT_RUN_ITEMS: &[&str] = &[
    "skin.weights_numeric",
    "rig.bind_matrices_numeric",
    "motion.semantic",
    "semantic.canonicalizer",
    "visual.quality",
    "engine.import",
];

/// Parsed facts contain no parser SDK values and confer no quality or rights claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComponentFacts {
    pub mesh_count: usize,
    pub skin_count: usize,
    pub animation_count: usize,
    pub has_bound_skinned_mesh: bool,
    pub has_joint_animation: bool,
}

/// Evidence about the exact input bytes, with parsing failure kept distinct.
pub(crate) struct CheckedBytes {
    pub file_sha256: String,
    pub bytes_read: usize,
    pub structure: Result<ComponentFacts, String>,
}

pub(crate) fn report(
    checked: CheckedBytes,
    requirement: RoleRequirement,
    source: SourceDeclaration,
) -> ValidationReport {
    let CheckedBytes {
        file_sha256,
        bytes_read,
        structure,
    } = checked;
    let mut checks = Vec::new();
    match structure {
        Err(reason) => {
            checks.push(CheckItem {
                id: "format.glb_container",
                status: CheckStatus::Fail,
                detail: reason,
            });
            checks.push(CheckItem {
                id: "role.contract",
                status: CheckStatus::NotRun,
                detail: "format parse failed; role contract not evaluated".to_owned(),
            });
            ValidationReport {
                checker_version: CHECKER_VERSION,
                file_sha256,
                bytes_read,
                format: SUPPORTED_FORMAT,
                format_status: CheckStatus::Fail,
                role: RoleResult {
                    requirement,
                    satisfied: None,
                    present: Vec::new(),
                    missing: Vec::new(),
                    status: CheckStatus::NotRun,
                },
                checks,
                not_run: NOT_RUN_ITEMS.to_vec(),
                source,
                units: "UNKNOWN: not declared by input; not inferred from bytes",
            }
        }
        Ok(ComponentFacts {
            mesh_count,
            skin_count,
            animation_count,
            has_bound_skinned_mesh,
            has_joint_animation,
        }) => {
            checks.push(CheckItem {
                id: "format.glb_container",
                status: CheckStatus::Pass,
                detail: "GLB 2.0 container accepted (magic/version/declared length/chunks), structure parsed by gltf 1.4.1".to_owned(),
            });
            let mut present = Vec::new();
            let components = [
                (
                    "mesh",
                    mesh_count > 0,
                    "components.mesh",
                    format!("{mesh_count} mesh(es) parsed"),
                ),
                (
                    "skin",
                    skin_count > 0 && has_bound_skinned_mesh,
                    "components.skin",
                    if skin_count > 0 && !has_bound_skinned_mesh {
                        format!(
                            "{skin_count} skin object(s) exist but no mesh node references a skin; an unbound skin is not a rig"
                        )
                    } else {
                        format!("{skin_count} skin(s), with at least one mesh node bound to a skin")
                    },
                ),
                (
                    "animation",
                    animation_count > 0 && has_joint_animation,
                    "components.animation",
                    if animation_count > 0 && !has_joint_animation {
                        format!(
                            "{animation_count} animation object(s) exist but none targets a skin joint; orphan animation is not motion for this rig"
                        )
                    } else {
                        format!(
                            "{animation_count} animation(s), with at least one channel targeting a skin joint"
                        )
                    },
                ),
            ];
            for (name, ok, id, detail) in components {
                if ok {
                    present.push(name);
                    checks.push(CheckItem {
                        id,
                        status: CheckStatus::Pass,
                        detail,
                    });
                } else {
                    checks.push(CheckItem {
                        id,
                        status: CheckStatus::NotRun,
                        detail,
                    });
                }
            }
            let missing: Vec<&'static str> = requirement
                .required()
                .iter()
                .copied()
                .filter(|r| !present.contains(r))
                .collect();
            let role_status = if missing.is_empty() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };
            checks.push(CheckItem {
                id: "role.contract",
                status: role_status,
                detail: if missing.is_empty() {
                    format!(
                        "role {} satisfied by bound components",
                        requirement.as_str()
                    )
                } else {
                    format!(
                        "role {} missing required component(s): {}",
                        requirement.as_str(),
                        missing.join(", ")
                    )
                },
            });
            ValidationReport {
                checker_version: CHECKER_VERSION,
                file_sha256,
                bytes_read,
                format: SUPPORTED_FORMAT,
                format_status: CheckStatus::Pass,
                role: RoleResult {
                    requirement,
                    satisfied: Some(missing.is_empty()),
                    present,
                    missing,
                    status: role_status,
                },
                checks,
                not_run: NOT_RUN_ITEMS.to_vec(),
                source,
                units: "UNKNOWN: not declared by input; not inferred from bytes",
            }
        }
    }
}
