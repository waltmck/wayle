# waltmck/wayle

This is a fork of Wayle for testing my experimental changes prior to upstreaming. It currently includes
- An IWD module for controling WiFi without NetworkManager
- A rewritten systray module that fixes several race conditions.
- A fix that makes the `netstat` module work correctly without NetworkManager.

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
