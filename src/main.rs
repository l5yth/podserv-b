// Copyright (C) 2026 l5yth
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! podserv-b — a minimalist podcast server for serving media files on the web.
//!
//! Startup reads [`config::Config`] and scans [`media::Section`]s from the
//! media directory once. All state is shared immutably across requests.

mod config;
mod media;
mod render;

use actix_files::Files;
use actix_web::{App, HttpResponse, HttpServer, get, web};
use config::Config;
use id3::Tag;
use media::{Section, scan_sections};
use std::fs;
use std::path::{Component, Path};

/// Serves the episode browser page.
///
/// Renders [`render::render_page`] with the pre-loaded [`Config`] and
/// [`Section`] list, returning a complete HTML document.
#[get("/")]
async fn index(config: web::Data<Config>, sections: web::Data<Vec<Section>>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(render::render_page(&config, &sections))
}

/// Serves embedded cover art for a media file.
///
/// Reads the MP3 at `{media_dir}/{path}`, extracts the first ID3 `APIC`
/// picture frame, and returns its bytes with the appropriate `Content-Type`.
///
/// Returns `404` if the file does not exist, has no embedded art, or if the
/// path contains `..` components (directory traversal guard).
#[get("/art/{path:.*}")]
async fn art(req_path: web::Path<String>, media_dir: web::Data<String>) -> HttpResponse {
    // Reject path traversal attempts
    let rel = Path::new(req_path.as_str());
    if rel
        .components()
        .any(|c| c == Component::ParentDir || c == Component::RootDir)
    {
        return HttpResponse::BadRequest().finish();
    }

    let full_path = Path::new(media_dir.as_str()).join(rel);
    if let Ok(tag) = Tag::read_from_path(&full_path)
        && let Some(pic) = tag.pictures().next()
    {
        return HttpResponse::Ok()
            .content_type(pic.mime_type.clone())
            .body(pic.data.clone());
    }
    HttpResponse::NotFound().finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let media_dir = std::env::var("MEDIA_DIR").unwrap_or_else(|_| "media".into());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());

    let media_path = Path::new(&media_dir);
    if !media_path.exists() {
        fs::create_dir_all(media_path)?;
        eprintln!("created {media_dir}/ — drop mp3s there and restart");
    }

    let config = Config::load();
    let sections = scan_sections(&media_dir);
    let total: usize = sections.iter().map(|s| s.episodes.len()).sum();
    eprintln!(
        "{total} episode(s) in {} section(s) — listening on http://{bind}",
        sections.len()
    );

    let config_data = web::Data::new(config);
    let sections_data = web::Data::new(sections);
    let media_dir_data = web::Data::new(media_dir.clone());

    HttpServer::new(move || {
        App::new()
            .app_data(config_data.clone())
            .app_data(sections_data.clone())
            .app_data(media_dir_data.clone())
            .service(index)
            .service(art)
            .service(Files::new("/media", &media_dir))
    })
    .bind(&bind)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};
    use id3::{Tag, TagLike, Version};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn new_temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("podserv_main_test_{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[actix_web::test]
    async fn index_returns_200_with_html_body() {
        let config = web::Data::new(Config::default());
        let sections: Vec<Section> = vec![];
        let app = test::init_service(
            App::new()
                .app_data(config)
                .app_data(web::Data::new(sections))
                .service(index),
        )
        .await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);
        assert!(test::read_body(resp).await.starts_with(b"<!DOCTYPE html>"));
    }

    #[actix_web::test]
    async fn art_returns_404_for_missing_file() {
        let dir = new_temp_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(dir.to_str().unwrap().to_string()))
                .service(art),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/art/nonexistent.mp3")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 404);
        fs::remove_dir_all(dir).unwrap();
    }

    #[actix_web::test]
    async fn art_returns_image_bytes() {
        let dir = new_temp_dir();
        let path = dir.join("art.mp3");
        let mut tag = Tag::new();
        tag.add_frame(id3::frame::Picture {
            mime_type: "image/jpeg".into(),
            picture_type: id3::frame::PictureType::CoverFront,
            description: String::new(),
            data: vec![0xFF, 0xD8, 0xFF],
        });
        fs::write(&path, []).unwrap();
        tag.write_to_path(&path, Version::Id3v23).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(dir.to_str().unwrap().to_string()))
                .service(art),
        )
        .await;
        let req = test::TestRequest::get().uri("/art/art.mp3").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(test::read_body(resp).await.as_ref(), &[0xFF, 0xD8, 0xFF]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[actix_web::test]
    async fn art_rejects_path_traversal() {
        let dir = new_temp_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(dir.to_str().unwrap().to_string()))
                .service(art),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/art/..%2F..%2Fetc%2Fpasswd")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_ne!(resp.status().as_u16(), 200);
        fs::remove_dir_all(dir).unwrap();
    }
}
