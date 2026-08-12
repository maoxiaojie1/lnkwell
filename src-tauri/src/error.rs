use serde::Serialize;

/// 统一错误类型（方案 §9.5）：序列化为带 code 的结构，前端按 code 分支给中文提示。
/// DecodeFailed / Db 是为 M6 搜索与编码细化预留的契约，当前尚未触发。
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum AppError {
    #[error("permission expired")]
    PermissionExpired,
    #[error("file not found")]
    NotFound,
    #[error("file too large: {0} bytes")]
    TooLarge(u64),
    #[error("decode failed")]
    DecodeFailed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("db: {0}")]
    Db(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

impl AppError {
    fn code(&self) -> &'static str {
        match self {
            Self::PermissionExpired => "PERMISSION_EXPIRED",
            Self::NotFound => "NOT_FOUND",
            Self::TooLarge(_) => "TOO_LARGE",
            Self::DecodeFailed => "DECODE_FAILED",
            Self::Io(_) => "IO",
            Self::Db(_) => "DB",
            Self::InvalidInput(_) => "INVALID_INPUT",
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
