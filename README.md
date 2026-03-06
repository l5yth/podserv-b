# podserv-b

[![Rust](https://github.com/l5yth/podserv-b/actions/workflows/rust.yml/badge.svg)](https://github.com/l5yth/podserv-b/actions/workflows/rust.yml)
[![Codecov](https://codecov.io/gh/l5yth/podserv-b/graph/badge.svg)](https://codecov.io/gh/l5yth/podserv-b)
[![GitHub Release](https://img.shields.io/github/v/release/l5yth/podserv-b)](https://github.com/l5yth/podserv-b/releases)
[![Crates.io](https://img.shields.io/crates/v/podserv-b.svg)](https://crates.io/crates/podserv-b)
[![AUR](https://img.shields.io/aur/version/podserv-b-git?logo=archlinux)](https://aur.archlinux.org/packages/podserv-b-git)
[![Nix Flake](https://img.shields.io/badge/nix-flake-5277C3?logo=nixos)](https://github.com/l5yth/podserv-b/blob/main/flake.nix)
[![Gentoo](https://img.shields.io/badge/gentoo-ebuild-54487A?logo=gentoo)](https://github.com/l5yth/podserv-b/tree/main/packaging/gentoo)
[![Top Language](https://img.shields.io/github/languages/top/l5yth/podserv-b)](https://github.com/l5yth/podserv-b)
[![License: Apache-2.0](https://img.shields.io/github/license/l5yth/podserv-b)](https://github.com/l5yth/podserv-b/blob/main/LICENSE)

_a minimalist podcast server (type b) for serving media files on the web._

![screenshot of the first version](assets/images/podserv-b-preview.png)

scans a provided directory of mp3 files, reads their id3 tags, and serves a
minimalist-themed single-page web page with an embedded audio player, album
art, and download links. supports flat and nested media directories.

## installation

```sh
cargo install podserv-b
```

for linux packages see [archlinux/PKGBUILD](./packaging/archlinux/PKGBUILD)
or
[gentoo/podserv-b-9999.ebuild](./packaging/gentoo/media-sound/podserv-b/podserv-b-9999.ebuild)

to deploy as a systemd service (packages handle the user/dir automatically):

```sh
# if installed via a linux package:
cp /etc/podserv-b.toml.example /etc/podserv-b.toml

# if installed via cargo install, create /etc/podserv-b.toml manually —
# all fields are optional, see the configuration section below for the schema.

systemctl enable --now podserv-b
```

## usage

`podserv-b` binds to `127.0.0.1:8447` and serves mp3 files in `./media` by default

```sh
podserv-b v0.1.1
a minimalist podcast server (type b) for serving media files on the web
apache v2 (c) 2026 l5yth

Command-line arguments

Usage: podserv-b [OPTIONS]

Options:
  -c, --config <CONFIG>  Path to the TOML configuration file [env: CONFIG=] [default: /etc/podserv-b.toml]
  -m, --media <MEDIA>    Directory containing MP3 files to serve [env: MEDIA_DIR=] [default: media]
  -b, --bind <BIND>      Address to bind the HTTP server to [env: BIND=] [default: 127.0.0.1:8447]
  -h, --help             Print help
  -V, --version          Print version
```

### configuration

the config file is a TOML file read at startup. pass its path with `-c` / `--config`
(or the `CONFIG` env var). the default path is `/etc/podserv-b.toml`.

```toml
title       = "My Podserv B"
description = "Station for Podcast Lovers, Listeners, and Dogs"
website     = "https://example-b.com"

# RSS feed fields
base_url    = "https://pods.example-b.com"  # absolute URL prefix for enclosure links
author      = "Jane Smith"                  # <itunes:author> / <managingEditor>
language    = "en"                          # BCP 47 language tag
explicit    = false                         # <itunes:explicit>
```

all fields are optional; defaults are used when the file is absent.

### endpoints

| route | description |
|---|---|
| `GET /` | episode browser (HTML) |
| `GET /rss` | RSS 2.0 + iTunes podcast feed (XML) |
| `GET /media/<file>` | audio file with range-request support |
| `GET /art/<file>` | embedded cover art |
