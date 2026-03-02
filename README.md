# podserv-b

a minimalist podcast server (type b) for serving media files on the web

Scans a directory of MP3 files, reads their ID3 tags, and serves a dark-themed single-page web UI with an embedded audio player, album art, and download links. Supports flat and nested media directories.

## Installation

```sh
cargo build --release
# binary is at target/release/podserv-b
```

## Usage

```sh
./target/release/podserv-b
```

Open `http://127.0.0.1:3000` in a browser.

### Environment variables

| Variable    | Default           | Description                      |
|-------------|-------------------|----------------------------------|
| `MEDIA_DIR` | `media`           | Path to the directory of MP3s    |
| `BIND`      | `127.0.0.1:3000`  | Address and port to listen on    |

```sh
MEDIA_DIR=/srv/podcasts BIND=0.0.0.0:8080 ./podserv-b
```

### Site configuration

Create `Config.toml` in the working directory to customise the page:

```toml
title       = "My Radio"
description = "Weekly shows and more"
website     = "https://mysite.example"
```

All fields are optional; defaults are used when the file is absent.

### Media directory layout

**Flat** — all MP3s directly in `MEDIA_DIR`, shown under a single "podcasts" heading:

```
media/
  episode-001.mp3
  episode-002.mp3
```

**Subdirectories** — each first-level subdirectory becomes its own heading:

```
media/
  podcasts/
    ep-001.mp3
  radio-shows/
    show-001.mp3
  music/
    track-001.mp3
```

**Nested subdirectories** — second-level directories shown as `parent/child`:

```
media/
  podcasts/
    2023/
      ep-001.mp3
    2024/
      ep-002.mp3
```

Directories deeper than two levels are ignored.
