# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`podserv-b` is a minimal podcast server written in Rust. It scans a directory for MP3 files, reads their ID3 tags, and serves a single-page web UI with an embedded audio player over HTTP.

## Commands

```sh
cargo build                          # build
cargo run                            # run (default: media/ dir, http://127.0.0.1:8447)
cargo run -- --media ~/music --bind 0.0.0.0:8080  # override defaults
cargo test --all --all-features      # run all tests
cargo fmt --all                      # format code
cargo clippy --all-targets --all-features -- -D warnings  # lint (warnings are errors)
cargo doc --workspace --no-deps --document-private-items  # build docs (warnings are errors: set RUSTDOCFLAGS=-D warnings)
```

CI runs `cargo check`, `cargo fmt --check`, `cargo test`, `cargo clippy`, and `cargo doc` on every push/PR to `main`. Clippy warnings and doc warnings are treated as errors.

## Code Standards

**License:** Apache 2.0. Every source file must begin with this header:
```rust
// Copyright (c) 2026 l5yth
// SPDX-License-Identifier: Apache-2.0
```

**Docs coverage:** 100% is required. Every `pub` and `pub(crate)` item — structs, fields, functions, modules — must have a `///` doc comment. `cargo doc` runs with `RUSTDOCFLAGS=-D warnings`, so missing docs fail CI.

**Test coverage:** 100% is required and enforced by Codecov (target: 100% for both project and patch). Every function must have unit tests covering all branches. Use `#[cfg(test)] mod tests` within the same file as the code under test.

**Modularity:** Do not write monolithic `.rs` files. Split code into focused modules under `src/` (e.g. `src/media.rs`, `src/render.rs`) and declare them in `src/lib.rs` or `src/main.rs`. Each module should have a single clear responsibility. `main.rs` should contain only startup/wiring logic.

## Architecture

Modules under `src/`:
- `config.rs` — TOML site config (`title`, `description`, `website`, `base_url`, `author`, `language`, `explicit`)
- `media.rs` — scans MP3s up to 2 dirs deep into `Vec<Section>`; `Episode` holds ID3 tags, `size_bytes`, `pub_date`, and optional cover art
- `render.rs` — pure string HTML renderer (`render_page`), `html_escape`, `url_encode_path`
- `rss.rs` — RSS 2.0 + iTunes feed renderer (`render_rss`), `xml_escape`, `format_pub_date`
- `main.rs` — startup wiring only: `PageCache`, `RssCache`, `ArtMap`, `RealIpKeyExtractor`, handlers, `HttpServer`

**Startup flow:**
1. Parse `--config`/`-c`, `--media`/`-m`, `--bind`/`-b` CLI flags (also `CONFIG`, `MEDIA_DIR`, `BIND` env vars).
2. `Config::load()` reads the TOML config file (defaults used if absent).
3. `scan_sections()` reads all `.mp3` files from the media directory (up to 2 levels deep), extracting ID3 tags, file size, modification time, and cover art into `Vec<Section>`.
4. `render_page()` and `render_rss()` are called once to pre-render both responses into `PageCache` and `RssCache` (with ETags).
5. Cover art is extracted into an in-memory `ArtMap` (`HashMap<rel_path, (mime, bytes)>`).
6. `HttpServer` mounts all handlers; episodes are never re-scanned without a restart.

**Request handling:**
- `GET /` — serves `PageCache` HTML with ETag / 304 support.
- `GET /rss` — serves `RssCache` XML (`application/rss+xml`) with ETag / 304 support.
- `GET /art/<path>` — serves cover art from the in-memory `ArtMap`; `Cache-Control: max-age=86400, immutable`.
- `GET /media/<file>` — served by `actix-files` with range-request support (needed for seek).

**Key design constraints:**
- All responses are pre-rendered at startup; restart required to pick up new files or config changes.
- HTML/XML escaping done manually — no template engine.
- Only `image/*` MIME types are stored in `ArtMap` to prevent Content-Type injection.
- Rate limiting via `actix-governor` with `RealIpKeyExtractor` (reads `X-Real-IP` from loopback peers for correct per-client limiting behind nginx).
