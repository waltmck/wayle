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

    # Wayle is Linux-only (see meta.platforms below).
    systems = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

    # Track the workspace version straight from the manifest.
    version = (lib.importTOML ./Cargo.toml).workspace.package.version;

    # The package definition. Modelled on the nixpkgs `wayle` derivation
    # (pkgs/by-name/wa/wayle/package.nix), but it builds THIS checkout rather than a
    # tagged release, and its system dependencies are widened to match the current
    # workspace (extra crates such as wayle-iwd / wayle-cava) and the devenv toolchain.
    mkWayle = pkgs:
      pkgs.rustPlatform.buildRustPackage (finalAttrs: {
        pname = "wayle";
        inherit version;

        __structuredAttrs = true;
        strictDeps = true;

        # Build the repository as it stands. NOTE: Cargo.toml carries a
        # `[patch.crates-io]` that pulls wayle-services (wayle-{systray,core,traits})
        # from a Git fork, so the lockfile references Git sources. `cargoHash` covers
        # the whole vendored dependency set including those Git crates.
        src = self;

        # A single fixed-output hash over all vendored deps (crates.io + the Git-patched
        # fork crates). It MUST be refreshed whenever Cargo.lock changes (dependency
        # bumps, or the fork moving). On a mismatch Nix prints the value to paste here;
        # start from lib.fakeHash. See the README section for the exact workflow.
        #
        # Prefer per-crate pinning instead? Replace this line with:
        #   cargoLock = {
        #     lockFile = ./Cargo.lock;
        #     outputHashes = {
        #       "wayle-systray-0.1.4" = lib.fakeHash;  # rev b7ef7f6…
        #       "wayle-traits-0.1.2"  = lib.fakeHash;  # rev b7ef7f6…
        #       "wayle-core-0.1.2"    = lib.fakeHash;  # rev c0ccb44…
        #     };
        #   };
        cargoHash = "sha256-P+Xne5+mNaDrpdelxjuxob3tXVvlbkyOewGfZEwz9xU=";

        nativeBuildInputs = with pkgs; [
          pkg-config
          cmake # libcava (wayle-cava, vendored) build
          rustPlatform.bindgenHook # bindgen for libcava; exports LIBCLANG_PATH
          wrapGAppsHook4
          glib # glib-compile-resources / gio at build time
          installShellFiles
          copyDesktopItems
        ];

        buildInputs = with pkgs; [
          # GTK / rendering stack
          gtk4
          gtk4-layer-shell
          gtksourceview5
          glib
          gdk-pixbuf
          pango
          cairo
          graphene
          harfbuzz
          librsvg # SVG icon loading at runtime
          pixman

          # Wayland / input
          wayland
          libxkbcommon

          # Audio / visualiser (wayle-audio, wayle-cava)
          libpulseaudio
          pipewire
          fftw
          fftwFloat # single-precision FFTW for libcava
          alsa-lib

          # System / hardware (udev-backed services)
          udev
        ];

        # Only the two shipped binaries; skip building examples/tests of every crate.
        cargoBuildFlags = [
          "--bin=wayle"
          "--bin=wayle-settings"
        ];

        preCheck = ''
          export HOME=$(mktemp -d)
        '';

        checkFlags = [
          # Requires a running GTK display.
          "--skip=tests::css_loads_into_gtk4"
        ];

        preInstall = ''
          mkdir -p "$out/share/icons/hicolor/scalable/apps"
          cp -r resources/icons "$out/share"
          cp resources/wayle-settings.svg "$out/share/icons/hicolor/scalable/apps"
        '';

        postInstall = lib.optionalString (pkgs.stdenv.buildPlatform.canExecute pkgs.stdenv.hostPlatform) ''
          installShellCompletion --cmd wayle \
            --bash <($out/bin/wayle completions bash) \
            --fish <($out/bin/wayle completions fish) \
            --zsh <($out/bin/wayle completions zsh)
        '';

        preFixup = ''
          # Let `wayle` find the `wayle-settings` binary at runtime.
          gappsWrapperArgs+=( --suffix PATH : $out/bin )
        '';

        desktopItems = [
          (pkgs.makeDesktopItem {
            name = "com.wayle.settings.desktop";
            type = "Application";
            desktopName = "Wayle Settings";
            genericName = "Shell Settings";
            comment = "Configure the Wayle desktop shell";
            exec = "wayle-settings";
            icon = "wayle-settings";
            terminal = false;
            categories = [
              "Settings"
              "DesktopSettings"
              "GTK"
            ];
            keywords = [
              "wayle"
              "settings"
              "shell"
              "bar"
              "wayland"
              "config"
            ];
            startupNotify = true;
            startupWMClass = "com.wayle.settings";
          })
        ];

        meta = {
          description = "Wayland Elements — a compositor-agnostic shell with extensive customization";
          homepage = "https://github.com/wayle-rs/wayle/";
          license = lib.licenses.mit;
          mainProgram = "wayle";
          platforms = lib.platforms.linux;
        };
      });
  in {
    packages = forAllSystems (pkgs: rec {
      wayle = mkWayle pkgs;
      default = wayle;
    });

    # `nix develop` — a shell to build/hack on Wayle with plain `cargo`.
    # Inherits every build/native input from the package, then adds the Rust toolchain
    # and editor tooling. (bindgenHook, inherited, exports LIBCLANG_PATH; pkg-config
    # picks up the inherited buildInputs.)
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

    overlays.default = final: _prev: {
      wayle = mkWayle final;
    };

    formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
  };
}
