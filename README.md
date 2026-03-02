# podserv-b

[![Rust](https://github.com/l5yth/podserv-b/actions/workflows/rust.yml/badge.svg)](https://github.com/l5yth/podserv-b/actions/workflows/rust.yml)
[![Codecov](https://codecov.io/gh/l5yth/podserv-b/graph/badge.svg)](https://codecov.io/gh/l5yth/podserv-b)
[![GitHub Release](https://img.shields.io/github/v/release/l5yth/podserv-b)](https://github.com/l5yth/podserv-b/releases)
[![Crates.io](https://img.shields.io/crates/v/podserv-b.svg)](https://crates.io/crates/podserv-b)
[![Top Language](https://img.shields.io/github/languages/top/l5yth/podserv-b)](https://github.com/l5yth/podserv-b)
[![License: Apache-2.0](https://img.shields.io/github/license/l5yth/podserv-b)](https://github.com/l5yth/podserv-b/blob/main/LICENSE)

_a minimalist podcast server (type b) for serving media files on the web._

![screenshot of the first version](assets/images/podserv-b-preview.png)

scans a provided directory of MP3 files, reads their ID3 tags, and serves a
minimalist-themed single-page web page with an embedded audio player, album
art, and download links. supports flat and nested media directories.

## installation

```sh
cargo build --release
```
binary is at `target/release/podserv-b`

## usage

```sh
./target/release/podserv-b
```

open `http://127.0.0.1:3000` in a browser.

### environment

| Variable    | Default           | Description                      |
|-------------|-------------------|----------------------------------|
| `MEDIA_DIR` | `media`           | Path to the directory of MP3s    |
| `BIND`      | `127.0.0.1:3000`  | Address and port to listen on    |

```sh
MEDIA_DIR=/srv/podcasts BIND=0.0.0.0:8080 ./podserv-b
```

### configuration

create `Config.toml` in the working directory to customise the page:

```toml
title       = "Funkfabrik B"
description = "FM Radio for Punks, Listeners, and Dogs"
website     = "https://funkfabrik-b.de"
```

all fields are optional; defaults are used when the file is absent.
