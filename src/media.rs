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

//! Media scanning: discovers MP3 files and extracts their ID3 metadata.

use id3::{Tag, TagLike};
use std::fs;
use std::path::Path;

/// Metadata for a single MP3 episode.
#[derive(Debug, Clone)]
pub struct Episode {
    /// Path relative to the media directory (e.g. `"podcasts/my show.mp3"`).
    ///
    /// The last path component is the original filename. Used to build
    /// `/media/…` and `/art/…` URLs.
    pub rel_path: String,
    /// ID3 title tag, or the filename if the tag is absent.
    pub title: String,
    /// ID3 artist tag, or `"Unknown"` if absent.
    pub artist: String,
    /// ID3 album tag, or empty string if absent.
    pub album: String,
    /// ID3 year as a string, or empty string if absent.
    pub year: String,
    /// Duration formatted as `"M:SS"`, or empty string if absent.
    pub duration: String,
    /// File size in megabytes, formatted to one decimal place.
    pub size_mb: String,
    /// Whether the ID3 tag contains embedded cover art.
    pub has_art: bool,
}

/// A named group of episodes, corresponding to a media directory.
#[derive(Debug)]
pub struct Section {
    /// Display heading (e.g. `"podcasts"` or `"podcasts/2020"`).
    pub heading: String,
    /// Episodes in this section, sorted alphabetically by filename.
    pub episodes: Vec<Episode>,
}

/// Scans `media_dir` for MP3 files up to two directory levels deep and
/// returns a list of [`Section`]s.
///
/// Layout rules:
/// - MP3 files directly in `media_dir` → section heading `"podcasts"`.
/// - Files in a first-level subdirectory → heading = directory name.
/// - Files in a second-level subdirectory → heading = `"parent/child"`.
/// - Directories deeper than two levels are ignored.
///
/// Sections with no episodes are omitted. Sections and episodes within each
/// section are sorted alphabetically.
pub fn scan_sections(media_dir: &str) -> Vec<Section> {
    let root = Path::new(media_dir);
    let mut sections = Vec::new();

    // Root-level MP3s → "podcasts"
    let root_eps = scan_mp3s_in_dir(root, media_dir);
    if !root_eps.is_empty() {
        sections.push(Section {
            heading: "podcasts".into(),
            episodes: root_eps,
        });
    }

    // Level-1 subdirectories
    for dir1 in sorted_subdirs(root) {
        let path1 = root.join(&dir1);

        // Direct MP3s in this subdirectory → heading = directory name
        let eps1 = scan_mp3s_in_dir(&path1, media_dir);
        if !eps1.is_empty() {
            sections.push(Section {
                heading: dir1.clone(),
                episodes: eps1,
            });
        }

        // Level-2 subdirectories → heading = "dir1/dir2"
        for dir2 in sorted_subdirs(&path1) {
            let path2 = path1.join(&dir2);
            let eps2 = scan_mp3s_in_dir(&path2, media_dir);
            if !eps2.is_empty() {
                sections.push(Section {
                    heading: format!("{dir1}/{dir2}"),
                    episodes: eps2,
                });
            }
        }
    }

    sections
}

/// Returns the alphabetically sorted names of immediate subdirectories of `dir`.
///
/// Entries whose names are not valid UTF-8 are silently skipped.
fn sorted_subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// Scans `dir` for `.mp3` files (extension check is case-insensitive) and
/// returns a sorted [`Vec`] of [`Episode`]s.
///
/// `media_dir` is the root of the media tree; it is used to compute each
/// episode's [`Episode::rel_path`].
fn scan_mp3s_in_dir(dir: &Path, media_dir: &str) -> Vec<Episode> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    paths.sort_by_key(|e| e.file_name());

    let media_root = Path::new(media_dir);
    let mut episodes = Vec::new();

    for entry in paths {
        let path = entry.path();
        let Some(ext) = path.extension() else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("mp3") {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy().to_string(); // used as ID3 title fallback
        let rel_path = path
            .strip_prefix(media_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let size_mb = fs::metadata(&path)
            .map(|m| format!("{:.1}", m.len() as f64 / (1024.0 * 1024.0)))
            .unwrap_or_default();

        let (title, artist, album, year, duration, has_art) =
            match Tag::read_from_path(&path) {
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
                    let art = tag.pictures().next().is_some();
                    (t, a, al, y, d, art)
                }
                Err(_) => (
                    filename.clone(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    false,
                ),
            };

        episodes.push(Episode {
            rel_path,
            title,
            artist,
            album,
            year,
            duration,
            size_mb,
            has_art,
        });
    }
    episodes
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
        let dir = std::env::temp_dir().join(format!("podserv_media_test_{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // --- sorted_subdirs ---

    #[test]
    fn sorted_subdirs_empty_dir() {
        let dir = new_temp_dir();
        assert!(sorted_subdirs(&dir).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sorted_subdirs_returns_only_dirs_sorted() {
        let dir = new_temp_dir();
        fs::create_dir(dir.join("c")).unwrap();
        fs::create_dir(dir.join("a")).unwrap();
        fs::write(dir.join("file.txt"), b"x").unwrap(); // file, not dir — skipped
        let result = sorted_subdirs(&dir);
        assert_eq!(result, ["a", "c"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sorted_subdirs_missing_dir_returns_empty() {
        let dir = new_temp_dir();
        let missing = dir.join("no_such");
        assert!(sorted_subdirs(&missing).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    // --- scan_mp3s_in_dir ---

    #[test]
    fn scan_mp3s_missing_dir_returns_empty() {
        let dir = new_temp_dir();
        let missing = dir.join("nonexistent");
        assert!(scan_mp3s_in_dir(&missing, dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_empty_dir_returns_empty() {
        let dir = new_temp_dir();
        assert!(scan_mp3s_in_dir(&dir, dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_skips_no_extension() {
        let dir = new_temp_dir();
        fs::write(dir.join("noext"), b"x").unwrap();
        assert!(scan_mp3s_in_dir(&dir, dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_skips_non_mp3_extension() {
        let dir = new_temp_dir();
        fs::write(dir.join("track.ogg"), b"x").unwrap();
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert!(scan_mp3s_in_dir(&dir, dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_case_insensitive_extension() {
        let dir = new_temp_dir();
        fs::write(dir.join("track.MP3"), b"x").unwrap();
        assert_eq!(scan_mp3s_in_dir(&dir, dir.to_str().unwrap()).len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_no_id3_falls_back_to_filename() {
        let dir = new_temp_dir();
        fs::write(dir.join("ep.mp3"), b"not mp3 data").unwrap();
        let eps = scan_mp3s_in_dir(&dir, dir.to_str().unwrap());
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].rel_path, "ep.mp3");
        assert_eq!(eps[0].title, "ep.mp3");
        assert!(eps[0].artist.is_empty());
        assert!(!eps[0].has_art);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_full_id3_tags() {
        let dir = new_temp_dir();
        let path = dir.join("tagged.mp3");
        let mut tag = Tag::new();
        tag.set_title("My Title");
        tag.set_artist("My Artist");
        tag.set_album("My Album");
        tag.set_year(2024);
        tag.set_duration(225_000); // 3:45
        fs::write(&path, []).unwrap();
        tag.write_to_path(&path, Version::Id3v23).unwrap();
        let eps = scan_mp3s_in_dir(&dir, dir.to_str().unwrap());
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].rel_path, "tagged.mp3");
        assert_eq!(eps[0].title, "My Title");
        assert_eq!(eps[0].artist, "My Artist");
        assert_eq!(eps[0].album, "My Album");
        assert_eq!(eps[0].year, "2024");
        assert_eq!(eps[0].duration, "3:45");
        assert!(!eps[0].has_art);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_detects_cover_art() {
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
        let eps = scan_mp3s_in_dir(&dir, dir.to_str().unwrap());
        assert!(eps[0].has_art);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_missing_title_falls_back_to_filename() {
        let dir = new_temp_dir();
        let path = dir.join("notitle.mp3");
        let mut tag = Tag::new();
        tag.set_artist("Artist Only");
        fs::write(&path, []).unwrap();
        tag.write_to_path(&path, Version::Id3v23).unwrap();
        let eps = scan_mp3s_in_dir(&dir, dir.to_str().unwrap());
        assert_eq!(eps[0].rel_path, "notitle.mp3");
        assert_eq!(eps[0].title, "notitle.mp3"); // unwrap_or(&filename)
        assert_eq!(eps[0].artist, "Artist Only");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_missing_optional_fields_use_defaults() {
        let dir = new_temp_dir();
        let path = dir.join("minimal.mp3");
        let mut tag = Tag::new();
        tag.set_title("Only Title");
        fs::write(&path, []).unwrap();
        tag.write_to_path(&path, Version::Id3v23).unwrap();
        let eps = scan_mp3s_in_dir(&dir, dir.to_str().unwrap());
        assert_eq!(eps[0].artist, "Unknown"); // unwrap_or("Unknown")
        assert!(eps[0].album.is_empty()); // unwrap_or("")
        assert!(eps[0].year.is_empty()); // None → unwrap_or_default
        assert!(eps[0].duration.is_empty()); // None → unwrap_or_default
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_sorted_by_filename() {
        let dir = new_temp_dir();
        fs::write(dir.join("c.mp3"), b"x").unwrap();
        fs::write(dir.join("a.mp3"), b"x").unwrap();
        fs::write(dir.join("b.mp3"), b"x").unwrap();
        let eps = scan_mp3s_in_dir(&dir, dir.to_str().unwrap());
        assert_eq!(
            eps.iter().map(|e| e.rel_path.as_str()).collect::<Vec<_>>(),
            ["a.mp3", "b.mp3", "c.mp3"]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_mp3s_rel_path_includes_subdir() {
        let dir = new_temp_dir();
        let sub = dir.join("shows");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("ep.mp3"), b"x").unwrap();
        let eps = scan_mp3s_in_dir(&sub, dir.to_str().unwrap());
        assert_eq!(eps[0].rel_path, "shows/ep.mp3");
        fs::remove_dir_all(dir).unwrap();
    }

    // --- scan_sections ---

    #[test]
    fn scan_sections_missing_dir_returns_empty() {
        assert!(scan_sections("/no/such/path/podserv_b_test_xyz").is_empty());
    }

    #[test]
    fn scan_sections_empty_dir_no_sections() {
        let dir = new_temp_dir();
        assert!(scan_sections(dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_sections_flat_structure_heading_is_podcasts() {
        let dir = new_temp_dir();
        fs::write(dir.join("a.mp3"), b"x").unwrap();
        let sections = scan_sections(dir.to_str().unwrap());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].heading, "podcasts");
        assert_eq!(sections[0].episodes.len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_sections_subdir_uses_dir_name_as_heading() {
        let dir = new_temp_dir();
        let sub = dir.join("radio-shows");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("ep.mp3"), b"x").unwrap();
        let sections = scan_sections(dir.to_str().unwrap());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].heading, "radio-shows");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_sections_two_level_subdir_uses_slash_heading() {
        let dir = new_temp_dir();
        let sub = dir.join("podcasts").join("2020");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("ep.mp3"), b"x").unwrap();
        let sections = scan_sections(dir.to_str().unwrap());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].heading, "podcasts/2020");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_sections_root_mp3s_alongside_subdirs() {
        let dir = new_temp_dir();
        fs::write(dir.join("root.mp3"), b"x").unwrap();
        let sub = dir.join("music");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("track.mp3"), b"x").unwrap();
        let sections = scan_sections(dir.to_str().unwrap());
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].heading, "podcasts");
        assert_eq!(sections[1].heading, "music");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_sections_empty_subdirs_omitted() {
        let dir = new_temp_dir();
        let sub = dir.join("empty");
        fs::create_dir(&sub).unwrap(); // no mp3s inside
        assert!(scan_sections(dir.to_str().unwrap()).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_sections_sorted_alphabetically() {
        let dir = new_temp_dir();
        let b = dir.join("b-shows");
        let a = dir.join("a-shows");
        fs::create_dir(&b).unwrap();
        fs::create_dir(&a).unwrap();
        fs::write(b.join("z.mp3"), b"x").unwrap();
        fs::write(b.join("a.mp3"), b"x").unwrap();
        fs::write(a.join("ep.mp3"), b"x").unwrap();
        let sections = scan_sections(dir.to_str().unwrap());
        assert_eq!(sections[0].heading, "a-shows");
        assert_eq!(sections[1].heading, "b-shows");
        assert_eq!(sections[1].episodes[0].rel_path, "b-shows/a.mp3");
        assert_eq!(sections[1].episodes[1].rel_path, "b-shows/z.mp3");
        fs::remove_dir_all(dir).unwrap();
    }
}
