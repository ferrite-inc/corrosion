use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Resolver {
    Direct,
}

impl Resolver {
    pub(crate) fn resolve(self, input: &str) -> Result<StreamUrl, ResolveError> {
        match self {
            Self::Direct => input.try_into(),
        }
    }
}

pub(crate) struct StreamUrl(Url);

impl StreamUrl {
    pub(crate) fn into_string(self) -> String {
        self.0.into()
    }
}

impl TryFrom<&str> for StreamUrl {
    type Error = ResolveError;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        if input.len() > 8192 {
            return Err(ResolveError::TooLong);
        }

        let url = Url::parse(input).map_err(|_| ResolveError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ResolveError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ResolveError::Credentials);
        }
        if url.as_str().len() > 8192 {
            return Err(ResolveError::TooLong);
        }

        Ok(Self(url))
    }
}

#[derive(Debug, Error)]
pub(crate) enum ResolveError {
    #[error("input must be an absolute stream URL with a host")]
    InvalidUrl,
    #[error("stream URL must use HTTP or HTTPS")]
    UnsupportedScheme,
    #[error("stream URL must not contain credentials")]
    Credentials,
    #[error("stream URL exceeds 8192 bytes")]
    TooLong,
}
