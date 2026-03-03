# Copyright (c) 2026 l5yth
# SPDX-License-Identifier: Apache-2.0

EAPI=8

inherit cargo git-r3

DESCRIPTION="a minimalist podcast server (type b) for serving media files on the web"
HOMEPAGE="https://github.com/l5yth/podserv-b"
EGIT_REPO_URI="https://github.com/l5yth/podserv-b.git"

LICENSE="Apache-2.0"
SLOT="0"
KEYWORDS=""
IUSE=""
PROPERTIES="live"

BDEPEND="
	>=virtual/rust-1.85
"

src_unpack() {
	git-r3_src_unpack
	cargo_live_src_unpack
}

src_compile() {
	cargo_src_compile --bin podserv-b
}

src_test() {
	cargo_src_test
}

src_install() {
	cargo_src_install
	einstalldocs
}
