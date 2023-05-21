{ pkgs ? import <nixpkgs> {} }:
let
  unstable = import <nixpkgs-unstable> {};
in
pkgs.mkShell {
  nativeBuildInputs = with unstable; [ rustc cargo gcc rustfmt clippy openssl pkgconfig pulseaudio lm_sensors ];

  # Certain Rust tools won't work without this
  # This can also be fixed by using oxalica/rust-overlay and specifying the rust-src extension
  # See https://discourse.nixos.org/t/rust-src-not-found-and-other-misadventures-of-developing-rust-on-nixos/11570/3?u=samuela. for more details.
  RUST_SRC_PATH = "${unstable.rust.packages.stable.rustPlatform.rustLibSrc}";
}
