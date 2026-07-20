{
  description = "Wayle — a compositor-agnostic, highly customizable Wayland shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    inherit (nixpkgs) lib;

    # Wayle is Linux-only (see the nixpkgs derivation's meta.platforms).
    systems = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

    # A single fixed-output hash over this checkout's vendored dependencies.
    # Cargo.toml carries a `[patch.crates-io]` that pulls the wayle-services
    # crates (systray/core/traits/iwd/notification) from a Git fork, so the
    # lockfile references Git sources and the vendor set differs from the tagged
    # release nixpkgs builds. It MUST be refreshed whenever Cargo.lock changes:
    # set it to lib.fakeHash, build, and paste the value Nix reports.
    cargoHash = "sha256-CMjoqXC2O69G4AhKIYZcR4XzPm5jr8ceHfSUTyLnfgU=";

    # Reuse the nixpkgs `wayle` derivation wholesale — build inputs, the
    # GApps/bindgen hooks, desktop item, shell completions, icon install and
    # meta all come along for free — and only swap in THIS checkout as the
    # source. `overrideAttrs` can't touch `cargoHash` (buildRustPackage consumes
    # it to build `cargoDeps` before the derivation exists), so re-vendor this
    # checkout's lockfile explicitly via fetchCargoVendor.
    mkWayle = pkgs:
      pkgs.wayle.overrideAttrs {
        src = self;
        cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
          src = self;
          hash = cargoHash;
        };
      };
  in {
    packages = forAllSystems (pkgs: rec {
      wayle = mkWayle pkgs;
      default = wayle;
    });

    # `nix develop` — the package's own build environment (every build/native
    # input inherited from nixpkgs' wayle, including the bindgen/clang setup that
    # exports LIBCLANG_PATH) plus the Rust toolchain and editor tooling.
    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        inputsFrom = [(mkWayle pkgs)];
        packages = with pkgs; [
          cargo
          rustc
          clippy
          rustfmt
          rust-analyzer
        ];
      };
    });

    # Override `pkgs.wayle` in a consumer's nixpkgs. Base off `prev.wayle` (the
    # un-overridden derivation) so applying the overlay doesn't recurse.
    overlays.default = final: prev: {
      wayle = mkWayle prev;
    };

    formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
  };
}
