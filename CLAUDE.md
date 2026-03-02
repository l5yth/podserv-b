# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`podserv-b` is a minimal podcast server written in Rust. It scans a directory for MP3 files, reads their ID3 tags, and serves a single-page web UI with an embedded audio player over HTTP.

## Commands

```sh
cargo build                          # build
cargo run                            # run (default: media/ dir, http://127.0.0.1:3000)
MEDIA_DIR=~/music BIND=0.0.0.0:8080 cargo run  # override config via env vars
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

The entire application currently lives in `src/main.rs`. New code should be extracted into separate modules rather than added to this file.

**Startup flow:**
1. Read `MEDIA_DIR` and `BIND` env vars (defaults: `"media"`, `"127.0.0.1:3000"`).
2. `scan_media()` reads all `.mp3` files from the directory, extracts ID3 tags (title, artist, album, year, duration) and file size into `Vec<Episode>`.
3. Episodes are loaded once at startup and stored in `web::Data<Vec<Episode>>` — the server does **not** hot-reload when files change.
4. `HttpServer` mounts `GET /` → `index` handler and `GET /media/<file>` via `actix_files::Files`.

**Request handling:**
- `GET /` calls `render_page()`, which generates a complete HTML page as a `String` containing inline CSS, the episode list as HTML rows, and inline JavaScript with the filenames and titles serialized as JSON arrays. Audio playback is handled entirely client-side; clicking an episode sets `<audio>.src` to `/media/<filename>` and auto-advances on `ended`.
- `GET /media/<filename>` is served directly by `actix-files` with range-request support (needed for seek).

**Key design constraints:**
- Episode metadata is scanned once at startup; restart required to pick up new files.
- HTML escaping is done manually via `html_escape()` — not a template engine.
- The `Episode` struct is `Serialize` but currently only used for JSON serialization of filenames/titles inside the rendered page (not exposed as an API endpoint).
