use relm4::gtk;

/// Fallback icon used when a country flag asset is not installed.
pub(super) const FLAG_FALLBACK: &str = "ld-globe-symbolic";

/// Resolves the flag icon name for an ISO country `code` (e.g. `"se"`).
///
/// Returns the bundled `cm-flag-{code}` icon when it is present in the current
/// icon theme, otherwise a generic globe so the row still renders.
pub(super) fn flag_icon(code: &str) -> String {
    let code = code.trim().to_ascii_lowercase();
    if code.is_empty() {
        return FLAG_FALLBACK.to_string();
    }

    let name = format!("cm-flag-{code}");
    let available = gtk::gdk::Display::default()
        .map(|display| gtk::IconTheme::for_display(&display).has_icon(&name))
        .unwrap_or(false);

    if available {
        name
    } else {
        FLAG_FALLBACK.to_string()
    }
}

/// Derives the ISO country code from a Mullvad relay hostname
/// (e.g. `"se-got-wg-101"` -> `"se"`).
pub(super) fn country_code_from_hostname(hostname: &str) -> Option<String> {
    hostname
        .split('-')
        .next()
        .filter(|code| code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase)
}
