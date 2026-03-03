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

//! Site configuration loaded from an optional `Config.toml` file.

use serde::Deserialize;
use std::fs;

const DEFAULT_TITLE: &str = "podserv-b";
const DEFAULT_DESCRIPTION: &str =
    "a minimalist podcast server (type b) for serving media files on the web";
const DEFAULT_WEBSITE: &str = "https://github.com/l5yth/podserv-b";

/// Site-wide configuration for the web interface.
///
/// Read from a TOML file whose path is given by the `--config` CLI flag
/// (default: `/etc/podserv-b.toml`). All fields are optional; the documented
/// defaults are used for any field that is absent.
///
/// # Example config file
///
/// ```toml
/// title       = "My Radio"
/// description = "Weekly shows and more"
/// website     = "https://mysite.example"
/// ```
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Page `<title>` and visible `<h1>` heading. Default: `"podserv-b"`.
    pub title: Option<String>,
    /// Short description shown below the heading. Default: the app tagline.
    pub description: Option<String>,
    /// Homepage URL linked in the footer. Default: `"https://github.com/l5yth/podserv-b"`.
    pub website: Option<String>,
}

impl Config {
    /// Loads configuration from the TOML file at `path`.
    ///
    /// Returns [`Config::default`] if the file is absent or cannot be parsed.
    /// Emits a warning to stderr if the file exists but contains invalid TOML.
    pub fn load(path: &str) -> Self {
        let raw = fs::read_to_string(path).unwrap_or_default();
        toml::from_str(&raw).unwrap_or_else(|e| {
            eprintln!("warning: {path} is invalid, using defaults ({e})");
            Config::default()
        })
    }

    /// Returns the configured title, or `"podserv-b"` if not set.
    pub fn title(&self) -> &str {
        self.title.as_deref().unwrap_or(DEFAULT_TITLE)
    }

    /// Returns the configured description, or the built-in tagline if not set.
    pub fn description(&self) -> &str {
        self.description.as_deref().unwrap_or(DEFAULT_DESCRIPTION)
    }

    /// Returns the configured website URL, or `"https://github.com/l5yth/podserv-b"` if not set.
    pub fn website(&self) -> &str {
        self.website.as_deref().unwrap_or(DEFAULT_WEBSITE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_toml(s: &str) -> Config {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn defaults_when_no_fields() {
        let cfg = from_toml("");
        assert_eq!(cfg.title(), DEFAULT_TITLE);
        assert_eq!(cfg.description(), DEFAULT_DESCRIPTION);
        assert_eq!(cfg.website(), DEFAULT_WEBSITE);
    }

    #[test]
    fn overrides_title() {
        let cfg = from_toml(r#"title = "My Show""#);
        assert_eq!(cfg.title(), "My Show");
    }

    #[test]
    fn overrides_description() {
        let cfg = from_toml(r#"description = "Daily news""#);
        assert_eq!(cfg.description(), "Daily news");
    }

    #[test]
    fn overrides_website() {
        let cfg = from_toml(r#"website = "https://example.org""#);
        assert_eq!(cfg.website(), "https://example.org");
    }

    #[test]
    fn overrides_all_fields() {
        let cfg = from_toml(
            r#"
            title       = "T"
            description = "D"
            website     = "https://w.example"
            "#,
        );
        assert_eq!(cfg.title(), "T");
        assert_eq!(cfg.description(), "D");
        assert_eq!(cfg.website(), "https://w.example");
    }

    #[test]
    fn invalid_toml_gives_default() {
        let cfg: Config = toml::from_str("!!!invalid!!!").unwrap_or_default();
        assert_eq!(cfg.title(), DEFAULT_TITLE);
    }

    #[test]
    fn load_nonexistent_path_gives_default() {
        let cfg = Config::load("/nonexistent/path/podserv-b-test.toml");
        assert_eq!(cfg.title(), DEFAULT_TITLE);
        assert_eq!(cfg.description(), DEFAULT_DESCRIPTION);
        assert_eq!(cfg.website(), DEFAULT_WEBSITE);
    }

    #[test]
    fn load_valid_file() {
        let path = std::env::temp_dir().join(format!(
            "podserv_b_config_valid_{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, r#"title = "FileTest""#).unwrap();
        let cfg = Config::load(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();
        assert_eq!(cfg.title(), "FileTest");
    }

    #[test]
    fn load_invalid_file_gives_default() {
        let path = std::env::temp_dir().join(format!(
            "podserv_b_config_invalid_{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "!!!invalid!!!").unwrap();
        let cfg = Config::load(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();
        assert_eq!(cfg.title(), DEFAULT_TITLE);
    }
}
