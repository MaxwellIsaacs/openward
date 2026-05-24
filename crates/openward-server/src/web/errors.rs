use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use openward_core::{DomainError, RegistryError};
use crate::i18n::{T, DEFAULT_LANGUAGE};
use crate::templates::ErrorTemplate;

/// Error type for HTML handlers.
pub enum HtmlError {
    Registry(RegistryError),
    Redirect(String),
    Validation(String),
    Forbidden,
    NotFound,
}

impl HtmlError {
    pub fn redirect(url: &str) -> Self {
        Self::Redirect(url.to_string())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
}

impl From<RegistryError> for HtmlError {
    fn from(e: RegistryError) -> Self {
        Self::Registry(e)
    }
}

impl From<sqlx::Error> for HtmlError {
    fn from(e: sqlx::Error) -> Self {
        Self::Registry(RegistryError::Database(e.to_string()))
    }
}

fn make_t() -> T {
    // Error pages use default language since we may not have session context.
    // The error handler doesn't have access to headers/state, so we use defaults.
    T::new(DEFAULT_LANGUAGE, None)
}

impl IntoResponse for HtmlError {
    fn into_response(self) -> Response {
        match self {
            HtmlError::Redirect(url) => Redirect::to(&url).into_response(),
            HtmlError::Registry(e) => {
                let t = make_t();
                let (status, message) = match &e {
                    RegistryError::Domain(d) => match d {
                        DomainError::DetaineeNotFound { .. } => {
                            (StatusCode::NOT_FOUND, t.get("error-not-found"))
                        }
                        DomainError::InactiveDetainee { .. }
                        | DomainError::ReleaseHold { .. } => {
                            (StatusCode::CONFLICT, t.get_1("error-conflict", "detail", &e.to_string()))
                        }
                        DomainError::InvalidBasisTransition { .. } => (
                            StatusCode::UNPROCESSABLE_ENTITY,
                            t.get_1("error-invalid-transition", "detail", &e.to_string()),
                        ),
                        DomainError::FutureDate { .. } | DomainError::ZeroSentence => {
                            (StatusCode::BAD_REQUEST, t.get_1("error-invalid-data", "detail", &e.to_string()))
                        }
                    },
                    RegistryError::Database(_) => {
                        tracing::error!("{}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            t.get("error-internal"),
                        )
                    }
                };

                let template = ErrorTemplate {
                    nav: None,
                    status: status.as_u16(),
                    message,
                    t,
                };
                (status, template).into_response()
            }
            HtmlError::Forbidden => {
                let t = make_t();
                let template = ErrorTemplate {
                    nav: None,
                    status: 403,
                    message: t.get("error-forbidden"),
                    t,
                };
                (StatusCode::FORBIDDEN, template).into_response()
            }
            HtmlError::NotFound => {
                let t = make_t();
                let template = ErrorTemplate {
                    nav: None,
                    status: 404,
                    message: t.get("error-not-found"),
                    t,
                };
                (StatusCode::NOT_FOUND, template).into_response()
            }
            HtmlError::Validation(msg) => {
                let t = make_t();
                // If msg looks like a fluent key, resolve it; otherwise use as-is
                let message = if msg.contains('-') && !msg.contains(' ') {
                    t.get(&msg)
                } else {
                    msg
                };
                let template = ErrorTemplate {
                    nav: None,
                    status: 422,
                    message,
                    t,
                };
                (StatusCode::UNPROCESSABLE_ENTITY, template).into_response()
            }
        }
    }
}
