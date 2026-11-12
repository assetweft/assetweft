//! 职责: 共享应用库入口；导出合同词表、应用与本地资产库、验证和几何检查模块。
//! 依赖: 词表使用 std；子模块分别使用 rusqlite、gltf 与 sha2。
//! 调用方: 将来的 CLI、桌面与验证入口；当前由本模块单元测试和 C1 调用。
//! 不变量: 未知合同值必须返回错误，不得静默默认；`NOT_DEFINED` 是"未定义摘要"的声明状态，
//!          不是第三种 lock_mode；大小写与空白不做宽松归一化（合同字段值必须逐字匹配）。

#![forbid(unsafe_code)]

/// AW-008:A shared application lifecycle and bounded validation entry.
pub mod application;

/// AW-022:A 静态几何引用检查；不解析外部资源。
pub mod geometry;

/// AW-009:A typed local library metadata and migration boundary.
pub mod library;

/// AW-004:A SQLite 文件持久化原型；不依赖验证模块或模型执行。
pub mod sqlite;

/// AW-017:A 独立文件验证入口与最小 ValidationReport；不依赖 SQLite/模型/Agent。
pub mod validation;

use std::fmt;

/// 合同字段 `locked_components[].lock_mode` 允许的锁定模式（docs/CONTRACTS.md §10.2）。
///
/// 合同表只定义 `BYTE` 与 `SEMANTIC` 两种模式；`NOT_DEFINED` 属于声明状态，见 [`LockStatus`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// 字节级：比较文件字节的 SHA-256。
    Byte,
    /// 语义级：比较预先声明的规范化摘要；规范化器未就绪时该模式不得被判为可测量。
    Semantic,
}

/// 合同里的锁定声明状态：要么声明了模式，要么明确未定义摘要。
///
/// 这里只做词表映射；"是否已就绪/可测量"属于执行期或规范化器合同，本模块不提供该主张。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockStatus {
    /// 已声明一种锁定模式。
    Declared(LockMode),
    /// 该组件在本合同版本中未定义摘要。
    NotDefined,
}

/// 解析合同词表值失败时返回；携带字段名与原始输入以便定位笔误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseContractValueError {
    expected: &'static str,
    input: String,
}

impl ParseContractValueError {
    /// 返回触发错误的原始字段值。
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// 返回该字段允许的值说明。
    #[must_use]
    pub fn expected(&self) -> &'static str {
        self.expected
    }
}

impl fmt::Display for ParseContractValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown value {:?}; expected {}",
            self.input, self.expected
        )
    }
}

impl std::error::Error for ParseContractValueError {}

impl LockMode {
    /// 模式在合同中的规范文本形式。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Byte => "BYTE",
            Self::Semantic => "SEMANTIC",
        }
    }

    /// 逐字解析 `lock_mode`；不做 trim、大小写折叠或别名。
    ///
    /// # Errors
    /// 输入不是 `BYTE` 或 `SEMANTIC`（包括把 `NOT_DEFINED` 当模式）时返回错误。
    pub fn parse(value: &str) -> Result<Self, ParseContractValueError> {
        match value {
            "BYTE" => Ok(Self::Byte),
            "SEMANTIC" => Ok(Self::Semantic),
            other => Err(ParseContractValueError {
                expected: "BYTE or SEMANTIC",
                input: other.to_owned(),
            }),
        }
    }
}

impl LockStatus {
    /// 声明状态在合同中的规范文本形式。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declared(mode) => mode.as_str(),
            Self::NotDefined => "NOT_DEFINED",
        }
    }

    /// 逐字解析锁定声明字段。
    ///
    /// # Errors
    /// 输入不是 `BYTE`、`SEMANTIC` 或 `NOT_DEFINED` 时返回错误。
    pub fn parse(value: &str) -> Result<Self, ParseContractValueError> {
        match value {
            "NOT_DEFINED" => Ok(Self::NotDefined),
            other => {
                LockMode::parse(other)
                    .map(Self::Declared)
                    .map_err(|_| ParseContractValueError {
                        expected: "BYTE, SEMANTIC or NOT_DEFINED",
                        input: other.to_owned(),
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LockMode, LockStatus};

    // 正例直接使用合同的独立 wire 字面量，避免 parse/as_str 互相自圆。
    #[test]
    fn lock_mode_accepts_the_two_contract_wire_values() {
        assert_eq!(LockMode::parse("BYTE"), Ok(LockMode::Byte));
        assert_eq!(LockMode::parse("SEMANTIC"), Ok(LockMode::Semantic));
        assert_eq!(LockMode::Byte.as_str(), "BYTE");
        assert_eq!(LockMode::Semantic.as_str(), "SEMANTIC");
    }

    #[test]
    fn lock_mode_rejects_definition_state_and_unknown_values() {
        for bad in [
            "NOT_DEFINED",
            "byte",
            "Semantic",
            " BYTE",
            "BYTE ",
            "",
            "NOT_RUN",
        ] {
            let err = LockMode::parse(bad).expect_err("must reject non-canonical lock_mode");
            assert_eq!(err.input(), bad);
            assert_eq!(err.expected(), "BYTE or SEMANTIC");
        }
    }

    #[test]
    fn lock_status_separates_definition_state_from_mode() {
        assert_eq!(
            LockStatus::parse("BYTE"),
            Ok(LockStatus::Declared(LockMode::Byte))
        );
        assert_eq!(
            LockStatus::parse("SEMANTIC"),
            Ok(LockStatus::Declared(LockMode::Semantic))
        );
        assert_eq!(LockStatus::parse("NOT_DEFINED"), Ok(LockStatus::NotDefined));
        assert_eq!(LockStatus::NotDefined.as_str(), "NOT_DEFINED");
    }

    #[test]
    fn lock_status_rejects_unknown_values() {
        for bad in ["not_defined", "NOT DEFINED", "", "NOT_RUN"] {
            let err = LockStatus::parse(bad).expect_err("must reject non-canonical status");
            assert_eq!(err.input(), bad);
            assert_eq!(err.expected(), "BYTE, SEMANTIC or NOT_DEFINED");
        }
    }
}
