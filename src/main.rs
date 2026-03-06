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
mod rss;

use actix_files::Files;
use actix_governor::{Governor, GovernorConfigBuilder, KeyExtractor, SimpleKeyExtractionError};
use actix_web::http::header;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, get, web};
use clap::Parser;
use config::Config;
use media::{Section, scan_sections};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::path::{Component, Path};

/// Command-line arguments.
#[derive(Parser)]
#[command(
    version = concat!(
        "v",
        env!("CARGO_PKG_VERSION"),
        "\n",
        env!("CARGO_PKG_DESCRIPTION"),
        "\napache v2 (c) 2026 l5yth"
    ),
    before_help = concat!(
        "podserv-b v",
        env!("CARGO_PKG_VERSION"),
        "\n",
        env!("CARGO_PKG_DESCRIPTION"),
        "\napache v2 (c) 2026 l5yth"
    )
)]
struct Cli {
    /// Path to the TOML configuration file.
    #[arg(
        long,
        short = 'c',
        env = "CONFIG",
        default_value = "/etc/podserv-b.toml"
    )]
    config: String,

    /// Directory containing MP3 files to serve.
    #[arg(long, short = 'm', env = "MEDIA_DIR", default_value = "media")]
    media: String,

    /// Address to bind the HTTP server to.
    #[arg(long, short = 'b', env = "BIND", default_value = "127.0.0.1:8447")]
    bind: String,

    /// When set, attempt to parse a date from the episode filename
    /// (patterns: `YYYY-MM-DD`, `YYYY_MM_DD`, `YYYYMMDD`) and use it as the
    /// publication date, falling back to the file modification time if no
    /// date pattern is found.
    #[arg(long, env = "FILE_TO_META")]
    file_to_meta: bool,
}

/// Pre-rendered index page and its HTTP ETag.
struct PageCache {
    /// Complete HTML document returned by `GET /`.
    html: String,
    /// Quoted ETag value (`"<hash>"`) for HTTP conditional-GET support.
    etag: String,
}

/// Pre-rendered RSS feed and its HTTP ETag.
struct RssCache {
    /// Complete RSS 2.0 + iTunes XML document returned by `GET /rss`.
    xml: String,
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

/// Rate-limiting key extractor that uses the real client IP address.
///
/// When the TCP peer is `127.0.0.1` (i.e. the nginx reverse proxy), reads the
/// `X-Real-IP` header that nginx sets to `$remote_addr`. Otherwise falls back
/// to the TCP peer address. This gives correct per-client rate limiting both
/// when running behind nginx and when accessed directly (e.g. in tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RealIpKeyExtractor;

impl KeyExtractor for RealIpKeyExtractor {
    type Key = IpAddr;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(
        &self,
        req: &actix_web::dev::ServiceRequest,
    ) -> Result<Self::Key, Self::KeyExtractionError> {
        let peer = req
            .peer_addr()
            .map(|s| s.ip())
            .ok_or(SimpleKeyExtractionError::new("no peer address"))?;
        // Trust X-Real-IP only for connections from localhost (our nginx).
        // Covers both IPv4 (127.0.0.1) and IPv6 (::1) loopback addresses.
        // Direct connections (tests, standalone mode) use peer IP as-is.
        // A missing or unparseable X-Real-IP (e.g. a hostname) is silently
        // ignored and falls back to peer IP rather than returning an error.
        if (peer == IpAddr::from([127, 0, 0, 1])
            || peer == IpAddr::V6(std::net::Ipv6Addr::LOCALHOST))
            && let Some(real_ip) = req
                .headers()
                .get("X-Real-IP")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<IpAddr>().ok())
        {
            return Ok(real_ip);
        }
        Ok(peer)
    }
}

/// Computes a quoted ETag value from the given string content.
///
/// Uses [`DefaultHasher`], whose output is stable only within a single process
/// run. That is sufficient here: ETags only need to match within a client
/// session, and the page is re-rendered (and a fresh ETag computed) on every
/// server restart anyway.
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

/// Serves the pre-rendered RSS 2.0 + iTunes podcast feed.
///
/// Returns `304 Not Modified` when the client's `If-None-Match` header
/// matches the current ETag, otherwise returns the full XML with `ETag`
/// and `Cache-Control: no-cache` headers.
#[get("/rss")]
async fn rss_feed(cache: web::Data<RssCache>, req: HttpRequest) -> HttpResponse {
    if let Some(inm) = req.headers().get(header::IF_NONE_MATCH)
        && inm.as_bytes() == cache.etag.as_bytes()
    {
        return HttpResponse::NotModified().finish();
    }
    HttpResponse::Ok()
        .content_type("application/rss+xml; charset=utf-8")
        .insert_header((header::ETAG, cache.etag.as_str()))
        .insert_header((header::CACHE_CONTROL, "no-cache"))
        .body(cache.xml.clone())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let media_dir = cli.media;
    let bind = cli.bind;

    let media_path = Path::new(&media_dir);
    if !media_path.exists() {
        fs::create_dir_all(media_path)?;
        eprintln!("created {media_dir}/ — drop mp3s there and restart");
    }

    let config = Config::load(&cli.config);
    let sections = scan_sections(&media_dir, cli.file_to_meta);
    let total: usize = sections.iter().map(|s| s.episodes.len()).sum();
    eprintln!(
        "{total} episode(s) in {} section(s) — listening on http://{bind}",
        sections.len()
    );

    let html = render::render_page(&config, &sections);
    let etag = compute_etag(&html);
    let cache = web::Data::new(PageCache { html, etag });

    let rss_xml = rss::render_rss(&config, &sections);
    let rss_etag = compute_etag(&rss_xml);
    let rss_cache = web::Data::new(RssCache {
        xml: rss_xml,
        etag: rss_etag,
    });

    let art_map = web::Data::new(build_art_map(&sections));
    // 512-request burst per IP, then 1 req/s replenishment.
    // The burst absorbs a full page load + cover-art spray for large libraries
    // (one request per episode plus browser parallelism headroom); 1 req/s
    // sustains audio streaming (range requests arrive every several seconds)
    // while stopping a flood after the first bucket.
    // Built outside the closure so all worker threads share the same
    // Arc-backed RateLimiter state.
    let governor_conf = GovernorConfigBuilder::default()
        .key_extractor(RealIpKeyExtractor)
        .seconds_per_request(1)
        .burst_size(512)
        .finish()
        .unwrap();

    HttpServer::new(move || {
        App::new()
            .wrap(Governor::new(&governor_conf))
            .app_data(cache.clone())
            .app_data(rss_cache.clone())
            .app_data(art_map.clone())
            .service(index)
            .service(rss_feed)
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
            size_bytes: 1024,
            pub_date: None,
            art: if has_art {
                Some(("image/jpeg".into(), vec![0xFF]))
            } else {
                None
            },
        }
    }

    // --- Cli ---
    //
    // Tests that read or write MEDIA_DIR/BIND/CONFIG must hold ENV_LOCK for
    // their entire duration so they do not race with each other or with
    // pre-existing values in the process environment.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn cli_defaults() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: ENV_LOCK serialises all env-var access in this test module.
        unsafe {
            std::env::remove_var("CONFIG");
            std::env::remove_var("MEDIA_DIR");
            std::env::remove_var("BIND");
        }
        let cli = Cli::try_parse_from(["podserv-b"]).unwrap();
        assert_eq!(cli.config, "/etc/podserv-b.toml");
        assert_eq!(cli.media, "media");
        assert_eq!(cli.bind, "127.0.0.1:8447");
        assert!(!cli.file_to_meta);
    }

    #[test]
    fn cli_custom_args() {
        let cli = Cli::try_parse_from([
            "podserv-b",
            "--config",
            "/tmp/my.toml",
            "--media",
            "/data",
            "--bind",
            "0.0.0.0:8080",
        ])
        .unwrap();
        assert_eq!(cli.config, "/tmp/my.toml");
        assert_eq!(cli.media, "/data");
        assert_eq!(cli.bind, "0.0.0.0:8080");
    }

    #[test]
    fn cli_env_var_fallback() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: ENV_LOCK serialises all env-var access in this test module.
        unsafe {
            std::env::set_var("CONFIG", "/tmp/env.toml");
            std::env::set_var("MEDIA_DIR", "/env/media");
            std::env::set_var("BIND", "0.0.0.0:9090");
        }
        let cli = Cli::try_parse_from(["podserv-b"]).unwrap();
        unsafe {
            std::env::remove_var("CONFIG");
            std::env::remove_var("MEDIA_DIR");
            std::env::remove_var("BIND");
        }
        assert_eq!(cli.config, "/tmp/env.toml");
        assert_eq!(cli.media, "/env/media");
        assert_eq!(cli.bind, "0.0.0.0:9090");
    }

    #[test]
    fn cli_file_to_meta_flag() {
        let cli = Cli::try_parse_from(["podserv-b", "--file-to-meta"]).unwrap();
        assert!(cli.file_to_meta);
    }

    #[test]
    fn cli_short_aliases() {
        let cli = Cli::try_parse_from([
            "podserv-b",
            "-c",
            "/tmp/my.toml",
            "-m",
            "/data",
            "-b",
            "0.0.0.0:8080",
        ])
        .unwrap();
        assert_eq!(cli.config, "/tmp/my.toml");
        assert_eq!(cli.media, "/data");
        assert_eq!(cli.bind, "0.0.0.0:8080");
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
    async fn art_returns_correct_content_type() {
        let mut map = ArtMap::new();
        map.insert(
            "art.mp3".to_string(),
            ("image/jpeg".to_string(), vec![0xFF, 0xD8, 0xFF]),
        );
        let app =
            aw_test::init_service(App::new().app_data(web::Data::new(map)).service(art)).await;
        let req = aw_test::TestRequest::get().uri("/art/art.mp3").to_request();
        let resp = aw_test::call_service(&app, req).await;
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/jpeg");
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

    // --- RealIpKeyExtractor ---

    #[test]
    fn real_ip_extractor_uses_x_real_ip_when_peer_is_localhost() {
        let req = aw_test::TestRequest::get()
            .peer_addr("127.0.0.1:1234".parse().unwrap())
            .insert_header(("X-Real-IP", "203.0.113.42"))
            .to_srv_request();
        let key = RealIpKeyExtractor.extract(&req).unwrap();
        assert_eq!(key, "203.0.113.42".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn real_ip_extractor_falls_back_to_peer_when_no_header() {
        let req = aw_test::TestRequest::get()
            .peer_addr("127.0.0.1:1234".parse().unwrap())
            .to_srv_request();
        let key = RealIpKeyExtractor.extract(&req).unwrap();
        assert_eq!(key, "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn real_ip_extractor_ignores_x_real_ip_for_non_localhost_peer() {
        // A direct (non-proxy) connection must not trust a client-supplied X-Real-IP.
        let req = aw_test::TestRequest::get()
            .peer_addr("10.0.0.1:5678".parse().unwrap())
            .insert_header(("X-Real-IP", "1.2.3.4"))
            .to_srv_request();
        let key = RealIpKeyExtractor.extract(&req).unwrap();
        assert_eq!(key, "10.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn real_ip_extractor_uses_x_real_ip_when_peer_is_ipv6_localhost() {
        let req = aw_test::TestRequest::get()
            .peer_addr("[::1]:1234".parse().unwrap())
            .insert_header(("X-Real-IP", "203.0.113.42"))
            .to_srv_request();
        let key = RealIpKeyExtractor.extract(&req).unwrap();
        assert_eq!(key, "203.0.113.42".parse::<IpAddr>().unwrap());
    }

    // --- rss_feed handler ---

    #[actix_web::test]
    async fn rss_returns_200_with_xml_content_type() {
        let xml = rss::render_rss(&Config::default(), &[]);
        let etag = compute_etag(&xml);
        let cache = web::Data::new(RssCache { xml, etag });
        let app = aw_test::init_service(App::new().app_data(cache).service(rss_feed)).await;
        let req = aw_test::TestRequest::get().uri("/rss").to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "application/rss+xml; charset=utf-8");
    }

    #[actix_web::test]
    async fn rss_returns_304_on_matching_etag() {
        let xml = rss::render_rss(&Config::default(), &[]);
        let etag = compute_etag(&xml);
        let cache = web::Data::new(RssCache {
            xml,
            etag: etag.clone(),
        });
        let app = aw_test::init_service(App::new().app_data(cache).service(rss_feed)).await;
        let req = aw_test::TestRequest::get()
            .uri("/rss")
            .insert_header((header::IF_NONE_MATCH, etag))
            .to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 304);
    }

    #[actix_web::test]
    async fn rss_returns_cache_control_no_cache() {
        let xml = rss::render_rss(&Config::default(), &[]);
        let etag = compute_etag(&xml);
        let cache = web::Data::new(RssCache { xml, etag });
        let app = aw_test::init_service(App::new().app_data(cache).service(rss_feed)).await;
        let req = aw_test::TestRequest::get().uri("/rss").to_request();
        let resp = aw_test::call_service(&app, req).await;
        let cc = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(cc, "no-cache");
    }
}
