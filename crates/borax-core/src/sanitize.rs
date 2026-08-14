//! The mandatory filename sanitization pass.
//!
//! Every rendered filename goes through [`sanitize`] before it touches
//! a filesystem; the pass is not configurable. It produces names valid
//! on every supported platform at once (the strictest rules — Windows's
//! — apply everywhere, so a collection syncs between machines without
//! renames).

/// Maximum byte length of one path component after sanitization.
pub const MAX_COMPONENT_BYTES: usize = 255;

/// Maximum byte length of the whole sanitized path.
pub const MAX_PATH_BYTES: usize = 1024;

/// Sanitize a rendered path. Pure string-to-string; the result uses
/// `/` as its separator regardless of platform.
///
/// The input is split on `/` (each piece a path component; empty
/// pieces are dropped) and each component is rewritten:
///
/// 1. Characters Windows forbids — `<` `>` `:` `"` `\` `|` `?` `*` and
///    every control character below U+0020 — are each replaced with
///    `_`.
/// 2. Trailing dots and spaces are trimmed (Windows strips them
///    silently; borax removes them explicitly).
/// 3. A component whose stem — the part before the first `.` —
///    case-insensitively equals a reserved Windows device name (`CON`,
///    `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`) gets `_`
///    appended to the stem (`CON.pdf` → `CON_.pdf`).
/// 4. A component longer than [`MAX_COMPONENT_BYTES`] is truncated at
///    a character boundary; when the component has an extension (a
///    final `.`-separated suffix), the stem is truncated instead and
///    the extension survives intact.
/// 5. A component left empty by the previous steps becomes `_`.
///
/// If no component survives, the result is `_`. If the joined path
/// exceeds [`MAX_PATH_BYTES`], the last component's stem is truncated
/// (at a character boundary, keeping its extension) until it fits.
pub fn sanitize(rendered: &str) -> String {
    let mut components: Vec<String> = rendered
        .split('/')
        .filter(|component| !component.is_empty())
        .map(sanitize_component)
        .collect();

    let Some(last) = components.pop() else {
        return "_".to_string();
    };
    let prefix_bytes = components.iter().map(|c| c.len() + 1).sum::<usize>();
    components.push(fit_in_path(last, prefix_bytes));

    components.join("/")
}

/// Windows device names, which are reserved whatever extension follows
/// them. `COM0` and `LPT0` are not reserved, nor is any two-digit form.
const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Steps 1-5 applied to a single, non-empty path component.
fn sanitize_component(component: &str) -> String {
    let replaced: String = component
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();

    let trimmed = replaced.trim_end_matches(['.', ' ']);
    let stem_end = trimmed.find('.').unwrap_or(trimmed.len());
    let mut result = if RESERVED_STEMS
        .iter()
        .any(|reserved| trimmed[..stem_end].eq_ignore_ascii_case(reserved))
    {
        format!("{}_{}", &trimmed[..stem_end], &trimmed[stem_end..])
    } else {
        trimmed.to_string()
    };

    if result.len() > MAX_COMPONENT_BYTES {
        result = truncate_component(&result, MAX_COMPONENT_BYTES);
    }

    if result.is_empty() {
        "_".to_string()
    } else {
        result
    }
}

/// Truncate `component` to at most `limit` bytes, cutting the stem and
/// keeping the extension when doing so can fit within `limit`.
fn truncate_component(component: &str, limit: usize) -> String {
    match split_extension(component) {
        Some((stem, extension)) if extension.len() < limit => {
            format!(
                "{}{extension}",
                &stem[..floor_char_boundary(stem, limit - extension.len())]
            )
        }
        _ => component[..floor_char_boundary(component, limit)].to_string(),
    }
}

/// Shorten the path's last component until `prefix_bytes` (the joined
/// leading components, each with its separator) plus the component fit
/// in [`MAX_PATH_BYTES`].
fn fit_in_path(last: String, prefix_bytes: usize) -> String {
    match MAX_PATH_BYTES.checked_sub(prefix_bytes) {
        Some(budget) if last.len() > budget => truncate_component(&last, budget),
        _ => last,
    }
}

/// Split a component into its stem and its final `.`-separated suffix
/// (the dot included). `None` when the component has no extension to
/// preserve.
fn split_extension(component: &str) -> Option<(&str, &str)> {
    let dot = component.rfind('.').filter(|dot| *dot > 0)?;
    Some((&component[..dot], &component[dot..]))
}

/// The largest character boundary of `s` at or below `index`.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut boundary = index;
    while !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}
