use futures::{FutureExt, TryFutureExt};
use reqwest::header;
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
    client: reqwest::Client,
}

impl VerificationService {
    pub fn new<S>(token_endpoint: S) -> VerificationService
    where
        S: Into<String>,
    {
        VerificationService {
            token_endpoint: token_endpoint.into(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn validate(&self, auth_token: &str) -> Result<AccessToken, impl std::error::Error> {
        self.client
            .get(&self.token_endpoint)
            .header(header::AUTHORIZATION, auth_token)
            .send()
            .map(|res| res.and_then(|r| r.error_for_status()))
            .and_then(|resp| resp.json())
            .await
    }
}
