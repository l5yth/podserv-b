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
//! media directory once, pre-renders the index page, and loads all cover art
//! into memory. All state is shared immutably across requests.

mod config;
mod media;
mod render;

use actix_files::Files;
use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::http::header;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, get, web};
use config::Config;
use media::{Section, scan_sections};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path};

/// Pre-rendered index page and its HTTP ETag.
struct PageCache {
    /// Complete HTML document returned by `GET /`.
    html: String,
    /// Quoted ETag value (`"<hash>"`) for HTTP conditional-GET support.
    etag: String,
}

/// Cover art keyed by episode relative path.
///
/// Values are `(mime_type, image_bytes)`. Only `image/*` MIME types are
/// included; all entries were validated at startup in [`media::scan_sections`].
type ArtMap = HashMap<String, (String, Vec<u8>)>;

/// Builds an [`ArtMap`] from pre-scanned section data.
fn build_art_map(sections: &[Section]) -> ArtMap {
    sections
        .iter()
        .flat_map(|s| s.episodes.iter())
        .filter_map(|e| {
            e.art
                .as_ref()
                .map(|(mime, data)| (e.rel_path.clone(), (mime.clone(), data.clone())))
        })
        .collect()
}

/// Computes a quoted ETag value from the given string content.
fn compute_etag(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("\"{}\"", h.finish())
}

/// Serves the pre-rendered episode browser page.
///
/// Returns `304 Not Modified` when the client's `If-None-Match` header
/// matches the current ETag, otherwise returns the full HTML with `ETag`
/// and `Cache-Control: no-cache` headers.
#[get("/")]
async fn index(cache: web::Data<PageCache>, req: HttpRequest) -> HttpResponse {
    if let Some(inm) = req.headers().get(header::IF_NONE_MATCH)
        && inm.as_bytes() == cache.etag.as_bytes()
    {
        return HttpResponse::NotModified().finish();
    }
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header((header::ETAG, cache.etag.as_str()))
        .insert_header((header::CACHE_CONTROL, "no-cache"))
        .body(cache.html.clone())
}

/// Serves embedded cover art for a media file.
///
/// Looks up the episode's pre-loaded art by `path` in the in-memory
/// [`ArtMap`] and returns its bytes with the appropriate `Content-Type` and
/// `Cache-Control: max-age=86400, immutable`.
///
/// Returns `404` if no art is available for that path. Returns `400` if the
/// path contains `..` or leading `/` components.
///
/// # Path traversal
///
/// This handler rejects `..` and leading `/` components as defence-in-depth.
/// Because art is served entirely from an in-memory map whose keys were
/// derived from scanned filenames, there is no filesystem access here and no
/// real traversal risk — the guard is retained to make the intent explicit and
/// to stay safe if the implementation ever changes.
///
/// The `/media/` route (served by `actix-files`) has its own independent
/// traversal protection built into that crate.
#[get("/art/{path:.*}")]
async fn art(req_path: web::Path<String>, art_map: web::Data<ArtMap>) -> HttpResponse {
    let rel = req_path.as_str();
    if Path::new(rel)
        .components()
        .any(|c| c == Component::ParentDir || c == Component::RootDir)
    {
        return HttpResponse::BadRequest().finish();
    }
    match art_map.get(rel) {
        Some((mime, data)) => HttpResponse::Ok()
            .content_type(mime.as_str())
            .insert_header((header::CACHE_CONTROL, "max-age=86400, immutable"))
            .body(data.clone()),
        None => HttpResponse::NotFound().finish(),
    }
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

    let html = render::render_page(&config, &sections);
    let etag = compute_etag(&html);
    let cache = web::Data::new(PageCache { html, etag });
    let art_map = web::Data::new(build_art_map(&sections));
    // 60-request burst per IP, then 1 req/s replenishment.
    // The burst absorbs a full page load + cover-art spray (~20–30 concurrent
    // image requests); 1 req/s sustains audio streaming (range requests arrive
    // every several seconds) while stopping a flood after the first bucket.
    // Built outside the closure so all worker threads share the same
    // Arc-backed RateLimiter state.
    let governor_conf = GovernorConfigBuilder::default()
        .seconds_per_request(1)
        .burst_size(60)
        .finish()
        .unwrap();

    HttpServer::new(move || {
        App::new()
            .wrap(Governor::new(&governor_conf))
            .app_data(cache.clone())
            .app_data(art_map.clone())
            .service(index)
            .service(art)
            // actix-files handles path-traversal sanitisation for /media/.
            .service(Files::new("/media", &media_dir))
    })
    .bind(&bind)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::App;
    use actix_web::test as aw_test;
    use media::Episode;

    fn make_ep(rel_path: &str, has_art: bool) -> Episode {
        Episode {
            rel_path: rel_path.into(),
            title: "T".into(),
            artist: "A".into(),
            album: "".into(),
            year: "".into(),
            duration: "".into(),
            size_mb: "1.0".into(),
            art: if has_art {
                Some(("image/jpeg".into(), vec![0xFF]))
            } else {
                None
            },
        }
    }

    // --- compute_etag ---

    #[test]
    fn etag_is_quoted() {
        let e = compute_etag("hello");
        assert!(e.starts_with('"'));
        assert!(e.ends_with('"'));
    }

    #[test]
    fn etag_same_input_same_output() {
        assert_eq!(compute_etag("hello"), compute_etag("hello"));
    }

    #[test]
    fn etag_different_inputs_differ() {
        assert_ne!(compute_etag("hello"), compute_etag("world"));
    }

    // --- build_art_map ---

    #[test]
    fn build_art_map_empty_sections() {
        assert!(build_art_map(&[]).is_empty());
    }

    #[test]
    fn build_art_map_includes_only_art_episodes() {
        let sections = vec![Section {
            heading: "p".into(),
            episodes: vec![make_ep("with.mp3", true), make_ep("without.mp3", false)],
        }];
        let map = build_art_map(&sections);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("with.mp3"));
        assert!(!map.contains_key("without.mp3"));
    }

    #[test]
    fn build_art_map_stores_mime_and_data() {
        let sections = vec![Section {
            heading: "p".into(),
            episodes: vec![make_ep("a.mp3", true)],
        }];
        let map = build_art_map(&sections);
        let (mime, data) = map.get("a.mp3").unwrap();
        assert_eq!(mime, "image/jpeg");
        assert_eq!(data, &[0xFF]);
    }

    // --- index handler ---

    #[actix_web::test]
    async fn index_returns_200_with_html_body() {
        let html = render::render_page(&Config::default(), &[]);
        let etag = compute_etag(&html);
        let cache = web::Data::new(PageCache { html, etag });
        let app = aw_test::init_service(App::new().app_data(cache).service(index)).await;
        let req = aw_test::TestRequest::get().uri("/").to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);
        assert!(
            aw_test::read_body(resp)
                .await
                .starts_with(b"<!DOCTYPE html>")
        );
    }

    #[actix_web::test]
    async fn index_returns_304_on_matching_etag() {
        let html = render::render_page(&Config::default(), &[]);
        let etag = compute_etag(&html);
        let cache = web::Data::new(PageCache {
            html,
            etag: etag.clone(),
        });
        let app = aw_test::init_service(App::new().app_data(cache).service(index)).await;
        let req = aw_test::TestRequest::get()
            .uri("/")
            .insert_header((header::IF_NONE_MATCH, etag))
            .to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 304);
    }

    #[actix_web::test]
    async fn index_returns_cache_control_header() {
        let html = render::render_page(&Config::default(), &[]);
        let etag = compute_etag(&html);
        let cache = web::Data::new(PageCache { html, etag });
        let app = aw_test::init_service(App::new().app_data(cache).service(index)).await;
        let req = aw_test::TestRequest::get().uri("/").to_request();
        let resp = aw_test::call_service(&app, req).await;
        let cc = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(cc, "no-cache");
    }

    // --- art handler ---

    #[actix_web::test]
    async fn art_returns_404_for_missing_file() {
        let app = aw_test::init_service(
            App::new()
                .app_data(web::Data::new(ArtMap::new()))
                .service(art),
        )
        .await;
        let req = aw_test::TestRequest::get()
            .uri("/art/nonexistent.mp3")
            .to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 404);
    }

    #[actix_web::test]
    async fn art_returns_image_bytes() {
        let mut map = ArtMap::new();
        map.insert(
            "art.mp3".to_string(),
            ("image/jpeg".to_string(), vec![0xFF, 0xD8, 0xFF]),
        );
        let app =
            aw_test::init_service(App::new().app_data(web::Data::new(map)).service(art)).await;
        let req = aw_test::TestRequest::get().uri("/art/art.mp3").to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(aw_test::read_body(resp).await.as_ref(), &[0xFF, 0xD8, 0xFF]);
    }

    #[actix_web::test]
    async fn art_returns_cache_control_header() {
        let mut map = ArtMap::new();
        map.insert("art.mp3".to_string(), ("image/jpeg".to_string(), vec![]));
        let app =
            aw_test::init_service(App::new().app_data(web::Data::new(map)).service(art)).await;
        let req = aw_test::TestRequest::get().uri("/art/art.mp3").to_request();
        let resp = aw_test::call_service(&app, req).await;
        let cc = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cc.contains("max-age=86400"));
    }

    #[actix_web::test]
    async fn art_rejects_path_traversal() {
        let app = aw_test::init_service(
            App::new()
                .app_data(web::Data::new(ArtMap::new()))
                .service(art),
        )
        .await;
        let req = aw_test::TestRequest::get()
            .uri("/art/..%2F..%2Fetc%2Fpasswd")
            .to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_ne!(resp.status().as_u16(), 200);
    }

    // --- rate limiting ---

    #[actix_web::test]
    async fn rate_limiter_returns_429_after_burst_exhausted() {
        // Tight limit (burst=2) so only 3 requests are needed.
        let conf = GovernorConfigBuilder::default()
            .seconds_per_request(1)
            .burst_size(2)
            .finish()
            .unwrap();
        let html = render::render_page(&Config::default(), &[]);
        let etag = compute_etag(&html);
        let cache = web::Data::new(PageCache { html, etag });
        let app = aw_test::init_service(
            App::new()
                .wrap(Governor::new(&conf))
                .app_data(cache)
                .service(index),
        )
        .await;
        // PeerIpKeyExtractor needs a real socket address; TestRequest has no
        // underlying TCP connection, so we supply one explicitly.
        let addr: std::net::SocketAddr = "127.0.0.1:1234".parse().unwrap();
        // First two requests succeed (burst_size = 2).
        for _ in 0..2 {
            let req = aw_test::TestRequest::get()
                .uri("/")
                .peer_addr(addr)
                .to_request();
            assert_eq!(
                aw_test::call_service(&app, req).await.status().as_u16(),
                200
            );
        }
        // Third request exhausts the bucket → 429.
        let req = aw_test::TestRequest::get()
            .uri("/")
            .peer_addr(addr)
            .to_request();
        assert_eq!(
            aw_test::call_service(&app, req).await.status().as_u16(),
            429
        );
    }
}
