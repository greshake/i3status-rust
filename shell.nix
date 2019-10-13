let
  moz_overlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  nixpkgs = import <nixpkgs> { overlays = [ moz_overlay ]; };
  ruststable = (nixpkgs.latest.rustChannels.stable.rust.override { extensions = [ "rust-src" "rls-preview" "rust-analysis" "rustfmt-preview" ];});
in
  with nixpkgs;
  stdenv.mkDerivation {
    name = "rust";
    nativeBuildInputs = [ pkgconfig ];
    buildInputs = [
      openssl
      nasm
      rustup
      ruststable
      cmake
      zlib
      gnumake
      gcc
      readline
      openssl
      libxml2
      curl
      dbus
      libpulseaudio
    ];

    shellHook = ''
        export OPENSSL_DIR="${openssl.dev}"
        export OPENSSL_LIB_DIR="${openssl.out}/lib"
    '';
  }
