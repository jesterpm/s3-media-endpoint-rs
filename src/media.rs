use std::io::Cursor;

use actix_web::error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound};
use actix_web::http::header;
use actix_web::{get, head, web, Error, HttpRequest, HttpResponse};

use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{GenericImageView, DynamicImage, ImageReader, ImageDecoder};
use image::ImageFormat;

use futures::TryFutureExt;
use tokio::io::AsyncReadExt;

use rusoto_s3::{GetObjectRequest, HeadObjectRequest, S3Client, S3};

use crate::SiteConfig;

/// Build an HttpResponse for an AWS response
macro_rules! response_for {
    ($resp:expr) => {{
        let mut client_resp = HttpResponse::Ok();

        // This will be the default cache-control header if the object doesn't have its own.
        client_resp.insert_header(header::CacheControl(vec![header::CacheDirective::MaxAge(
            31557600u32,
        )]));

        // Allow CORS
        client_resp.insert_header((header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"));

        // Copy all of the relevant S3 headers.
        $resp
            .cache_control
            .map(|v| client_resp.insert_header((header::CACHE_CONTROL, v)));
        $resp
            .content_disposition
            .map(|v| client_resp.insert_header((header::CONTENT_DISPOSITION, v)));
        $resp
            .content_encoding
            .map(|v| client_resp.insert_header((header::CONTENT_ENCODING, v)));
        $resp
            .content_language
            .map(|v| client_resp.insert_header((header::CONTENT_LANGUAGE, v)));
        $resp
            .content_type
            .map(|v| client_resp.insert_header((header::CONTENT_TYPE, v)));
        $resp
            .e_tag
            .map(|v| client_resp.insert_header((header::ETAG, v)));
        $resp
            .last_modified
            .map(|v| client_resp.insert_header((header::LAST_MODIFIED, v)));

        client_resp
    }};
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(serve_photo)
        .service(serve_file)
        .service(head_file);
}

#[head("/media/{type}/{filename:.+}")]
async fn head_file(
    req: HttpRequest,
    config: web::Data<SiteConfig>,
    s3_client: web::Data<S3Client>,
) -> Result<HttpResponse, Error> {
    // Get the path parameters
    let media_type = req
        .match_info()
        .get("type")
        .ok_or(ErrorBadRequest("Bad URI"))?;
    let filename = req
        .match_info()
        .get("filename")
        .ok_or(ErrorBadRequest("Bad URI"))?;

    // Construct an S3 key
    let key = format!("{}/{}", media_type, filename);
    let resp = s3_client
        .head_object(HeadObjectRequest {
            bucket: config.s3_bucket().to_owned(),
            key,
            ..Default::default()
        })
        .map_err(|e| ErrorInternalServerError(e))
        .await?;

    let mut client_resp = response_for!(resp);
    // TODO: trick actix into returning the content-length.
    Ok(client_resp.finish())
}

#[get("/media/{type}/{filename:.+}")]
async fn serve_file(
    req: HttpRequest,
    config: web::Data<SiteConfig>,
    s3_client: web::Data<S3Client>,
) -> Result<HttpResponse, Error> {
    // Get the path parameters
    let media_type = req
        .match_info()
        .get("type")
        .ok_or(ErrorBadRequest("Bad URI"))?;
    let filename = req
        .match_info()
        .get("filename")
        .ok_or(ErrorBadRequest("Bad URI"))?;

    // Construct an S3 key
    let key = format!("{}/{}", media_type, filename);
    let resp = s3_client
        .get_object(GetObjectRequest {
            bucket: config.s3_bucket().to_owned(),
            key,
            ..Default::default()
        })
        .map_err(|e| ErrorInternalServerError(e))
        .await?;

    // If there is no payload, return a 404.
    let data = resp.body.ok_or(ErrorNotFound("Not found"))?;

    let mut client_resp = response_for!(resp);
    Ok(client_resp.streaming(data))
}

#[get("/media/photo/{width:\\d+}x{height:\\d+}/{filename}")]
async fn serve_photo(
    req: HttpRequest,
    config: web::Data<SiteConfig>,
    s3_client: web::Data<S3Client>,
) -> Result<HttpResponse, Error> {
    let width = req
        .match_info()
        .get("width")
        .ok_or(ErrorBadRequest("Bad URI"))
        .and_then(|v| v.parse().map_err(|_| ErrorBadRequest("Bad URI")))?;
    let height = req
        .match_info()
        .get("height")
        .ok_or(ErrorBadRequest("Bad URI"))
        .and_then(|v| v.parse().map_err(|_| ErrorBadRequest("Bad URI")))?;
    let filename = req
        .match_info()
        .get("filename")
        .ok_or(ErrorBadRequest("Bad URI"))?;

    let key = format!("photo/{}", filename);
    let resp = s3_client
        .get_object(GetObjectRequest {
            bucket: config.s3_bucket().to_owned(),
            key,
            ..Default::default()
        })
        .map_err(|e| ErrorInternalServerError(e))
        .await?;

    let mut data = Vec::new();
    resp.body
        .ok_or(ErrorNotFound("Not found"))?
        .into_async_read()
        .read_to_end(&mut data)
        .await?;

    // Resize the image
    let (mime, new_data) = web::block(move || scale_image(data.as_ref(), width, height))
        .await?
        .map_err(|e| ErrorInternalServerError(e))?;

    // Send the new image to the client.
    let mut client_resp = response_for!(resp);
    client_resp.insert_header((header::CONTENT_TYPE, mime));

    Ok(client_resp.body(new_data))
}

fn scale_image(
    data: &[u8],
    width: u32,
    height: u32,
) -> Result<(&'static str, Vec<u8>), image::ImageError> {
    // Determine the image format
    let fmt = image::guess_format(data)?;

    // Grab the orientation
    let mut decoder = ImageReader::with_format(Cursor::new(data), fmt).into_decoder()?;
    let orientation = decoder.orientation()
        .unwrap_or(Orientation::NoTransforms);

    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);

    let (orig_width, orig_height) = img.dimensions();

    let scaled = if width < orig_width && height < orig_height {
        // Take the largest size that maintains the aspect ratio
        let ratio = orig_width as f64 / orig_height as f64;
        let (new_width, new_height) = if width > height {
            (width, (width as f64 / ratio) as u32)
        } else {
            ((height as f64 * ratio) as u32, height)
        };
        img.resize(new_width, new_height, FilterType::CatmullRom)
    } else {
        // We're not going to scale up images.
        img
    };

    let mut new_data = Vec::new();
    scaled.write_to(&mut Cursor::new(&mut new_data), fmt)?;

    Ok((mime_for_image(fmt), new_data))
}

fn mime_for_image(fmt: ImageFormat) -> &'static str {
    match fmt {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::Tiff => "image/tiff",
        ImageFormat::Ico => "image/vnd.microsoft.icon",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Bmp => "image/bmp",
        ImageFormat::Pnm => "image/x-portable-anymap",
        ImageFormat::Tga => "image/x-tga",
        ImageFormat::Dds => "image/vnd.ms-dds",
        ImageFormat::Hdr => "image/vnd.radiance",
        ImageFormat::Farbfeld => "image/farbfeld",
        _ => "",
    }
}
