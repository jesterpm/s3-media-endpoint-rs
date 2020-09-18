use actix_web::client::Client;
use actix_web::{middleware, web, App, HttpServer};

use rusoto_core::Region;
use rusoto_s3::S3Client;

use serde::{Deserialize, Serialize};

mod media;
mod micropub;
mod oauth;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SiteConfig {
    bind: String,

    token_endpoint: String,
    s3_bucket: String,
    media_url: String,
}

impl SiteConfig {
    pub fn bind(&self) -> &str {
        &self.bind
    }

    /// The URI to use to validate an access token.
    pub fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }

    /// S3 output bucket
    pub fn s3_bucket(&self) -> &str {
        &self.s3_bucket
    }

    /// Base URL for S3 bucket assets.
    pub fn media_url(&self) -> &str {
        &self.media_url
    }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    let site_config = SiteConfig {
        bind: std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8180".to_string()),
        s3_bucket: std::env::var("S3_BUCKET").expect("Expected S3_BUCKET env var"),
        media_url: std::env::var("MEDIA_URL").expect("Expected MEDIA_URL env var"),
        token_endpoint: std::env::var("TOKEN_ENDPOINT").expect("Expected TOKEN_ENDPOINT env var"),
    };

    let bind = site_config.bind().to_string();
    let s3_client = S3Client::new(Region::default());
    let token_endpoint = site_config.token_endpoint().to_string();

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(Client::new())
            .data(site_config.clone())
            .data(s3_client.clone())
            .data(oauth::VerificationService::new(token_endpoint.clone()))
            .service(
                web::resource("/micropub/media").route(web::post().to(micropub::handle_upload)),
            )
            .configure(media::configure)
    })
    .bind(bind)?
    .run()
    .await
}
