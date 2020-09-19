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

    media_url: String,
    token_endpoint: String,
    s3_bucket: String,

    default_width: u32,
    default_height: u32,
}

impl SiteConfig {
    pub fn bind(&self) -> &str {
        &self.bind
    }

    /// Base URL for serving files
    pub fn media_url(&self) -> &str {
        &self.media_url
    }

    /// The URI to use to validate an access token.
    pub fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }

    /// S3 output bucket
    pub fn s3_bucket(&self) -> &str {
        &self.s3_bucket
    }

    pub fn default_width(&self) -> u32 {
        self.default_width
    }

    pub fn default_height(&self) -> u32 {
        self.default_height
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
        default_width: std::env::var("DEFAULT_WIDTH").ok().and_then(|v| v.parse().ok()).unwrap_or(1000),
        default_height: std::env::var("DEFAULT_HEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
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
