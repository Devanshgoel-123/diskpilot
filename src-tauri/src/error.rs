use serde::Serialize;

use crate::safety::SafetyError;

/// Error returned by every Tauri command. Serialized as `{ kind, message }`
/// so the frontend can show a meaningful message instead of a stack trace.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Safety(#[from] SafetyError),
    #[error("A scan is already running")]
    ScanInProgress,
    #[error("No completed scan yet. Run a scan first.")]
    NoScan,
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    #[error("History database error: {0}")]
    History(String),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn io(path: impl AsRef<std::path::Path>, err: &std::io::Error) -> Self {
        AppError::Io {
            path: path.as_ref().display().to_string(),
            message: describe_io_error(err),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            AppError::Safety(_) => "safety",
            AppError::ScanInProgress => "scanInProgress",
            AppError::NoScan => "noScan",
            AppError::Io { .. } => "io",
            AppError::History(_) => "history",
            AppError::Other(_) => "other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::History(e.to_string())
    }
}

/// Human-friendly description of an I/O error, with a hint for the common
/// macOS privacy (TCC) case.
pub fn describe_io_error(err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => {
            "Permission denied (macOS privacy protection or file permissions)".to_string()
        }
        std::io::ErrorKind::NotFound => "No longer exists".to_string(),
        _ => err.to_string(),
    }
}

pub type AppResult<T> = Result<T, AppError>;
