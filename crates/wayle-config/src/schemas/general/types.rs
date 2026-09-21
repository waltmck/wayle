use wayle_derive::wayle_enum;

/// When to use full-colour app icons instead of symbolic ones.
#[wayle_enum(default)]
pub enum ColorIconMode {
    /// Never use colour icons; unresolved apps get the generic fallback icon.
    Never,
    /// Use a colour icon only when no symbolic icon can be resolved.
    #[default]
    Fallback,
    /// Use colour icons first, falling back to symbolic ones when none exists.
    Prefer,
}

/// Layer-shell layer a window is placed on, from furthest back to furthest front.
#[wayle_enum(default)]
pub enum Layer {
    /// Below everything else, used for wallpapers and ambient surfaces.
    Background,
    /// Behind regular application windows.
    Bottom,
    /// Above regular application windows.
    #[default]
    Top,
    /// Above everything, including fullscreen application windows.
    Overlay,
}
