pub type LocalError = crate::error::Error;
pub type LocalResult<T> = std::result::Result<T, LocalError>;

#[derive(Debug, Clone)]
pub enum Error {
    PermissionDenied {
        permission: String,
        org_unit: Option<i32>,
    },
    InvalidInput(String),
    NotFound(String),
    Internal(String),
    JsonParse(String),
    Unauthenticated,
    Conflict {
        code: &'static str,
        field: Option<String>,
        message: String,
    },
}

impl Error {
    pub fn permission_denied(
        permission: impl Into<String>,
        org_unit: impl Into<Option<i32>>,
    ) -> Self {
        Error::PermissionDenied {
            permission: permission.into(),
            org_unit: org_unit.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Error::NotFound(message.into())
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Error::InvalidInput(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Error::Internal(message.into())
    }

    pub fn unauthenticated() -> Self {
        Error::Unauthenticated
    }

    pub fn conflict(code: &'static str, field: Option<&str>, message: impl Into<String>) -> Self {
        Error::Conflict {
            code,
            field: field.map(String::from),
            message: message.into(),
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Error::PermissionDenied { .. } => "PERMISSION_DENIED",
            Error::NotFound(_) => "NOT_FOUND",
            Error::InvalidInput(_) => "INVALID_INPUT",
            Error::Internal(_) => "INTERNAL_ERROR",
            Error::Unauthenticated => "UNAUTHENTICATED",
            Error::JsonParse(_) => "JSON_PARSE_ERROR",
            Error::Conflict { code, .. } => code,
        }
    }

    pub fn field(&self) -> Option<&str> {
        match self {
            Error::Conflict { field, .. } => field.as_deref(),
            _ => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Error::PermissionDenied {
                permission,
                org_unit,
            } => {
                if let Some(org_unit_id) = org_unit {
                    format!(
                        "Permission denied: '{}' at org unit {}",
                        permission, org_unit_id
                    )
                } else {
                    format!("Permission denied: '{}'", permission)
                }
            }
            Error::NotFound(msg) => format!("{msg} not found"),
            Error::InvalidInput(msg) => format!("Invalid input: {msg}"),
            Error::Internal(msg) => format!("Internal error: {msg}"),
            Error::JsonParse(msg) => format!("Error parsing JSON: {msg}"),
            Error::Unauthenticated => "Missing or invalid authorization".to_string(),
            Error::Conflict { message, .. } => message.clone(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::JsonParse(e.to_string())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Error::internal(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Error::internal(message)
    }
}

#[cfg(feature = "seaorm")]
impl From<sea_orm::DbErr> for Error {
    fn from(e: sea_orm::DbErr) -> Self {
        Error::internal(e.to_string())
    }
}

// -- Axum integration: shared ApiError for all odo HTTP services ----------

#[derive(Debug)]
pub struct ApiError(pub Error);
pub type ApiResult<T> = std::result::Result<T, ApiError>;

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        ApiError(e)
    }
}

#[cfg(feature = "seaorm")]
impl From<sea_orm::DbErr> for ApiError {
    fn from(e: sea_orm::DbErr) -> Self {
        ApiError(Error::internal(e.to_string()))
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError(Error::from(e))
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use axum::Json;
        use axum::http::StatusCode;

        let status = match &self.0 {
            Error::Unauthenticated => StatusCode::UNAUTHORIZED,
            Error::PermissionDenied { .. } => StatusCode::FORBIDDEN,
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Error::Conflict { .. } => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let mut body = serde_json::json!({
            "code": self.0.code(),
            "message": self.0.message(),
        });

        if let Some(field) = self.0.field() {
            body["field"] = serde_json::Value::String(field.to_string());
        }

        (status, Json(body)).into_response()
    }
}
