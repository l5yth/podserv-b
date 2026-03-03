# Copyright (c) 2026 l5yth
# SPDX-License-Identifier: Apache-2.0

EAPI=8

inherit cargo git-r3 systemd

DESCRIPTION="a minimalist podcast server (type b) for serving media files on the web"
HOMEPAGE="https://github.com/l5yth/podserv-b"
EGIT_REPO_URI="https://github.com/l5yth/podserv-b.git"

LICENSE="Apache-2.0"
SLOT="0"
KEYWORDS=""
IUSE="systemd"
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

pkg_preinst() {
	enewgroup podserv-b
	enewuser podserv-b -1 -1 /srv/podcasts podserv-b
}

src_install() {
	cargo_src_install
	einstalldocs

	# Ship an example config so upgrades never clobber admin edits.
	insinto /etc
	newins "${S}/Config.toml" podserv-b.toml.example

	# Media directory — ownership is set in pkg_postinst once the user exists.
	keepdir /srv/podcasts

	use systemd && systemd_dounit "${S}/packaging/systemd/podserv-b.service"
}

pkg_postinst() {
	# pkg_preinst has already created the podserv-b user; the directory has
	# been merged from ${D} by this point, so chown can resolve the username.
	chown podserv-b:podserv-b "${EROOT}/srv/podcasts"

	if [[ ! -e "${EROOT}/etc/podserv-b.toml" ]]; then
		elog "No config file found. Copy the example to get started:"
		elog "  cp /etc/podserv-b.toml.example /etc/podserv-b.toml"
	fi
}
