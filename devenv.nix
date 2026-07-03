{ pkgs, ... }:

{
  # System libraries required to build wayle (GTK4 + Wayland stack).
  # The Rust toolchain itself is taken from the ambient environment.
  packages = with pkgs; [
    pkg-config
    gcc
    cmake
    # Sets LIBCLANG_PATH + clang include args for crates using bindgen (wayle-cava).
    rustPlatform.bindgenHook

    udev
    fftw
    fftwFloat
    pipewire
    alsa-lib

    glib
    gtk4
    gtk4-layer-shell
    gtksourceview5
    gdk-pixbuf
    pango
    cairo
    graphene
    harfbuzz
    librsvg

    wayland
    libxkbcommon
    libpulseaudio
  ];
}
