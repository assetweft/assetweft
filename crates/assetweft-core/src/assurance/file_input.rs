use super::{domain::ValidationLimits, ports::BoundedInput};
use std::{fmt, fs, io::Read, path::Path};

/// 读取/打开阶段的错误；解析失败不是这里的错误，而是报告中的 FAIL。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    Io(String),
    InputTooLarge {
        limit: u64,
        actual: u64,
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
    let not_regular = || ValidationError::Io("input must be a regular file".into());
    let metadata = fs::metadata(path).map_err(|e| ValidationError::Io(e.to_string()))?;
    if !metadata.is_file() {
        return Err(not_regular());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits().cast_signed());
    }
    #[cfg(any(
        target_vendor = "apple",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt;

        // Darwin and BSD fcntl.h define O_NONBLOCK as 0x0004 (#83).
        const O_NONBLOCK: i32 = 0x0004;
        options.custom_flags(O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|e| ValidationError::Io(e.to_string()))?;
    let meta = file
        .metadata()
        .map_err(|e| ValidationError::Io(e.to_string()))?;
    if !meta.is_file() {
        return Err(not_regular());
    }
    if meta.len() > limits.max_bytes {
        return Err(ValidationError::InputTooLarge {
            limit: limits.max_bytes,
            actual: meta.len(),
            bytes_read: 0,
        });
    }
    let mut buf = Vec::new();
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
