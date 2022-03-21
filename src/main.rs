use actix_middleware_rfc7662::RequireAuthorizationConfig;
use actix_web::web::Data;
use actix_web::{middleware, App, HttpServer};
use rusoto_core::Region;
use rusoto_s3::S3Client;
use serde::{Deserialize, Serialize};

mod media;
mod micropub;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SiteConfig {
    bind: String,

    media_url: String,
    s3_bucket: String,

    oauth2_auth_endpoint: String,
    oauth2_introspect_endpoint: String,
    oauth2_client_id: String,
    oauth2_client_secret: String,

    allowed_username: String,

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

    pub fn oauth2_auth_endpoint(&self) -> &str {
        &self.oauth2_auth_endpoint
    }

    pub fn oauth2_introspect_endpoint(&self) -> &str {
        &self.oauth2_introspect_endpoint
    }

    pub fn oauth2_client_id(&self) -> &str {
        &self.oauth2_client_id
    }

    pub fn oauth2_client_secret(&self) -> &str {
        &self.oauth2_client_secret
    }

    /// The username that is allowed to upload to this endpoint.
    pub fn allowed_username(&self) -> &str {
        &self.allowed_username
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let site_config = Data::new(SiteConfig {
        bind: std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8180".to_string()),
        s3_bucket: std::env::var("S3_BUCKET").expect("Expected S3_BUCKET env var"),
        media_url: std::env::var("MEDIA_URL").expect("Expected MEDIA_URL env var"),
        oauth2_auth_endpoint: std::env::var("OAUTH2_AUTH_ENDPOINT")
            .expect("Expected OAUTH2_AUTH_ENDPOINT env var"),
        oauth2_introspect_endpoint: std::env::var("OAUTH2_INTROSPECT_ENDPOINT")
            .expect("Expected OAUTH2_INTROSPECT_ENDPOINT env var"),
        oauth2_client_id: std::env::var("OAUTH2_CLIENT_ID")
            .expect("Expected OAUTH2_CLIENT_ID env var"),
        oauth2_client_secret: std::env::var("OAUTH2_CLIENT_SECRET")
            .expect("Expected OAUTH2_CLIENT_SECRET env var"),
        allowed_username: std::env::var("ALLOWED_USERNAME")
            .expect("Expected ALLOWED_USERNAME env var"),
        default_width: std::env::var("DEFAULT_WIDTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000),
        default_height: std::env::var("DEFAULT_HEIGHT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    });

    let bind = site_config.bind().to_string();
    let s3_client = Data::new(S3Client::new(Region::default()));

    let oauth2_config = RequireAuthorizationConfig::new(
        site_config.oauth2_client_id().to_string(),
        Some(site_config.oauth2_client_secret().to_string()),
        site_config
            .oauth2_auth_endpoint()
            .parse()
            .expect("invalid url"),
        site_config
            .oauth2_introspect_endpoint()
            .parse()
            .expect("invalid url"),
    );

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(site_config.clone())
            .app_data(s3_client.clone())
            .app_data(oauth2_config.clone())
            .service(micropub::handle_upload)
            .configure(media::configure)
    })
    .bind(bind)?
    .run()
    .await
}
