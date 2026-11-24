#![forbid(unsafe_code)]

pub mod application;
pub mod assurance;
pub mod budget;
pub mod candidate;
pub mod catalog;
pub mod consumption;

pub mod geometry;

pub mod library;
pub mod workflow;

pub mod storage;

pub mod generation_spec;

pub mod rig;

pub mod motion;

pub mod sqlite;

pub mod validation;

use std::fmt;

/// 合同字段 `locked_components[].lock_mode` 允许的锁定模式（docs/CONTRACTS.md §10.2）。
///
/// 合同表只定义 `BYTE` 与 `SEMANTIC` 两种模式；`NOT_DEFINED` 属于声明状态，见 [`LockStatus`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Byte,
    /// 语义级：比较预先声明的规范化摘要；规范化器未就绪时该模式不得被判为可测量。
    Semantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockStatus {
    Declared(LockMode),
    NotDefined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseContractValueError {
    expected: &'static str,
    input: String,
}

impl ParseContractValueError {
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

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

pub mod execution;
pub mod executor;

pub mod history;

pub mod backup;

pub mod executor_runtime;

pub mod production;
