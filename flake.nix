# Copyright (c) 2026 l5yth
# SPDX-License-Identifier: Apache-2.0
{
  description = "a minimalist podcast server (type b) for serving media files on the web";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;
      in {
        packages.default = craneLib.buildPackage {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
        };
      });
}
