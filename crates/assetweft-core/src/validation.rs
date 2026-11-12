//! 职责: 独立于 SQLite/模型/Agent 的文件验证入口与最小 ValidationReport（AW-017:A）。
//! 依赖: gltf 1.4.1（成熟 glTF 2.0/GLB 解析器）、sha2 0.11.1（SHA-256）、std。
//! 调用方: 将来的 CLI/验证入口与 `tests/validation.rs`；本模块不写数据库、不联网、不执行文件内容。
//! 不变量: 只读调用者给定的单一文件；先查元数据大小再读、读取本身也带上限；
//!          解析失败或截断一律 FAIL，且角色合同结果为 NOT_RUN（不猜测）；
//!          可解析但不满足所选角色合同单独记为角色 FAIL，不把「能解析」当质量 PASS；
//!          未实现的数值/视觉/引擎/规范化器检查一律 NOT_RUN；
//!          来源标签只记录调用者声明，不从扩展名推断，也不当作已验证许可。

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

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

/// 读取/打开阶段的错误；解析失败不是这里的错误，而是报告中的 FAIL。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// 无法打开/读取文件。
    Io(String),
    /// 文件超过配置上限，未完整读取。
    InputTooLarge {
        /// 配置的上限。
        limit: u64,
        /// 元数据或已读字节给出的实际大小。
        actual: u64,
        /// 实际读取的字节数（超限时最多 limit+1）。
        bytes_read: usize,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(m) => write!(f, "io error: {m}"),
            Self::InputTooLarge {
                limit,
                actual,
                bytes_read,
            } => write!(
                f,
                "input too large: limit={limit} actual={actual} bytes_read={bytes_read}"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

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

/// 有界读取调用者给出的文件；超限时不完整读取。
///
/// # Errors
/// 打开/读取失败返回 `Io`；超过 `limits.max_bytes` 返回 `InputTooLarge`。
pub fn read_bounded(path: &Path, limits: ValidationLimits) -> Result<Vec<u8>, ValidationError> {
    let meta = fs::metadata(path).map_err(|e| ValidationError::Io(e.to_string()))?;
    if meta.len() > limits.max_bytes {
        return Err(ValidationError::InputTooLarge {
            limit: limits.max_bytes,
            actual: meta.len(),
            bytes_read: 0,
        });
    }
    let file = fs::File::open(path).map_err(|e| ValidationError::Io(e.to_string()))?;
    let mut buf = Vec::new();
    // u64::MAX 上限时饱和，避免 +1 溢出 panic（debug）或回绕（release）。
    file.take(limits.max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| ValidationError::Io(e.to_string()))?;
    if buf.len() as u64 > limits.max_bytes {
        return Err(ValidationError::InputTooLarge {
            limit: limits.max_bytes,
            actual: buf.len() as u64,
            bytes_read: buf.len(),
        });
    }
    Ok(buf)
}

/// 验证调用者给定的单个文件。
///
/// # Errors
/// 读取失败或超限时返回 [`ValidationError`]；文件格式失败体现在报告中。
pub fn validate_file(
    path: &Path,
    requirement: RoleRequirement,
    source: SourceDeclaration,
    limits: ValidationLimits,
) -> Result<ValidationReport, ValidationError> {
    let bytes = read_bounded(path, limits)?;
    Ok(validate_bytes(&bytes, requirement, source))
}

/// 结构化校验 GLB 2.0 二进制容器：12 字节头 + 分块表（magic/version/声明长度/chunk 边界与类型）。
///
/// 只做容器层检查，不解析 JSON 内容、不替代成熟结构解析器；用于阻止把 plain JSON 当 GLB。
fn check_glb_container(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 12 {
        return Err(format!("too short for a GLB header: {} bytes", bytes.len()));
    }
    if &bytes[0..4] != b"glTF" {
        return Err(
            "magic is not glTF; plain JSON or other formats are outside the declared GLB scope"
                .to_owned(),
        );
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 2 {
        return Err(format!(
            "unsupported GLB container version {version}; only 2 is in scope"
        ));
    }
    let declared = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if declared != bytes.len() {
        return Err(format!(
            "declared length {declared} != actual {} bytes",
            bytes.len()
        ));
    }
    let mut offset = 12usize;
    let mut first = true;
    while offset < bytes.len() {
        if offset + 8 > bytes.len() {
            return Err("truncated chunk header".to_owned());
        }
        let len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let ty = &bytes[offset + 4..offset + 8];
        if first && ty != b"JSON" {
            return Err("first GLB chunk must be JSON".to_owned());
        }
        first = false;
        if !len.is_multiple_of(4) {
            return Err(format!("chunk length {len} is not 4-byte aligned"));
        }
        let start = offset + 8;
        if start + len > bytes.len() {
            return Err("chunk length exceeds the file".to_owned());
        }
        offset = start + len;
    }
    Ok(())
}

/// 验证已读取的字节（不读文件、不联网）。
#[must_use]
pub fn validate_bytes(
    bytes: &[u8],
    requirement: RoleRequirement,
    source: SourceDeclaration,
) -> ValidationReport {
    let digest = Sha256::digest(bytes);
    // 只做十六进制展示（sha2 0.11 的摘要类型不再实现 LowerHex）；哈希算法仍由 sha2 提供。
    let file_sha256 = digest
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{byte:02x}");
            acc
        });
    let mut checks = Vec::new();

    if let Err(reason) = check_glb_container(bytes) {
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
        return ValidationReport {
            checker_version: CHECKER_VERSION,
            file_sha256,
            bytes_read: bytes.len(),
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
        };
    }

    match gltf::Gltf::from_slice(bytes) {
        Err(err) => {
            checks.push(CheckItem {
                id: "format.glb_container",
                status: CheckStatus::Fail,
                detail: format!("gltf parser rejected the GLB structure: {err}"),
            });
            checks.push(CheckItem {
                id: "role.contract",
                status: CheckStatus::NotRun,
                detail: "format parse failed; role contract not evaluated".to_owned(),
            });
            ValidationReport {
                checker_version: CHECKER_VERSION,
                file_sha256,
                bytes_read: bytes.len(),
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
        Ok(gltf) => {
            checks.push(CheckItem {
                id: "format.glb_container",
                status: CheckStatus::Pass,
                detail: "GLB 2.0 container accepted (magic/version/declared length/chunks), structure parsed by gltf 1.4.1".to_owned(),
            });
            let document = gltf.document;
            let mesh_count = document.meshes().count();
            let skin_count = document.skins().count();
            let animation_count = document.animations().count();
            // 角色关联：mesh node 必须实际引用 skin；动画必须命中该 rig 的 joint。
            let has_bound_skinned_mesh = document
                .nodes()
                .any(|n| n.mesh().is_some() && n.skin().is_some());
            // 只收集被 mesh node 实际引用的 skin 的 joint；未绑定 skin 的 joint 不参与动画判定。
            let bound_skins: HashSet<usize> = document
                .nodes()
                .filter(|n| n.mesh().is_some())
                .filter_map(|n| n.skin().map(|s| s.index()))
                .collect();
            let joints: HashSet<usize> = document
                .skins()
                .filter(|s| bound_skins.contains(&s.index()))
                .flat_map(|s| s.joints())
                .map(|n| n.index())
                .collect();
            let has_joint_animation = document.animations().any(|a| {
                a.channels()
                    .any(|c| joints.contains(&c.target().node().index()))
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
                bytes_read: bytes.len(),
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

/// 本卡明确未执行、也不得被当作 PASS 的检查项。
pub const NOT_RUN_ITEMS: &[&str] = &[
    "skin.weights_numeric",
    "rig.bind_matrices_numeric",
    "motion.semantic",
    "semantic.canonicalizer",
    "visual.quality",
    "engine.import",
];
