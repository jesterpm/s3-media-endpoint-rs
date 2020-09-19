use actix_web::client::Client;
use actix_web::error::Error;
use actix_web::http::{header, StatusCode};
use actix_web::ResponseError;
use derive_more::Display;
use futures::{FutureExt, TryFutureExt};
use serde::{Deserialize, Serialize};

/// Representation of an OAuth Access Token
#[derive(Serialize, Deserialize)]
pub struct AccessToken {
    me: String,
    client_id: String,
    scope: String,
}

impl AccessToken {
    pub fn me(&self) -> &str {
        &self.me
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn scopes(&self) -> impl Iterator<Item = &str> + '_ {
        self.scope.split_ascii_whitespace()
    }
}

/// Verification Service takes an Authorization header and checks if it's valid.
pub struct VerificationService {
    token_endpoint: String,
    client: Client,
}

impl VerificationService {
    pub fn new<S>(token_endpoint: S) -> VerificationService
    where
        S: Into<String>,
    {
        VerificationService {
            token_endpoint: token_endpoint.into(),
            client: Client::new(),
        }
    }

    pub async fn validate(&self, auth_token: &str) -> Result<AccessToken, impl std::error::Error> {
        self.client
            .get(&self.token_endpoint)
            .header(header::AUTHORIZATION, auth_token)
            .send()
            .map_err(Error::from)
            .map(|res| {
                res.and_then(|r| {
                    if r.status().is_success() {
                        Ok(r)
                    } else if r.status() == StatusCode::UNAUTHORIZED {
                        Err(VerificationError::Unauthenticated.into())
                    } else {
                        Err(VerificationError::InternalError(
                            r.status()
                                .canonical_reason()
                                .unwrap_or("Unknown Error")
                                .to_string(),
                        )
                        .into())
                    }
                })
            })
            .map_err(Error::from)
            .and_then(|mut resp| resp.json().map_err(Error::from))
            .await
    }
}

#[derive(Display, Debug)]
pub enum VerificationError {
    #[display(fmt = "Unauthenticated")]
    Unauthenticated,
    #[display(fmt = "AuthServer Error")]
    InternalError(String),
}

impl ResponseError for VerificationError {}
