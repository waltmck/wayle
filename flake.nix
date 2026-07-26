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
    cargoHash = "sha256-EgB0fEJjQ52xSI/dS+vUXbZBvdITkNt02BKu7MFGo+A=";

    # Reuse the nixpkgs `wayle` derivation wholesale — build inputs, the
    # GApps/bindgen hooks, desktop item, shell completions, icon install and
    # meta all come along for free — and only swap in THIS checkout as the
    # source. `overrideAttrs` can't touch `cargoHash` (buildRustPackage consumes
    # it to build `cargoDeps` before the derivation exists), so re-vendor this
    # checkout's lockfile explicitly via fetchCargoVendor.
    mkWayle = pkgs: let
      # Only the files the build actually reads. Source-irrelevant commits
      # (docs, README, CI, flake/devenv, packaging, scripts, formatter configs,
      # preview assets) then leave the derivation hash unchanged, so the prebuilt
      # binary is substituted from the cache instead of recompiled. Extend this
      # list if the build ever grows a new top-level input.
      src = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          ./Cargo.toml
          ./Cargo.lock
          ./.cargo # cargo config (aliases today; may gain [build]/[target] settings)
          ./crates # workspace crates + their locales/scss/embedded resources
          ./wayle # the wayle / wayle-settings binary crate
          ./resources # icons, wayle.portal, wayle-settings.svg (install phase)
        ];
      };
    in
      pkgs.wayle.overrideAttrs (old: {
        inherit src;
        cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
          inherit src;
          hash = cargoHash;
        };

        postInstall =
          (old.postInstall or "")
          + ''
            install -Dm644 resources/wayle.portal \
              "$out/share/xdg-desktop-portal/portals/wayle.portal"
          '';
      });
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

    # NixOS module: `services.wayle`. Self-contained — it builds the package with
    # `mkWayle` (this fork's build, which also installs `wayle.portal`), writes the
    # TOML config to /etc, runs a user service, and wires up the notification portal.
    # No overlay required.
    nixosModules.default = {
      config,
      lib,
      pkgs,
      ...
    }: let
      cfg = config.services.wayle;
      # Use the flake's pre-built package — built with wayle's *pinned* nixpkgs,
      # which is exactly what CI builds and pushes to cachix. Rebuilding via
      # `mkWayle pkgs` here would use the CONSUMER's nixpkgs, producing a
      # different store path that isn't in the cache (so `useCache` would miss).
      wayle = self.packages.${pkgs.system}.default;
      inherit (lib) mkEnableOption mkIf mkOption types;
    in {
      options.services.wayle = {
        enable = mkEnableOption "wayle";

        useCache = mkEnableOption "the waltmck cachix binary cache, so the wayle package (and its fork-pinned deps) is substituted as a prebuilt binary instead of compiled from source";

        settings = mkOption {
          inherit (pkgs.formats.toml {}) type;

          default = {};

          description = ''
            Wayle settings in Nix form. You can configure this by configuring with the GUI and then running [toml2nix](https://github.com/erooke/toml2nix) on `~/.config/wayle/runtime.toml`. Example:
            ```nix
            {
              general = {
                font-sans = "SFProDisplay Nerd Font";

                prefer-color-icons = true;
              };
            }
            ```
          '';
        };

        package = mkOption {
          readOnly = true;
          default = wayle;
        };

        deps = mkOption {
          type = types.listOf types.package;
          default = [];
          description = ''
            Packages placed on the wayle service's PATH (tools it shells out to at runtime). Example: `[pkgs.hypridle]`
          '';
        };

        iconThemes = mkOption {
          type = types.listOf types.package;
          default = [];
          description = ''
            Icon theme packages whose share/ directories are added to the service's XDG_DATA_DIRS. Example:
            ```nix
            with pkgs; [
              morewaita-icon-theme
              adwaita-icon-theme
              material-design-icons
              papirus-icon-theme
              hicolor-icon-theme
            ]
            ```
          '';
        };
      };

      config = mkIf cfg.enable {
        # Pull the wayle package (and its fork-pinned deps) from the waltmck
        # cachix cache rather than compiling it. These lists concatenate with
        # nixpkgs' defaults, so cache.nixos.org is preserved.
        nix.settings = mkIf cfg.useCache {
          substituters = ["https://waltmck.cachix.org"];
          trusted-public-keys = ["waltmck.cachix.org-1:jRubmCo4HVNZsnL8+GgsMLaOyi+pMLSuC5/QjYDjWW0="];
        };

        # Default required dependencies
        services.wayle.deps = with pkgs; [
          cfg.package
          systemd
        ];

        environment = {
          etc."wayle/config.toml".source = (pkgs.formats.toml {}).generate "wayle-config" cfg.settings;

          systemPackages = [cfg.package];
        };

        systemd.user.services.wayle = {
          after = ["graphical-session.target"];
          partOf = ["graphical-session.target"];
          wantedBy = ["graphical-session.target"];

          before = [
            "xdg-desktop-autostart.target"
            "tray.target"
          ];

          path = cfg.deps;

          environment.XDG_DATA_DIRS = lib.concatStringsSep ":" (
            (map (p: "${p}/share") cfg.iconThemes)
            ++ [
              "/run/current-system/sw/share"
            ]
          );

          unitConfig = {
            ConditionEnvironment = "WAYLAND_DISPLAY";
          };

          serviceConfig = {
            ExecStart = "${cfg.package}/bin/wayle shell --config /etc/wayle/config.toml";
            Restart = "on-failure";

            Type = "notify";
            NotifyAccess = "main";
          };
        };

        xdg.portal.extraPortals = [wayle];
        xdg.portal.config.common."org.freedesktop.impl.portal.Notification" = "wayle";
      };
    };

    formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
  };
}
