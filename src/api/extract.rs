use axum::{
    extract::{FromRequest, FromRequestParts},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use super::error::Error;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(Error))]
pub(super) struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(Error))]
pub(super) struct Path<T>(pub T);
