# waltmck/wayle

This is a fork of Wayle for testing my experimental changes prior to upstreaming. Currently outstanding PRs included in this branch:
- An IWD module for controling WiFi without NetworkManager. [#300](https://github.com/wayle-rs/wayle/pull/300) [#35](https://github.com/wayle-rs/wayle-services/pull/35)
- A rewritten systray module that fixes several race conditions. [#34](https://github.com/wayle-rs/wayle-services/pull/34)
- A fix that makes the `netstat` module work correctly without NetworkManager. [#316](https://github.com/wayle-rs/wayle/pull/316)
- Use absolute paths for `/bin/sh` to fix certain functionality when run from a systemd service. [#316](https://github.com/wayle-rs/wayle/pull/317)
- [WIP] Allow passing a config file location with `-c` or `--config`, enabling a declarative Nix-based module without home-manager. [#318](https://github.com/wayle-rs/wayle/pull/318)
- Generally much more robust notification service, that works more efficiently with a large number of notifications [#321](https://github.com/wayle-rs/wayle/pull/321) [#38](https://github.com/wayle-rs/wayle-services/pull/36):
  - Direct dbus actions to the notification owner's bus name to work around broken apps (matching GNOME and dunst)
  - Remove non-functional action buttons from orphaned FDO notifications
  - Add support for the `org.gtk.Notifications` backend, which allows notification actions to persist app restarts (only for apps that support `org.gtk.Notifications`, in practice GTK/GApplications).
  - Make batch dismissals ("dismiss all") do atomic database writes and atomic re-renders for drastic performance improvement with many notifications.
  - Minimize widget churn on adding/removing notifications
  - Deduplicate icon images in memory
- Implement systemd `Type=notify` support to avoid use-before-setup race conditions on startup. [#323](https://github.com/wayle-rs/wayle/pull/323)
- Add `general.symbolic-icon-fallback` option to fall back to a symbolic desktop icon if there is no hardcoded symbolic icon (applies to notification and workspace modules). [#325](https://github.com/wayle-rs/wayle/pull/325)

Roadmap:
- Fix large systray dropdown menus [#285](https://github.com/wayle-rs/wayle/issues/285)
- Squash bugs in Media module's mpris2 controls [#156](https://github.com/wayle-rs/wayle/issues/156)
- Implement modules:
  - Mullvad (status, connect, disconnect, select relay). Daemon is controllable over dbus interface.
  - Syncthing (sync status, etc.)
  - systemd-networkd (exposes dbus API to get/set status of managed interfaces). Need to think about how this can/should interface with Network module
  - ZFS (pool status, dataset usage, health)
  - mpd (play, pause, select song/album from music library)
- UI annoyances:
  - `dropdown-autohide = true` breaks focus when module is closed on Hyprland. Need to investigate to see whose fault it is and possibly work around Hyprland's behavior (seems possible since this doesn't happen with other shells).
  - Hyprland Workspaces should change the cursor to a pointer, as do other clickable elements in the shell.

Please feel free to test these changes and report any issues so that I can fix them before upstreaming.

# Installation

This package can be built normally with `cargo build`. It is also packaged as flake for fellow Nix enjoyers.

## Flake outputs

| Output | Description |
|--------|-------------|
| `packages.<system>.wayle` / `.default` | The `wayle` package (stolen from `nixpkgs`) |
| `devShells.<system>.default` | A dev shell (alternatively use `devenv`) |
| `overlays.default` | Overlay that overrides `pkgs.wayle` |
| `formatter.<system>` | `nixfmt-rfc-style`, for `nix fmt` |

Supported systems: `x86_64-linux`, `aarch64-linux`.

## Building/running with Nix

```bash
nix run .#wayle -- shell            # start the shell
nix run .#wayle -- systray list     # any wayle subcommand
# or, after `nix build`:
./result/bin/wayle shell
```

## Development shell

```bash
nix develop
# inside the shell, the usual cargo workflow works:
cargo build -p wayle
cargo run -p wayle -- shell
```

The shell inherits every build/native input from the package (pkg-config, GTK/Wayland
libraries, `cmake`, the bindgen/clang setup for libcava) and adds `cargo`, `rustc`,
`clippy`, `rustfmt`, and `rust-analyzer`.

## Installation with NixOS

### NixOS (flake)

```nix
{
  inputs.waltmck-wayle.url = "github:waltmck/wayle";   # or a fork / "git+file:///path/to/wayle"

  outputs = { nixpkgs, wayle, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        {
          nixpkgs.overlays = [ wayle.overlays.default ];
          environment.systemPackages = [ pkgs.wayle ];
        }
        # ... OR ...
        {
          environment.systemPackages = [ inputs.waltmck-wayle.packages.${pkgs.system}.default ];
        }
      ];
    };
  };
}
```

### home-manager (flake)

You must also add `wayle` to your inputs, and then

```nix
{ inputs, pkgs, ... }: let
  wayle = inputs.waltmck-wayle.packages.${pkgs.system}.default;
in {
  home.packages = [ wayle ];

  # To make HM's Wayle module use waltmck/wayle
  services.wayle.package = wayle;
}
```
