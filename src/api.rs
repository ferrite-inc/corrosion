mod error;
mod extract;
mod v1;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Request, State},
    http::{
        StatusCode,
        header::{AUTHORIZATION, WWW_AUTHENTICATE},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use serde_json::json;
use subtle::ConstantTimeEq;
#[cfg(feature = "bindings")]
pub(crate) use v1::{Enqueue, UpdateVoice};

use crate::runtime::Runtime;

pub(crate) fn router(runtime: Runtime, token: String) -> Router {
    Router::new()
        .nest("/api/v1", v1::router())
        .with_state(runtime)
        .layer(middleware::from_fn_with_state(
            Arc::<str>::from(token),
            authenticate,
        ))
}

async fn authenticate(State(token): State<Arc<str>>, request: Request, next: Next) -> Response {
    if token.is_empty() {
        return next.run(request).await;
    }

    let mut authorization = request.headers().get_all(AUTHORIZATION).iter();
    let credentials = match (authorization.next(), authorization.next()) {
        (Some(header), None) => header.to_str().ok(),
        _ => None,
    };
    let authorized = credentials
        .and_then(|credentials| credentials.split_once(' '))
        .is_some_and(|(scheme, provided)| {
            scheme.eq_ignore_ascii_case("Bearer")
                && bool::from(token.as_bytes().ct_eq(provided.trim_start().as_bytes()))
        });

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, "Bearer")],
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    next.run(request).await
}
