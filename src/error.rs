use std::{error, fmt};

/// Result type used by jstream operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by jstream operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A serde_json parse, serialization, or deserialization error.
    Json(serde_json::Error),
    /// A value did not match the required example shape.
    Shape {
        /// Path to the value that failed validation.
        path: String,
        /// JSON kind required by the example shape.
        expected: JsonKind,
        /// Actual JSON kind, or `None` when the field was missing.
        actual: Option<JsonKind>,
    },
    /// Text or a template could not be processed by `parse_string`.
    ParseString {
        /// Category of parse failure.
        kind: ParseStringErrorKind,
        /// Byte position in the template or input, when applicable.
        position: Option<usize>,
    },
}

/// The JSON value categories used by shape validation errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum JsonKind {
    /// JSON `null`.
    Null,
    /// A JSON boolean.
    Bool,
    /// A JSON number.
    Number,
    /// A JSON string.
    String,
    /// A JSON array.
    Array,
    /// A JSON object.
    Object,
}

/// Categories of `parse_string` failure.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseStringErrorKind {
    /// A `{` in the template has no corresponding `}`.
    UnmatchedOpeningBrace,
    /// The template contains `{}`.
    EmptyPlaceholder,
    /// A JSON Pointer output path is empty.
    EmptyPointer,
    /// Literal template text could not be found in the input.
    ExpectedLiteral {
        /// Literal text that was expected.
        literal: String,
        /// Preceding field, when the literal delimits a captured field.
        after_field: Option<String>,
    },
    /// Input remained after the complete template matched.
    TrailingInput,
}

impl fmt::Display for JsonKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            JsonKind::Null => "null",
            JsonKind::Bool => "bool",
            JsonKind::Number => "number",
            JsonKind::String => "string",
            JsonKind::Array => "array",
            JsonKind::Object => "object",
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Json(error) => write!(f, "{error}"),
            Error::Shape {
                path,
                expected,
                actual: Some(actual),
            } => write!(f, "{path}: expected {expected}, found {actual}"),
            Error::Shape {
                path,
                expected: _,
                actual: None,
            } => write!(f, "{path}: missing required field"),
            Error::ParseString { kind, position } => {
                write!(f, "{kind}")?;
                if let Some(position) = position {
                    write!(f, " at byte {position}")?;
                }
                Ok(())
            }
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::Json(error) => Some(error),
            Error::Shape { .. } | Error::ParseString { .. } => None,
        }
    }
}

impl fmt::Display for ParseStringErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmatchedOpeningBrace => f.write_str("unmatched opening brace in template"),
            Self::EmptyPlaceholder => f.write_str("empty field placeholder in template"),
            Self::EmptyPointer => f.write_str("JSON Pointer field must not be empty"),
            Self::ExpectedLiteral {
                literal,
                after_field: Some(field),
            } => write!(f, "expected literal {literal:?} after field {field:?}"),
            Self::ExpectedLiteral {
                literal,
                after_field: None,
            } => write!(f, "expected literal {literal:?}"),
            Self::TrailingInput => f.write_str("unparsed trailing input"),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Json(error)
    }
}
