use std::fmt;

/// CCP custom `Result` type, using `Error` as the `Err` type.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
/// CCP custom error type.
pub struct Error(pub String);

impl<T: std::error::Error + std::fmt::Display> From<T> for Error {
    fn from(e: T) -> Error {
        Error(format!("portus err: {}", e))
    }
}

#[derive(Debug, Clone)]
pub struct StaleProgramError;
impl std::error::Error for StaleProgramError {
    fn description(&self) -> &str {
        "this report does not match the current scope"
    }
}
impl std::fmt::Display for StaleProgramError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "this report does not match the current scope")
    }
}
#[derive(Debug, Clone)]
pub struct InvalidRegTypeError;
impl std::error::Error for InvalidRegTypeError {
    fn description(&self) -> &str {
        "the requested field is not a report variable, and therefore cannot be accessed in ccp"
    }
}
impl std::fmt::Display for InvalidRegTypeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "the requested field is not a report variable, and therefore cannot be accessed in ccp"
        )
    }
}
#[derive(Debug, Clone)]
pub struct InvalidReportError;
impl std::error::Error for InvalidReportError {
    fn description(&self) -> &str {
        "the requested field is in scope but was not found in the report"
    }
}
impl std::fmt::Display for InvalidReportError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "the requested field is in scope but was not found in the report"
        )
    }
}
#[derive(Debug, Clone)]
pub struct FieldNotFoundError;
impl std::error::Error for FieldNotFoundError {
    fn description(&self) -> &str {
        "the requested field was not found in this scope"
    }
}
impl std::fmt::Display for FieldNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "the requested field was not found in this scope")
    }
}
