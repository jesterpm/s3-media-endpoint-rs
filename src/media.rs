use actix_web::client::{Client, ClientResponse};
use actix_web::error::{ErrorBadRequest, ErrorInternalServerError};
use actix_web::http::header;
use actix_web::{web, Error, HttpRequest, HttpResponse};

use image::imageops::FilterType;
use image::GenericImageView;
use image::ImageFormat;

use crate::SiteConfig;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/media/photo/{width:\\d+}x{height:\\d+}/{filename}")
            .route(web::get().to(serve_photo)),
    );
    cfg.service(
        web::resource("/media/{type}/{filename:.+}")
            .route(web::get().to(serve_file))
            .route(web::head().to(serve_file)),
    );
}

async fn serve_photo(
    req: HttpRequest,
    config: web::Data<SiteConfig>,
    client: web::Data<Client>,
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

    let new_url = format!("{}/photo/{}", config.media_url(), filename);

    let forwarded_req = client.request_from(new_url, req.head());
    let forwarded_req = if let Some(addr) = req.head().peer_addr {
        forwarded_req.header("x-forwarded-for", format!("{}", addr.ip()))
    } else {
        forwarded_req
    };

    let mut res = forwarded_req.send().await.map_err(Error::from)?;

    // Check response code
    if !res.status().is_success() {
        return forward_response(res).await;
    }

    // Get the payload, at at least 20 MB of it...
    let data = res.body().limit(20971520).await?;

    // Resize the image
    let (mime, new_data) = web::block(move || scale_image(data.as_ref(), width, height)).await
        .map_err(|e| ErrorInternalServerError(e))?;

    // Send the new image to the client.
    let mut client_resp = HttpResponse::build(res.status());
    client_resp.set(header::CacheControl(vec![header::CacheDirective::MaxAge(
        86400u32,
    )]));
    client_resp.set_header(header::CONTENT_TYPE, mime);

    Ok(client_resp.body(new_data))
}

async fn serve_file(
    req: HttpRequest,
    config: web::Data<SiteConfig>,
    client: web::Data<Client>,
) -> Result<HttpResponse, Error> {
    let media_type = req
        .match_info()
        .get("type")
        .ok_or(ErrorBadRequest("Bad URI"))?;
    let filename = req
        .match_info()
        .get("filename")
        .ok_or(ErrorBadRequest("Bad URI"))?;

    let new_url = format!("{}/{}/{}", config.media_url(), media_type, filename);

    let forwarded_req = client.request_from(new_url, req.head()).no_decompress();

    let forwarded_req = if let Some(addr) = req.head().peer_addr {
        forwarded_req.header("x-forwarded-for", format!("{}", addr.ip()))
    } else {
        forwarded_req
    };

    let res = forwarded_req.send().await.map_err(Error::from)?;

    forward_response(res).await
}

async fn forward_response<S>(mut res: ClientResponse<S>) -> Result<HttpResponse, Error>
where
    S: futures::Stream<Item = std::result::Result<bytes::Bytes, actix_web::error::PayloadError>>
        + std::marker::Unpin,
{
    let mut client_resp = HttpResponse::build(res.status());

    // Remove `Connection` as per
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Connection#Directives
    for (header_name, header_value) in res.headers().iter().filter(|(h, _)| *h != "connection") {
        client_resp.header(header_name.clone(), header_value.clone());
    }

    Ok(client_resp.body(res.body().limit(2147483648).await?))
}

fn scale_image(data: &[u8], width: u32, height: u32) -> Result<(&'static str, Vec<u8>), image::ImageError> {
    // Determine the image format
    let fmt = image::guess_format(data)?;

    // Parse the image
    let img = image::load_from_memory_with_format(data, fmt)?;

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
    scaled
        .write_to(&mut new_data, fmt)?; // ImageOutputFormat::Jpeg(128))

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
