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
    let _ = rendered;
    todo!("apply the sanitization pass")
}
