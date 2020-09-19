use actix_multipart::Multipart;
use actix_web::http::header;
use actix_web::{web, HttpRequest, HttpResponse};

use chrono::Utc;

use futures::{StreamExt, TryStreamExt};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use rusoto_s3::{PutObjectRequest, S3Client, S3};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fmt::Display;
use std::iter;

use crate::oauth;
use crate::SiteConfig;

// To make the timepart shorter, we'll offset it with a custom epoch.
const EPOCH: i64 = 631152000;

#[derive(Serialize, Deserialize)]
struct MicropubError {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_description: Option<String>,
}

impl MicropubError {
    pub fn new<S>(err: S) -> Self
    where
        S: Into<String>,
    {
        MicropubError {
            error: err.into(),
            error_description: None,
        }
    }

    pub fn with_description<S, D>(err: S, description: D) -> Self
    where
        S: Into<String>,
        D: Display,
    {
        MicropubError {
            error: err.into(),
            error_description: Some(format!("{}", description)),
        }
    }
}

fn random_id() -> String {
    let now = Utc::now();

    // Generate the time part
    let ts = now.timestamp() - EPOCH;
    let offset = (ts.leading_zeros() / 8) as usize;
    let time_part = base32::encode(
        base32::Alphabet::RFC4648 { padding: false },
        &ts.to_be_bytes()[offset..],
    );

    // Generate the random part
    let mut rng = thread_rng();
    let random_part: String = iter::repeat(())
        .map(|()| rng.sample(Alphanumeric))
        .take(7)
        .collect();

    format!("{}-{}", time_part, random_part)
}

pub async fn handle_upload(
    req: HttpRequest,
    mut payload: Multipart,
    site: web::Data<SiteConfig>,
    s3_client: web::Data<S3Client>,
    verification_service: web::Data<oauth::VerificationService>,
) -> HttpResponse {
    let auth_header = match req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|s| s.to_str().ok())
    {
        Some(auth_header) => auth_header,
        None => return HttpResponse::Unauthorized().json(MicropubError::new("unauthorized")),
    };

    let access_token = match verification_service.validate(auth_header).await {
        Ok(token) => token,
        Err(e) => {
            return HttpResponse::Unauthorized()
                .json(MicropubError::with_description("unauthorized", e))
        }
    };

    if !access_token.scopes().any(|s| s == "media") {
        return HttpResponse::Unauthorized().json(MicropubError::new("unauthorized"));
    }

    // iterate over multipart stream
    if let Ok(Some(field)) = payload.try_next().await {
        let content_disp = field.content_disposition().unwrap();
        let content_type = field.content_type().clone();
        let filename = content_disp.get_filename();
        let ext = filename.and_then(|f| f.rsplit('.').next());
        let (classification, sep, suffix) = match content_type.type_() {
            mime::IMAGE => ("photo", '.', ext),
            mime::AUDIO => ("audio", '.', ext),
            mime::VIDEO => ("video", '.', ext),
            _ => ("file", '/', filename),
        };

        // This will be the key in S3.
        let key = match suffix {
            Some(ext) => format!("{}{}{}", random_id(), sep, ext),
            None => format!("{}", random_id()),
        };

        // This will be the publicly accessible URL for the file.
        let url = if classification == "photo" {
            format!("{}/photo/{}x{}/{}", site.media_url(), site.default_width(), site.default_height(), key)
        } else {
            format!("{}/{}/{}", site.media_url(), classification, key)
        };

        let mut metadata: HashMap<String, String> = HashMap::new();
        metadata.insert(
            "client-id".to_string(),
            access_token.client_id().to_string(),
        );
        metadata.insert("author".to_string(), access_token.me().to_string());
        if let Some(f) = filename {
            metadata.insert("filename".to_string(), f.to_string());
        }

        let body = field
            .map(|b| b.map(|b| b.to_vec()))
            .try_concat()
            .await
            .unwrap();

        let put_request = PutObjectRequest {
            bucket: site.s3_bucket().to_owned(),
            key: format!("{}/{}", classification, key),
            body: Some(body.into()),
            metadata: Some(metadata),
            content_type: Some(content_type.to_string()),
            ..Default::default()
        };

        match s3_client.put_object(put_request).await {
            Ok(_) => {
                return HttpResponse::Created()
                    .header(header::LOCATION, url)
                    .finish();
            }
            Err(e) => return HttpResponse::InternalServerError().body(format!("{}", e)),
        };
    }

    HttpResponse::BadRequest().finish()
}
