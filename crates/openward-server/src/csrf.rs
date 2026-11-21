//! Cross-site request forgery protection.
//!
//! Rejects state-changing requests (anything other than GET/HEAD/OPTIONS)
//! that a browser reports as coming from another site. This follows the
//! header-based approach of Go's `net/http.CrossOriginProtection`:
//!
//! 1. If `Sec-Fetch-Site` is present (all current browsers send it), only
//!    `same-origin` and `none` (typed URL, bookmark) are allowed.
//! 2. Otherwise, if `Origin` is present, its host must match `Host`.
//! 3. Requests carrying neither header are not from a browser (curl,
//!    scripts, tests) and cannot be forged cross-site, so they pass.
//!
//! Together with `SameSite=Strict` session cookies this covers every form
//! and htmx POST/PUT/DELETE without threading tokens through templates.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub async fn protect(req: Request<Body>, next: Next) -> Response {
    if is_safe_method(req.method()) || is_same_origin(&req) {
        return next.run(req).await;
    }
    tracing::warn!(
        method = %req.method(),
        path = %req.uri().path(),
        "rejected cross-origin request"
    );
    (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response()
}

fn is_safe_method(m: &Method) -> bool {
    matches!(*m, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn is_same_origin(req: &Request<Body>) -> bool {
    let headers = req.headers();

    if let Some(site) = headers.get("sec-fetch-site") {
        return matches!(site.to_str(), Ok("same-origin") | Ok("none"));
    }

    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    // "null" origins (sandboxed iframes, file://) are never same-origin.
    let origin_host = origin.split_once("://").map(|(_, rest)| rest);
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    match (origin_host, host) {
        (Some(o), Some(h)) => o.eq_ignore_ascii_case(h),
        _ => false,
    }
}
