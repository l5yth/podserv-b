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

//! HTML page generation for the web interface.

use crate::config::Config;
use crate::media::Section;

/// Renders the complete HTML page for the given configuration and sections.
///
/// The page includes:
/// - A `<header>` with the site title and description.
/// - One `<section>` per [`Section`], each with a heading, episode count,
///   and episode rows containing album art, metadata, and a download link.
/// - A fixed bottom player bar showing cover art and "artist – title".
/// - A `<footer>` linking to the configured website and the project repository.
///
/// All user-supplied strings are HTML-escaped. Episode filenames in the
/// embedded JavaScript are URL-encoded per segment to handle spaces and
/// other special characters.
pub fn render_page(config: &Config, sections: &[Section]) -> String {
    // Flat arrays for the JavaScript player (global indices across all sections)
    let all_rel_paths: Vec<&str> = sections
        .iter()
        .flat_map(|s| s.episodes.iter().map(|e| e.rel_path.as_str()))
        .collect();
    let all_titles: Vec<&str> = sections
        .iter()
        .flat_map(|s| s.episodes.iter().map(|e| e.title.as_str()))
        .collect();
    let all_artists: Vec<&str> = sections
        .iter()
        .flat_map(|s| s.episodes.iter().map(|e| e.artist.as_str()))
        .collect();
    let all_has_art: Vec<bool> = sections
        .iter()
        .flat_map(|s| s.episodes.iter().map(|e| e.art.is_some()))
        .collect();
    let files_json = serde_json::to_string(&all_rel_paths).unwrap();
    let titles_json = serde_json::to_string(&all_titles).unwrap();
    let artists_json = serde_json::to_string(&all_artists).unwrap();
    let has_art_json = serde_json::to_string(&all_has_art).unwrap();
    let total = all_rel_paths.len();

    // Render each section
    let mut sections_html = String::new();
    let mut global_idx: usize = 0;
    for section in sections {
        let count = section.episodes.len();
        let s = if count == 1 { "" } else { "s" };
        let mut rows = String::new();
        for ep in &section.episodes {
            let enc = url_encode_path(&ep.rel_path);
            let art_tag = if ep.art.is_some() {
                format!(r#"<img src="/art/{enc}" alt="" loading="lazy">"#)
            } else {
                String::new()
            };
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
                r#"<div class="ep" onclick="play({i})"><div class="ep-art">{art}</div><div class="ep-body"><div class="ep-title">{title}</div><div class="ep-meta">{meta}{dur} — {size} MB</div></div><a class="dl-btn" href="/media/{enc}" download title="Download">&#8595;</a></div>"#,
                i = global_idx,
                art = art_tag,
                title = html_escape(&ep.title),
                meta = html_escape(&meta),
                dur = html_escape(&dur),
                size = ep.size_mb,
                enc = enc,
            ));
            global_idx += 1;
        }
        sections_html.push_str(&format!(
            r#"<section><h2 class="sh">{heading}</h2><div class="count">{count} episode{s}</div><div class="ep-list">{rows}</div></section>"#,
            heading = html_escape(&section.heading),
            count = count,
            s = s,
            rows = rows,
        ));
    }

    let title_esc = html_escape(config.title());
    let desc_esc = html_escape(config.description());
    let website_esc = html_escape(config.website());

    // First episode (alphabetically) that has embedded art → favicon.
    let favicon_tag = sections
        .iter()
        .flat_map(|s| s.episodes.iter())
        .find(|e| e.art.is_some())
        .map(|e| {
            let (mime, _) = e.art.as_ref().unwrap();
            let enc = url_encode_path(&e.rel_path);
            format!(r#"<link rel="icon" type="{mime}" href="/art/{enc}">"#)
        })
        .unwrap_or_default();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title_esc}</title>
{favicon_tag}<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:monospace;background:#111;color:#ccc;padding:1rem 1rem 5rem;max-width:640px;margin:0 auto}}
header{{border-bottom:1px solid #333;padding-bottom:.6rem;margin-bottom:1rem}}
h1{{font-size:1rem;color:#ccc}}
.desc{{font-size:.75rem;color:#555;margin-top:.25rem}}
h2.sh{{font-size:.8rem;color:#888;margin:1rem 0 .3rem;text-transform:uppercase;letter-spacing:.05em}}
.count{{font-size:.7rem;color:#555;margin-bottom:.4rem}}
.ep{{display:flex;align-items:center;gap:.5rem;padding:.4rem .2rem;border-bottom:1px solid #1a1a1a;cursor:pointer}}
.ep:hover{{background:#1a1a1a}}
.ep.active{{background:#1a1a1a;color:#fff}}
.ep-art{{width:2.5rem;height:2.5rem;flex-shrink:0;background:#1a1a1a;overflow:hidden}}
.ep-art img{{width:100%;height:100%;object-fit:cover}}
.ep-body{{flex:1;min-width:0}}
.ep-title{{font-size:.85rem;color:#ddd;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
.ep-meta{{font-size:.7rem;color:#666;margin-top:.1rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
.dl-btn{{width:2.5rem;height:2.5rem;display:flex;align-items:center;justify-content:center;color:#444;text-decoration:none;flex-shrink:0;font-size:.9rem}}
.dl-btn:hover{{color:#999;background:#1a1a1a}}
#player-bar{{position:fixed;bottom:0;left:0;right:0;background:#0a0a0a;border-top:1px solid #222;padding:.6rem 1rem;display:none;font-size:.75rem}}
#player-inner{{display:flex;align-items:center;gap:.75rem}}
#player-art{{width:2.5rem;height:2.5rem;flex-shrink:0;background:#1a1a1a;overflow:hidden}}
#player-art img{{width:100%;height:100%;object-fit:cover;display:block}}
#player-info{{flex:1;min-width:0}}
#player-bar .now{{color:#999;margin-bottom:.3rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
#player-bar audio{{width:100%;height:28px;filter:grayscale(1)}}
footer{{font-size:.7rem;color:#555;margin-top:2rem;padding-top:.6rem;border-top:1px solid #222}}
footer a{{color:#555;text-decoration:none}}
footer a:hover{{color:#aaa}}
</style>
</head>
<body>
<header>
<h1>{title_esc}</h1>
<p class="desc">{desc_esc}</p>
</header>
<main>{sections_html}</main>
<footer><a href="{website_esc}">{website_esc}</a> &middot; powered by <a href="https://github.com/l5yth/podserv-b">podserv-b</a> (v{version})</footer>
<div id="player-bar">
  <div id="player-inner">
    <div id="player-art"></div>
    <div id="player-info">
      <div class="now" id="now"></div>
      <audio id="audio" controls preload="none"></audio>
    </div>
  </div>
</div>
<script>
const files={files_json};
const titles={titles_json};
const artists={artists_json};
const hasArt={has_art_json};
const total={total};
let cur=-1;
const audio=document.getElementById('audio');
const bar=document.getElementById('player-bar');
const now=document.getElementById('now');
const playerArt=document.getElementById('player-art');
function encodeRelPath(p){{return p.split('/').map(encodeURIComponent).join('/');}}
function play(i){{
  if(cur>=0)document.querySelectorAll('.ep')[cur].classList.remove('active');
  cur=i;
  document.querySelectorAll('.ep')[cur].classList.add('active');
  audio.src='/media/'+encodeRelPath(files[i]);
  audio.play();
  const a=artists[i],t=titles[i];
  now.textContent=a?a+' \u2013 '+t:t;
  if(hasArt[i]){{
    playerArt.innerHTML='<img src="/art/'+encodeRelPath(files[i])+'" alt="">';
  }}else{{
    playerArt.innerHTML='';
  }}
  bar.style.display='block';
}}
audio.addEventListener('ended',()=>{{if(cur<total-1)play(cur+1);}});
</script>
</body>
</html>"#,
        title_esc = title_esc,
        desc_esc = desc_esc,
        website_esc = website_esc,
        sections_html = sections_html,
        files_json = files_json,
        titles_json = titles_json,
        artists_json = artists_json,
        has_art_json = has_art_json,
        total = total,
        version = env!("CARGO_PKG_VERSION"),
        favicon_tag = favicon_tag,
    )
}

/// Escapes HTML special characters to prevent XSS.
///
/// Replaces `&`, `<`, `>`, and `"` with their HTML entity equivalents.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Percent-encodes each `/`-separated segment of a relative path, preserving
/// the slashes.
///
/// Characters that are safe in URL path segments (`A–Z`, `a–z`, `0–9`, `-`,
/// `.`, `_`, `~`) are left unchanged; all other bytes are percent-encoded.
///
/// `"my show/ep 1.mp3"` → `"my%20show/ep%201.mp3"`
pub fn url_encode_path(rel_path: &str) -> String {
    rel_path
        .split('/')
        .map(url_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// Percent-encodes a single URL path segment.
fn url_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::media::{Episode, Section};

    fn default_config() -> Config {
        Config::default()
    }

    #[allow(clippy::too_many_arguments)]
    fn make_ep(
        rel_path: &str,
        title: &str,
        artist: &str,
        album: &str,
        year: &str,
        duration: &str,
        size_mb: &str,
        has_art: bool,
    ) -> Episode {
        Episode {
            rel_path: rel_path.to_string(),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            year: year.into(),
            duration: duration.into(),
            size_mb: size_mb.into(),
            art: if has_art {
                Some(("image/jpeg".into(), vec![]))
            } else {
                None
            },
        }
    }

    fn section(heading: &str, episodes: Vec<Episode>) -> Section {
        Section {
            heading: heading.to_string(),
            episodes,
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

    // --- url_encode_path ---

    #[test]
    fn url_encode_plain_ascii_unchanged() {
        assert_eq!(url_encode_path("track.mp3"), "track.mp3");
    }

    #[test]
    fn url_encode_space_encoded() {
        assert_eq!(url_encode_path("my show.mp3"), "my%20show.mp3");
    }

    #[test]
    fn url_encode_preserves_slash() {
        assert_eq!(url_encode_path("a/b.mp3"), "a/b.mp3");
    }

    #[test]
    fn url_encode_slash_with_spaces() {
        assert_eq!(
            url_encode_path("my shows/ep 1.mp3"),
            "my%20shows/ep%201.mp3"
        );
    }

    #[test]
    fn url_encode_safe_chars_unchanged() {
        assert_eq!(url_encode_path("a-b_c~d.mp3"), "a-b_c~d.mp3");
    }

    // --- render_page: header / footer ---

    #[test]
    fn render_header_contains_title() {
        let cfg = toml::from_str::<Config>(r#"title = "My Radio""#).unwrap();
        let html = render_page(&cfg, &[]);
        assert!(html.contains("<h1>My Radio</h1>"));
        assert!(html.contains("<title>My Radio</title>"));
    }

    #[test]
    fn render_header_contains_description() {
        let cfg = toml::from_str::<Config>(r#"description = "Great shows""#).unwrap();
        let html = render_page(&cfg, &[]);
        assert!(html.contains("Great shows"));
    }

    #[test]
    fn render_footer_contains_website() {
        let cfg = toml::from_str::<Config>(r#"website = "https://mysite.example""#).unwrap();
        let html = render_page(&cfg, &[]);
        assert!(html.contains("https://mysite.example"));
    }

    #[test]
    fn render_footer_contains_default_website() {
        let html = render_page(&default_config(), &[]);
        assert!(html.contains("github.com/l5yth/podserv-b"));
    }

    #[test]
    fn render_footer_website_href_html_escaped() {
        // & in the URL must appear as &amp; in the href attribute to be valid HTML
        let cfg = toml::from_str::<Config>(r#"website = "https://example.org/?a=1&b=2""#).unwrap();
        let html = render_page(&cfg, &[]);
        assert!(html.contains(r#"href="https://example.org/?a=1&amp;b=2""#));
        assert!(!html.contains(r#"href="https://example.org/?a=1&b=2""#));
    }

    #[test]
    fn render_footer_contains_powered_by_link() {
        let html = render_page(&default_config(), &[]);
        assert!(html.contains("powered by"));
        assert!(html.contains(r#"href="https://github.com/l5yth/podserv-b">podserv-b</a>"#));
        assert!(html.contains(&format!("(v{})", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn render_header_uses_default_title_when_unset() {
        let html = render_page(&default_config(), &[]);
        assert!(html.contains("<h1>podserv-b</h1>"));
    }

    // --- render_page: favicon ---

    #[test]
    fn render_favicon_absent_when_no_sections() {
        let html = render_page(&default_config(), &[]);
        assert!(!html.contains(r#"rel="icon""#));
    }

    #[test]
    fn render_favicon_absent_when_no_episode_has_art() {
        let sec = section(
            "podcasts",
            vec![
                make_ep("a.mp3", "A", "", "", "", "", "1.0", false),
                make_ep("b.mp3", "B", "", "", "", "", "1.0", false),
            ],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(!html.contains(r#"rel="icon""#));
    }

    #[test]
    fn render_favicon_uses_first_alphabetical_episode_with_art() {
        // "a-shows" section comes first alphabetically; "b.mp3" has art within it.
        // "z-shows" section has art at "a.mp3" but comes after.
        // Expect the favicon to point to a-shows/b.mp3.
        let sec_a = section(
            "a-shows",
            vec![
                make_ep("a-shows/a.mp3", "A", "", "", "", "", "1.0", false),
                make_ep("a-shows/b.mp3", "B", "", "", "", "", "1.0", true),
            ],
        );
        let sec_z = section(
            "z-shows",
            vec![make_ep("z-shows/a.mp3", "Z", "", "", "", "", "1.0", true)],
        );
        let html = render_page(&default_config(), &[sec_a, sec_z]);
        // The favicon link must point to the first alphabetical episode with art.
        assert!(html.contains(r#"<link rel="icon" type="image/jpeg" href="/art/a-shows/b.mp3">"#));
        // z-shows/a.mp3 appears in the episode row <img> but NOT as the favicon href.
        assert!(!html.contains(r#"<link rel="icon" type="image/jpeg" href="/art/z-shows/a.mp3">"#));
    }

    // --- render_page: sections ---

    #[test]
    fn render_section_heading_appears() {
        let sec = section(
            "radio-shows",
            vec![make_ep("a.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("radio-shows"));
    }

    #[test]
    fn render_one_episode_singular() {
        let sec = section(
            "podcasts",
            vec![make_ep("a.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("1 episode<"));
        assert!(!html.contains("1 episodes"));
    }

    #[test]
    fn render_multiple_episodes_plural() {
        let sec = section(
            "podcasts",
            vec![
                make_ep("a.mp3", "A", "", "", "", "", "1.0", false),
                make_ep("b.mp3", "B", "", "", "", "", "2.0", false),
            ],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("2 episodes"));
    }

    #[test]
    fn render_zero_sections_produces_valid_html() {
        let html = render_page(&default_config(), &[]);
        assert!(html.starts_with("<!DOCTYPE html>"));
    }

    // --- render_page: episode row ---

    #[test]
    fn render_duration_shown() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "T", "", "", "", "3:45", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("[3:45]"));
    }

    #[test]
    fn render_empty_duration_hidden() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(!html.contains("[]"));
    }

    #[test]
    fn render_full_meta_joined() {
        let sec = section(
            "p",
            vec![make_ep(
                "a.mp3", "T", "Art", "Alb", "2024", "", "1.0", false,
            )],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("Art"));
        assert!(html.contains("Alb"));
        assert!(html.contains("2024"));
    }

    #[test]
    fn render_partial_meta_filters_empty_fields() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "T", "Art", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("Art"));
        assert!(!html.contains(" · ·"));
    }

    #[test]
    fn render_title_html_escaped() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "<b>", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("&lt;b&gt;"));
    }

    #[test]
    fn render_meta_html_escaped() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "T", "A&B", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("A&amp;B"));
    }

    #[test]
    fn render_duration_html_escaped() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "T", "", "", "", "1<2", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("[1&lt;2]"));
    }

    // --- render_page: art and download ---

    #[test]
    fn render_art_img_present_when_has_art() {
        let sec = section(
            "p",
            vec![make_ep("ep.mp3", "T", "", "", "", "", "1.0", true)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains(r#"src="/art/ep.mp3""#));
    }

    #[test]
    fn render_art_img_absent_when_no_art() {
        let sec = section(
            "p",
            vec![make_ep("ep.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(!html.contains("/art/ep.mp3"));
    }

    #[test]
    fn render_art_url_encoded() {
        let sec = section(
            "p",
            vec![make_ep("my show.mp3", "T", "", "", "", "", "1.0", true)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains(r#"src="/art/my%20show.mp3""#));
    }

    #[test]
    fn render_download_button_present() {
        let sec = section(
            "p",
            vec![make_ep("ep.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains(r#"href="/media/ep.mp3""#));
        assert!(html.contains("download"));
    }

    #[test]
    fn render_download_url_encoded() {
        let sec = section(
            "p",
            vec![make_ep("my show.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains(r#"href="/media/my%20show.mp3""#));
    }

    #[test]
    fn render_download_url_with_subdir() {
        let sec = section(
            "p",
            vec![make_ep(
                "podcasts/my show.mp3",
                "T",
                "",
                "",
                "",
                "",
                "1.0",
                false,
            )],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains(r#"href="/media/podcasts/my%20show.mp3""#));
    }

    // --- render_page: JavaScript arrays ---

    #[test]
    fn render_files_in_js() {
        let sec = section(
            "p",
            vec![make_ep("my file.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("\"my file.mp3\""));
    }

    #[test]
    fn render_titles_in_js() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "My Title", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("\"My Title\""));
    }

    #[test]
    fn render_artists_in_js() {
        let sec = section(
            "p",
            vec![make_ep(
                "a.mp3",
                "T",
                "Cool Artist",
                "",
                "",
                "",
                "1.0",
                false,
            )],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("\"Cool Artist\""));
    }

    #[test]
    fn render_has_art_true_in_js() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "T", "", "", "", "", "1.0", true)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("const hasArt=[true]"));
    }

    #[test]
    fn render_has_art_false_in_js() {
        let sec = section(
            "p",
            vec![make_ep("a.mp3", "T", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[sec]);
        assert!(html.contains("const hasArt=[false]"));
    }

    #[test]
    fn render_player_bar_has_art_element() {
        let html = render_page(&default_config(), &[]);
        assert!(html.contains(r#"id="player-art""#));
    }

    #[test]
    fn render_player_bar_has_info_element() {
        let html = render_page(&default_config(), &[]);
        assert!(html.contains(r#"id="player-info""#));
    }

    #[test]
    fn render_play_indices_span_sections() {
        // Two sections; second section episode should get global index 1
        let s1 = section(
            "a",
            vec![make_ep("a.mp3", "A", "", "", "", "", "1.0", false)],
        );
        let s2 = section(
            "b",
            vec![make_ep("b.mp3", "B", "", "", "", "", "1.0", false)],
        );
        let html = render_page(&default_config(), &[s1, s2]);
        assert!(html.contains("onclick=\"play(0)\""));
        assert!(html.contains("onclick=\"play(1)\""));
    }
}
