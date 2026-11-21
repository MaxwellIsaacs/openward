//! Static assets compiled into the binary, so a deployed `openward-server`
//! needs no files beside it.

use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

const ASSETS: &[(&str, &str, &[u8])] = &[
    ("htmx.min.js", "text/javascript; charset=utf-8", include_bytes!("../static/htmx.min.js")),
    ("pico.min.css", "text/css; charset=utf-8", include_bytes!("../static/pico.min.css")),
    ("style.css", "text/css; charset=utf-8", include_bytes!("../static/style.css")),
];

/// GET /static/{file}
pub async fn serve(Path(file): Path<String>) -> Response {
    match ASSETS.iter().find(|(name, _, _)| *name == file) {
        Some((_, content_type, body)) => {
            ([(header::CONTENT_TYPE, *content_type)], *body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
