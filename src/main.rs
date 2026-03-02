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

use actix_files::Files;
use actix_web::{App, HttpResponse, HttpServer, get, web};
use id3::{Tag, TagLike};
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
struct Episode {
    filename: String,
    title: String,
    artist: String,
    album: String,
    year: String,
    duration: String,
    size_mb: String,
}

fn scan_media(dir: &str) -> Vec<Episode> {
    let mut episodes = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        eprintln!("warning: cannot read {dir}");
        return episodes;
    };

    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    paths.sort_by_key(|e| e.file_name());

    for entry in paths {
        let path = entry.path();
        let Some(ext) = path.extension() else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("mp3") {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let size_mb = fs::metadata(&path)
            .map(|m| format!("{:.1}", m.len() as f64 / (1024.0 * 1024.0)))
            .unwrap_or_default();

        let (title, artist, album, year, duration) = match Tag::read_from_path(&path) {
            Ok(tag) => {
                let t = tag.title().unwrap_or(&filename).to_string();
                let a = tag.artist().unwrap_or("Unknown").to_string();
                let al = tag.album().unwrap_or("").to_string();
                let y = tag.year().map(|y| y.to_string()).unwrap_or_default();
                let d = tag
                    .duration()
                    .map(|ms| {
                        let s = ms / 1000;
                        format!("{}:{:02}", s / 60, s % 60)
                    })
                    .unwrap_or_default();
                (t, a, al, y, d)
            }
            Err(_) => (
                filename.clone(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };

        episodes.push(Episode {
            filename,
            title,
            artist,
            album,
            year,
            duration,
            size_mb,
        });
    }
    episodes
}

fn render_page(episodes: &[Episode]) -> String {
    let mut rows = String::new();
    for (i, ep) in episodes.iter().enumerate() {
        let meta_parts: Vec<&str> = [ep.artist.as_str(), ep.album.as_str(), ep.year.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        let meta = meta_parts.join(" \u{00b7} ");
        let dur = if ep.duration.is_empty() {
            String::new()
        } else {
            format!(" [{}]", ep.duration)
        };

        rows.push_str(&format!(
            r#"<div class="ep" onclick="play({i})">
  <div class="ep-title">{title}</div>
  <div class="ep-meta">{meta}{dur} — {size} MB</div>
</div>"#,
            i = i,
            title = html_escape(&ep.title),
            meta = html_escape(&meta),
            dur = html_escape(&dur),
            size = ep.size_mb,
        ));
    }

    let filenames_json =
        serde_json::to_string(&episodes.iter().map(|e| &e.filename).collect::<Vec<_>>()).unwrap();
    let titles_json =
        serde_json::to_string(&episodes.iter().map(|e| &e.title).collect::<Vec<_>>()).unwrap();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>podcasts</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:monospace;background:#111;color:#ccc;padding:1rem 1rem 5rem;max-width:640px;margin:0 auto}}
h1{{font-size:1rem;color:#888;border-bottom:1px solid #333;padding-bottom:.4rem;margin-bottom:.5rem}}
.ep{{padding:.5rem .4rem;border-bottom:1px solid #1a1a1a;cursor:pointer}}
.ep:hover{{background:#1a1a1a}}
.ep.active{{background:#1a1a1a;color:#fff}}
.ep-title{{font-size:.85rem;color:#ddd}}
.ep-meta{{font-size:.7rem;color:#666;margin-top:.15rem}}
#player-bar{{position:fixed;bottom:0;left:0;right:0;background:#0a0a0a;border-top:1px solid #222;padding:.6rem 1rem;display:none;font-size:.75rem}}
#player-bar .now{{color:#999;margin-bottom:.3rem}}
#player-bar audio{{width:100%;height:28px;filter:grayscale(1)}}
.count{{font-size:.7rem;color:#555;margin-bottom:.6rem}}
</style>
</head>
<body>
<h1>&#9632; podcasts</h1>
<div class="count">{count} episode{s}</div>
<div id="list">{rows}</div>
<div id="player-bar">
  <div class="now" id="now"></div>
  <audio id="audio" controls preload="none"></audio>
</div>
<script>
const files={filenames};
const titles={titles};
let cur=-1;
const audio=document.getElementById('audio');
const bar=document.getElementById('player-bar');
const now=document.getElementById('now');
function play(i){{
  if(cur>=0)document.querySelectorAll('.ep')[cur].classList.remove('active');
  cur=i;
  document.querySelectorAll('.ep')[cur].classList.add('active');
  audio.src='/media/'+encodeURIComponent(files[i]);
  audio.play();
  now.textContent=titles[i];
  bar.style.display='block';
}}
audio.addEventListener('ended',()=>{{if(cur<files.length-1)play(cur+1);}});
</script>
</body>
</html>"#,
        count = episodes.len(),
        s = if episodes.len() == 1 { "" } else { "s" },
        rows = rows,
        filenames = filenames_json,
        titles = titles_json,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[get("/")]
async fn index(data: web::Data<Vec<Episode>>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(render_page(&data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use id3::{Tag, TagLike, Version};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn new_temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("podserv_test_{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ep(
        filename: &str,
        title: &str,
        artist: &str,
        album: &str,
        year: &str,
        duration: &str,
        size_mb: &str,
    ) -> Episode {
        Episode {
            filename: filename.into(),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            year: year.into(),
            duration: duration.into(),
            size_mb: size_mb.into(),
        }
    }

    // --- html_escape ---

    #[test]
    fn escape_empty() {
        assert_eq!(html_escape(""), "");
    }

    #[test]
    fn escape_no_special_chars() {
        assert_eq!(html_escape("hello world"), "hello world");
    }

    #[test]
    fn escape_ampersand() {
        assert_eq!(html_escape("a&b"), "a&amp;b");
    }

    #[test]
    fn escape_lt() {
        assert_eq!(html_escape("<"), "&lt;");
    }

    #[test]
    fn escape_gt() {
        assert_eq!(html_escape(">"), "&gt;");
    }

    #[test]
    fn escape_quote() {
        assert_eq!(html_escape("\"x\""), "&quot;x&quot;");
    }

    #[test]
    fn escape_all_special_chars() {
        assert_eq!(
            html_escape("<a href=\"x&y\">"),
            "&lt;a href=&quot;x&amp;y&quot;&gt;"
        );
    }

    // --- render_page ---

    #[test]
    fn render_zero_episodes() {
        assert!(render_page(&[]).contains("0 episodes"));
    }

    #[test]
    fn render_one_episode_singular() {
        let html = render_page(&[ep("a.mp3", "T", "", "", "", "", "1.0")]);
        assert!(html.contains("1 episode<"));
        assert!(!html.contains("1 episodes"));
    }

    #[test]
    fn render_multiple_episodes_plural() {
        let html = render_page(&[
            ep("a.mp3", "A", "", "", "", "", "1.0"),
            ep("b.mp3", "B", "", "", "", "", "2.0"),
        ]);
        assert!(html.contains("2 episodes"));
    }

    #[test]
    fn render_duration_shown() {
        let html = render_page(&[ep("a.mp3", "T", "", "", "", "3:45", "1.0")]);
        assert!(html.contains("[3:45]"));
    }

    #[test]
    fn render_empty_duration_hidden() {
        let html = render_page(&[ep("a.mp3", "T", "", "", "", "", "1.0")]);
        assert!(!html.contains("[]"));
    }

    #[test]
    fn render_full_meta_joined() {
        let html = render_page(&[ep("a.mp3", "T", "Art", "Alb", "2024", "", "1.0")]);
        assert!(html.contains("Art"));
        assert!(html.contains("Alb"));
        assert!(html.contains("2024"));
    }

    #[test]
    fn render_partial_meta_filters_empty_fields() {
        // album and year are empty; only artist should appear, no stray separators
        let html = render_page(&[ep("a.mp3", "T", "Art", "", "", "", "1.0")]);
        assert!(html.contains("Art"));
        assert!(!html.contains(" · ·"));
    }

    #[test]
    fn render_title_html_escaped() {
        let html = render_page(&[ep("a.mp3", "<b>", "", "", "", "", "1.0")]);
        assert!(html.contains("&lt;b&gt;"));
    }

    #[test]
    fn render_meta_html_escaped() {
        let html = render_page(&[ep("a.mp3", "T", "A&B", "", "", "", "1.0")]);
        assert!(html.contains("A&amp;B"));
    }

    #[test]
    fn render_duration_html_escaped() {
        let html = render_page(&[ep("a.mp3", "T", "", "", "", "1<2", "1.0")]);
        assert!(html.contains("[1&lt;2]"));
    }

    #[test]
    fn render_filename_embedded_in_js() {
        let html = render_page(&[ep("my file.mp3", "T", "", "", "", "", "1.0")]);
        assert!(html.contains("\"my file.mp3\""));
    }

    #[test]
    fn render_title_embedded_in_js() {
        let html = render_page(&[ep("a.mp3", "My Title", "", "", "", "", "1.0")]);
        assert!(html.contains("\"My Title\""));
    }

    // --- scan_media ---

    #[test]
    fn scan_missing_dir_returns_empty() {
        assert!(scan_media("/no/such/path/podserv_test").is_empty());
    }

    #[test]
    fn scan_empty_dir_returns_empty() {
        let dir = new_temp_dir();
        assert!(scan_media(dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_skips_file_without_extension() {
        let dir = new_temp_dir();
        fs::write(dir.join("noext"), b"x").unwrap();
        assert!(scan_media(dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_skips_non_mp3_extension() {
        let dir = new_temp_dir();
        fs::write(dir.join("track.ogg"), b"x").unwrap();
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert!(scan_media(dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_case_insensitive_mp3_extension() {
        let dir = new_temp_dir();
        fs::write(dir.join("track.MP3"), b"x").unwrap();
        assert_eq!(scan_media(dir.to_str().unwrap()).len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_no_id3_tags_falls_back_to_filename() {
        let dir = new_temp_dir();
        fs::write(dir.join("ep.mp3"), b"not real mp3 data").unwrap();
        let episodes = scan_media(dir.to_str().unwrap());
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].filename, "ep.mp3");
        assert_eq!(episodes[0].title, "ep.mp3");
        assert!(episodes[0].artist.is_empty());
        assert!(episodes[0].album.is_empty());
        assert!(episodes[0].year.is_empty());
        assert!(episodes[0].duration.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_full_id3_tags() {
        let dir = new_temp_dir();
        let path = dir.join("tagged.mp3");
        let mut tag = Tag::new();
        tag.set_title("My Title");
        tag.set_artist("My Artist");
        tag.set_album("My Album");
        tag.set_year(2024);
        tag.set_duration(225_000); // 3 min 45 sec in ms → "3:45"
        fs::write(&path, []).unwrap();
        tag.write_to_path(&path, Version::Id3v23).unwrap();
        let episodes = scan_media(dir.to_str().unwrap());
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].title, "My Title");
        assert_eq!(episodes[0].artist, "My Artist");
        assert_eq!(episodes[0].album, "My Album");
        assert_eq!(episodes[0].year, "2024");
        assert_eq!(episodes[0].duration, "3:45");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_id3_missing_title_falls_back_to_filename() {
        let dir = new_temp_dir();
        let path = dir.join("notitle.mp3");
        let mut tag = Tag::new();
        tag.set_artist("Artist Only"); // no title set
        fs::write(&path, []).unwrap();
        tag.write_to_path(&path, Version::Id3v23).unwrap();
        let episodes = scan_media(dir.to_str().unwrap());
        assert_eq!(episodes[0].title, "notitle.mp3"); // unwrap_or(&filename)
        assert_eq!(episodes[0].artist, "Artist Only");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_id3_missing_optional_fields_use_defaults() {
        let dir = new_temp_dir();
        let path = dir.join("minimal.mp3");
        let mut tag = Tag::new();
        tag.set_title("Only Title"); // no artist, album, year, duration
        fs::write(&path, []).unwrap();
        tag.write_to_path(&path, Version::Id3v23).unwrap();
        let episodes = scan_media(dir.to_str().unwrap());
        assert_eq!(episodes[0].title, "Only Title");
        assert_eq!(episodes[0].artist, "Unknown"); // unwrap_or("Unknown")
        assert!(episodes[0].album.is_empty()); // unwrap_or("")
        assert!(episodes[0].year.is_empty()); // None → unwrap_or_default
        assert!(episodes[0].duration.is_empty()); // None → unwrap_or_default
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_sorted_by_filename() {
        let dir = new_temp_dir();
        fs::write(dir.join("c.mp3"), b"x").unwrap();
        fs::write(dir.join("a.mp3"), b"x").unwrap();
        fs::write(dir.join("b.mp3"), b"x").unwrap();
        let episodes = scan_media(dir.to_str().unwrap());
        assert_eq!(
            episodes.iter().map(|e| e.filename.as_str()).collect::<Vec<_>>(),
            ["a.mp3", "b.mp3", "c.mp3"]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    // --- index handler ---

    #[actix_web::test]
    async fn index_returns_200_with_html_body() {
        use actix_web::{test, App};
        let data = web::Data::new(Vec::<Episode>::new());
        let app = test::init_service(App::new().app_data(data).service(index)).await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);
        assert!(test::read_body(resp).await.starts_with(b"<!DOCTYPE html>"));
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

    let episodes = scan_media(&media_dir);
    eprintln!("{} episodes found in {media_dir}/", episodes.len());
    eprintln!("listening on http://{bind}");

    let data = web::Data::new(episodes);
    let media_dir_clone = media_dir.clone();

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(index)
            .service(Files::new("/media", &media_dir_clone))
    })
    .bind(&bind)?
    .run()
    .await
}
