//! Responsibility: concrete bounded filesystem reading and original errors.
//! Dependencies: std fs/io and Assurance BoundedInput.
//! Callers: legacy validation composition and read_bounded consumers.
//! Invariants: metadata bound before open; bounded read with saturating limit+1.

use super::{domain::ValidationLimits, ports::BoundedInput};
use std::{fmt, fs, io::Read, path::Path};

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

pub(crate) struct FileInput<'a>(pub &'a Path);

impl BoundedInput for FileInput<'_> {
    type Error = ValidationError;
    fn read(&self, limits: ValidationLimits) -> Result<Vec<u8>, Self::Error> {
        read_bounded(self.0, limits)
    }
}
