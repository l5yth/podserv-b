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

//! RSS 2.0 + iTunes podcast feed generation.

use crate::config::Config;
use crate::media::Section;
use crate::render::url_encode_path;
use std::time::SystemTime;

/// Escapes XML special characters in `s`.
///
/// Replaces `&`, `<`, `>`, `"`, and `'` with their XML entity equivalents.
/// Must be applied to all user-supplied strings before interpolation into XML.
pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Converts a [`SystemTime`] to an RFC 2822 date string.
///
/// Produces the format `"Wed, 01 Jan 2025 12:00:00 +0000"` used by RSS
/// `<pubDate>` elements. Implemented without external crates by computing
/// calendar date from the Unix epoch offset.
pub(crate) fn format_pub_date(t: SystemTime) -> String {
    const DAY_NAMES: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTH_NAMES: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const DAYS_PER_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let total_days = secs / 86400;
    let time_of_day = secs % 86400;
    let dow = DAY_NAMES[(total_days % 7) as usize];

    // Compute year from total days since Unix epoch (1 Jan 1970).
    let mut year = 1970u32;
    let mut days_rem = total_days as u32;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days_rem < days_in_year {
            break;
        }
        days_rem -= days_in_year;
        year += 1;
    }

    // Compute month and day within the year.
    let mut month = 0usize;
    loop {
        let mut dim = DAYS_PER_MONTH[month];
        if month == 1 && is_leap_year(year) {
            dim += 1;
        }
        if days_rem < dim {
            break;
        }
        days_rem -= dim;
        month += 1;
    }
    let day = days_rem + 1;

    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    format!(
        "{dow}, {day:02} {mon} {year} {h:02}:{m:02}:{s:02} +0000",
        mon = MONTH_NAMES[month],
    )
}

/// Returns `true` if `year` is a leap year in the proleptic Gregorian calendar.
fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// Renders a complete RSS 2.0 + iTunes podcast feed for the given configuration
/// and episode sections.
///
/// The feed includes:
/// - Channel-level metadata from [`Config`]: title, description, link, language,
///   author, and explicit flag.
/// - A channel `<image>` and `<itunes:image>` pointing to the first available
///   cover art across all episodes (same selection logic as the HTML favicon).
/// - One `<item>` per episode with enclosure, iTunes duration, season, and
///   episode number.
///
/// **Season mapping**: each [`Section`] maps to one iTunes season (1-based).
/// Episodes within a section are numbered 1-based. The directory structure
/// therefore becomes the season/episode hierarchy:
/// - Root MP3s → Season 1
/// - `shows/` directory → Season 2 (or 1 if no root files)
/// - `shows/2024/` nested directory → Season N
///
/// All user-supplied strings are XML-escaped. URLs are percent-encoded using
/// [`url_encode_path`].
pub fn render_rss(config: &Config, sections: &[Section]) -> String {
    let base = xml_escape(config.base_url());
    let title = xml_escape(config.title());
    let desc = xml_escape(config.description());
    let lang = xml_escape(config.language());
    let explicit_str = if config.explicit() { "true" } else { "false" };
    let author = config.author();

    // Channel image: first episode (alphabetically) with embedded art.
    let channel_image = sections
        .iter()
        .flat_map(|s| s.episodes.iter())
        .find_map(|e| {
            e.art.as_ref().map(|_| {
                let enc = url_encode_path(&e.rel_path);
                format!("{base}/art/{enc}")
            })
        });

    let mut out = String::new();

    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(
        "<rss version=\"2.0\" xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\">\n",
    );
    out.push_str("<channel>\n");
    out.push_str(&format!("  <title>{title}</title>\n"));
    out.push_str(&format!("  <link>{base}</link>\n"));
    out.push_str(&format!("  <description>{desc}</description>\n"));
    out.push_str(&format!("  <language>{lang}</language>\n"));
    out.push_str(&format!(
        "  <itunes:explicit>{explicit_str}</itunes:explicit>\n"
    ));

    if !author.is_empty() {
        let author_esc = xml_escape(author);
        out.push_str(&format!(
            "  <managingEditor>{author_esc}</managingEditor>\n"
        ));
        out.push_str(&format!("  <itunes:author>{author_esc}</itunes:author>\n"));
    }

    if let Some(ref img_url) = channel_image {
        out.push_str(&format!(
            "  <image>\n    <url>{img_url}</url>\n    <title>{title}</title>\n    <link>{base}</link>\n  </image>\n"
        ));
        out.push_str(&format!("  <itunes:image href=\"{img_url}\"/>\n"));
    }

    for (season_idx, section) in sections.iter().enumerate() {
        let season = season_idx + 1;
        for (ep_idx, ep) in section.episodes.iter().enumerate() {
            let ep_num = ep_idx + 1;
            let enc = url_encode_path(&ep.rel_path);
            let media_url = format!("{base}/media/{enc}");
            let ep_title = xml_escape(&ep.title);
            let ep_artist = xml_escape(&ep.artist);

            // Build a human-readable description from available metadata.
            // "Unknown" is the media scanner's fallback for a missing artist tag;
            // suppress it here so the description stays clean.
            let mut desc_parts: Vec<String> = Vec::new();
            if !ep.artist.is_empty() && ep.artist != "Unknown" {
                desc_parts.push(ep.artist.clone());
            }
            if !ep.album.is_empty() {
                desc_parts.push(ep.album.clone());
            }
            if !ep.duration.is_empty() {
                desc_parts.push(format!("[{}]", ep.duration));
            }
            let ep_desc = xml_escape(&desc_parts.join(" \u{00b7} "));

            out.push_str("  <item>\n");
            out.push_str(&format!("    <title>{ep_title}</title>\n"));
            out.push_str(&format!(
                "    <guid isPermaLink=\"false\">{media_url}</guid>\n"
            ));
            if let Some(pub_date) = ep.pub_date {
                out.push_str(&format!(
                    "    <pubDate>{}</pubDate>\n",
                    format_pub_date(pub_date)
                ));
            }
            out.push_str(&format!(
                "    <enclosure url=\"{media_url}\" length=\"{}\" type=\"audio/mpeg\"/>\n",
                ep.size_bytes
            ));
            if !ep_desc.is_empty() {
                out.push_str(&format!("    <description>{ep_desc}</description>\n"));
            }
            if !ep.artist.is_empty() && ep.artist != "Unknown" {
                out.push_str(&format!("    <itunes:author>{ep_artist}</itunes:author>\n"));
            }
            if !ep.duration.is_empty() {
                out.push_str(&format!(
                    "    <itunes:duration>{}</itunes:duration>\n",
                    xml_escape(&ep.duration)
                ));
            }
            out.push_str(&format!("    <itunes:season>{season}</itunes:season>\n"));
            out.push_str(&format!("    <itunes:episode>{ep_num}</itunes:episode>\n"));
            if ep.art.is_some() {
                out.push_str(&format!("    <itunes:image href=\"{base}/art/{enc}\"/>\n"));
            }
            out.push_str("  </item>\n");
        }
    }

    out.push_str("</channel>\n</rss>\n");
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

    fn config_with_base(base: &str) -> Config {
        toml::from_str(&format!("base_url = \"{base}\"")).unwrap()
    }

    fn make_ep(rel_path: &str, has_art: bool) -> Episode {
        Episode {
            rel_path: rel_path.into(),
            title: "Episode Title".into(),
            artist: "Artist Name".into(),
            album: "Album Name".into(),
            year: "2024".into(),
            duration: "3:45".into(),
            size_mb: "12.3".into(),
            size_bytes: 12_900_000,
            pub_date: None,
            art: if has_art {
                Some(("image/jpeg".into(), vec![0xFF]))
            } else {
                None
            },
        }
    }

    fn section(heading: &str, episodes: Vec<Episode>) -> Section {
        Section {
            heading: heading.into(),
            episodes,
        }
    }

    // --- xml_escape ---

    #[test]
    fn xml_escape_ampersand() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
    }

    #[test]
    fn xml_escape_lt() {
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
    }

    #[test]
    fn xml_escape_gt() {
        assert_eq!(xml_escape("a>b"), "a&gt;b");
    }

    #[test]
    fn xml_escape_double_quote() {
        assert_eq!(xml_escape("say \"hi\""), "say &quot;hi&quot;");
    }

    #[test]
    fn xml_escape_single_quote() {
        assert_eq!(xml_escape("it's"), "it&apos;s");
    }

    #[test]
    fn xml_escape_clean_string_unchanged() {
        assert_eq!(xml_escape("hello world"), "hello world");
    }

    // --- format_pub_date ---

    #[test]
    fn format_pub_date_epoch() {
        assert_eq!(
            format_pub_date(SystemTime::UNIX_EPOCH),
            "Thu, 01 Jan 1970 00:00:00 +0000"
        );
    }

    #[test]
    fn format_pub_date_known_timestamp() {
        // 2024-01-15 12:30:45 UTC = 1705321845 seconds since epoch.
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_705_321_845);
        assert_eq!(format_pub_date(t), "Mon, 15 Jan 2024 12:30:45 +0000");
    }

    #[test]
    fn format_pub_date_leap_day() {
        // 2000-02-29 00:00:00 UTC = 951782400 seconds since epoch.
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(951_782_400);
        assert_eq!(format_pub_date(t), "Tue, 29 Feb 2000 00:00:00 +0000");
    }

    // --- render_rss: structure ---

    #[test]
    fn render_rss_has_xml_header() {
        let xml = render_rss(&default_config(), &[]);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    }

    #[test]
    fn render_rss_has_rss_element_with_itunes_ns() {
        let xml = render_rss(&default_config(), &[]);
        assert!(xml.contains("xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\""));
    }

    // --- render_rss: channel ---

    #[test]
    fn render_rss_channel_title() {
        let cfg: Config = toml::from_str("title = \"My Podcast\"").unwrap();
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<title>My Podcast</title>"));
    }

    #[test]
    fn render_rss_channel_description() {
        let cfg: Config = toml::from_str("description = \"Great shows\"").unwrap();
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<description>Great shows</description>"));
    }

    #[test]
    fn render_rss_language_default() {
        let xml = render_rss(&default_config(), &[]);
        assert!(xml.contains("<language>en</language>"));
    }

    #[test]
    fn render_rss_language_override() {
        let cfg: Config = toml::from_str("language = \"de\"").unwrap();
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<language>de</language>"));
    }

    #[test]
    fn render_rss_explicit_false_by_default() {
        let xml = render_rss(&default_config(), &[]);
        assert!(xml.contains("<itunes:explicit>false</itunes:explicit>"));
    }

    #[test]
    fn render_rss_explicit_true() {
        let cfg: Config = toml::from_str("explicit = true").unwrap();
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<itunes:explicit>true</itunes:explicit>"));
    }

    #[test]
    fn render_rss_author_omitted_when_empty() {
        let xml = render_rss(&default_config(), &[]);
        assert!(!xml.contains("managingEditor"));
        assert!(!xml.contains("itunes:author"));
    }

    #[test]
    fn render_rss_author_present_when_set() {
        let cfg: Config = toml::from_str("author = \"Jane Smith\"").unwrap();
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<managingEditor>Jane Smith</managingEditor>"));
        assert!(xml.contains("<itunes:author>Jane Smith</itunes:author>"));
    }

    #[test]
    fn render_rss_base_url_falls_back_to_website() {
        let cfg: Config = toml::from_str("website = \"https://example.org\"").unwrap();
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<link>https://example.org</link>"));
    }

    #[test]
    fn render_rss_base_url_override() {
        let cfg = config_with_base("https://pods.example.org");
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<link>https://pods.example.org</link>"));
    }

    #[test]
    fn render_rss_channel_description_special_chars_escaped() {
        let cfg: Config = toml::from_str("description = \"News & <Views>\"").unwrap();
        let xml = render_rss(&cfg, &[]);
        assert!(xml.contains("<description>News &amp; &lt;Views&gt;</description>"));
    }

    #[test]
    fn render_rss_unknown_artist_suppressed_from_itunes_author() {
        let mut ep = make_ep("ep.mp3", false);
        ep.artist = "Unknown".into();
        let sec = section("podcasts", vec![ep]);
        let xml = render_rss(&default_config(), &[sec]);
        assert!(!xml.contains("<itunes:author>"));
        assert!(!xml.contains("Unknown"));
    }

    // --- render_rss: channel image ---

    #[test]
    fn render_rss_channel_image_absent_when_no_art() {
        let sec = section("podcasts", vec![make_ep("ep.mp3", false)]);
        let xml = render_rss(&default_config(), &[sec]);
        assert!(!xml.contains("<image>"));
        assert!(!xml.contains("itunes:image"));
    }

    #[test]
    fn render_rss_channel_image_uses_first_art() {
        let cfg = config_with_base("https://pods.example.com");
        let sec = section(
            "podcasts",
            vec![make_ep("a.mp3", false), make_ep("b.mp3", true)],
        );
        let xml = render_rss(&cfg, &[sec]);
        assert!(xml.contains("<url>https://pods.example.com/art/b.mp3</url>"));
        assert!(xml.contains("itunes:image href=\"https://pods.example.com/art/b.mp3\""));
    }

    // --- render_rss: items ---

    #[test]
    fn render_rss_enclosure_url_and_type() {
        let cfg = config_with_base("https://pods.example.com");
        let sec = section("podcasts", vec![make_ep("show/ep.mp3", false)]);
        let xml = render_rss(&cfg, &[sec]);
        assert!(xml.contains("url=\"https://pods.example.com/media/show/ep.mp3\""));
        assert!(xml.contains("type=\"audio/mpeg\""));
    }

    #[test]
    fn render_rss_enclosure_length() {
        let cfg = config_with_base("https://pods.example.com");
        let sec = section("podcasts", vec![make_ep("ep.mp3", false)]);
        let xml = render_rss(&cfg, &[sec]);
        assert!(xml.contains("length=\"12900000\""));
    }

    #[test]
    fn render_rss_itunes_season_numbering() {
        let cfg = config_with_base("https://pods.example.com");
        let sec1 = section("podcasts", vec![make_ep("ep.mp3", false)]);
        let sec2 = section("shows", vec![make_ep("shows/ep.mp3", false)]);
        let xml = render_rss(&cfg, &[sec1, sec2]);
        // First section → season 1, second → season 2.
        let s1 = xml.find("<itunes:season>1</itunes:season>").unwrap();
        let s2 = xml.find("<itunes:season>2</itunes:season>").unwrap();
        assert!(s1 < s2);
    }

    #[test]
    fn render_rss_itunes_episode_numbering() {
        let cfg = config_with_base("https://pods.example.com");
        let sec = section(
            "podcasts",
            vec![make_ep("a.mp3", false), make_ep("b.mp3", false)],
        );
        let xml = render_rss(&cfg, &[sec]);
        let e1 = xml.find("<itunes:episode>1</itunes:episode>").unwrap();
        let e2 = xml.find("<itunes:episode>2</itunes:episode>").unwrap();
        assert!(e1 < e2);
    }

    #[test]
    fn render_rss_item_image_present_when_art() {
        let cfg = config_with_base("https://pods.example.com");
        let sec = section("podcasts", vec![make_ep("ep.mp3", true)]);
        let xml = render_rss(&cfg, &[sec]);
        // Channel-level image + item-level image — both should be present.
        assert_eq!(
            xml.matches("itunes:image href=\"https://pods.example.com/art/ep.mp3\"")
                .count(),
            2
        );
    }

    #[test]
    fn render_rss_item_image_absent_when_no_art() {
        let cfg = config_with_base("https://pods.example.com");
        let sec = section("podcasts", vec![make_ep("ep.mp3", false)]);
        let xml = render_rss(&cfg, &[sec]);
        assert!(!xml.contains("itunes:image"));
    }

    #[test]
    fn render_rss_pubdate_omitted_when_none() {
        let sec = section("podcasts", vec![make_ep("ep.mp3", false)]);
        let xml = render_rss(&default_config(), &[sec]);
        assert!(!xml.contains("<pubDate>"));
    }

    #[test]
    fn render_rss_pubdate_present_when_set() {
        let mut ep = make_ep("ep.mp3", false);
        ep.pub_date = Some(SystemTime::UNIX_EPOCH);
        let sec = section("podcasts", vec![ep]);
        let xml = render_rss(&default_config(), &[sec]);
        assert!(xml.contains("<pubDate>Thu, 01 Jan 1970 00:00:00 +0000</pubDate>"));
    }

    #[test]
    fn render_rss_special_chars_escaped_in_title() {
        let mut ep = make_ep("ep.mp3", false);
        ep.title = "Ep & <1>".into();
        let sec = section("podcasts", vec![ep]);
        let xml = render_rss(&default_config(), &[sec]);
        assert!(xml.contains("<title>Ep &amp; &lt;1&gt;</title>"));
    }

    #[test]
    fn render_rss_url_encoded_paths() {
        let cfg = config_with_base("https://pods.example.com");
        let sec = section("podcasts", vec![make_ep("my show/ep 1.mp3", false)]);
        let xml = render_rss(&cfg, &[sec]);
        assert!(xml.contains("my%20show/ep%201.mp3"));
        assert!(!xml.contains("my show/ep 1.mp3"));
    }
}
