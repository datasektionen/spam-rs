use aws_sdk_sesv2 as sesv2;
use log::error;
use std::fmt::Display;

use actix_web::{HttpResponse, ResponseError, http::StatusCode};

#[derive(Debug)]
pub enum Error {
    EnvVarMissing(String),
    InvalidEmailDomain(String),
    InvalidContentType,
    ApiKeyInvalid,
    ApiKeyLookup(String),
    MissingContent,
    EmailSend(String),
    TemplateRender(String),
    TemplateLoad(String),
    Attachment(String),
    NotASCII(String),
    EmailBody(String),
}

impl From<sesv2::Error> for Error {
    fn from(err: sesv2::Error) -> Self {
        Error::EmailSend(err.to_string())
    }
}

impl From<handlebars::RenderError> for Error {
    fn from(err: handlebars::RenderError) -> Self {
        Error::TemplateRender(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::TemplateLoad(err.to_string())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EnvVarMissing(msg) => write!(f, "Environment variable missing: {}", msg),
            Error::ApiKeyInvalid => write!(f, "API key is invalid or lacks permissions"),
            Error::ApiKeyLookup(msg) => write!(f, "API lookup failed: {}", msg),
            Error::InvalidEmailDomain(domain) => write!(f, "Invalid email domain: {}", domain),
            Error::InvalidContentType => write!(f, "Invalid content type"),
            Error::EmailSend(msg) => write!(f, "Failed to send email: {}", msg),
            Error::TemplateRender(msg) => write!(f, "Failed to render template: {}", msg),
            Error::TemplateLoad(msg) => write!(f, "Failed to load template: {}", msg),
            Error::Attachment(msg) => write!(f, "Failed to process attachment: {}", msg),
            Error::EmailBody(msg) => write!(f, "Failed to process email body: {}", msg),
            Error::NotASCII(field) => write!(f, "Contains non-ASCII characters: {}", field),
            Error::MissingContent => write!(f, "No 'html' or 'content' field provided."),
        }
    }
}

impl From<&Error> for HttpResponse {
    fn from(val: &Error) -> Self {
        match val {
            Error::ApiKeyInvalid => HttpResponse::Unauthorized().body(val.to_string()),
            Error::EmailSend(_)
            | Error::TemplateRender(_)
            | Error::TemplateLoad(_)
            | Error::ApiKeyLookup(_)
            | Error::EnvVarMissing(_) => HttpResponse::InternalServerError().body(val.to_string()),
            Error::Attachment(_)
            | Error::EmailBody(_)
            | Error::InvalidEmailDomain(_)
            | Error::InvalidContentType
            | Error::NotASCII(_)
            | Error::MissingContent => HttpResponse::BadRequest().body(val.to_string()),
        }
    }
}

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        error!("Response Error: {}", self);
        HttpResponse::from(self)
    }
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            Error::ApiKeyInvalid => StatusCode::UNAUTHORIZED,
            Error::EmailSend(_)
            | Error::TemplateRender(_)
            | Error::TemplateLoad(_)
            | Error::ApiKeyLookup(_)
            | Error::EnvVarMissing(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Attachment(_)
            | Error::EmailBody(_)
            | Error::InvalidEmailDomain(_)
            | Error::InvalidContentType
            | Error::NotASCII(_)
            | Error::MissingContent => StatusCode::BAD_REQUEST,
        }
    }
}
