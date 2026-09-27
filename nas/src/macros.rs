//! Project-wide macros.

/// Declare an API response struct with camelCase JSON field names.
///
/// All Go→Rust ported structs that are serialised to JSON for the web
/// frontend should use this macro instead of a bare `#[derive(Serialize)]`.
/// It automatically applies `#[serde(rename_all = "camelCase")]` so that
/// Rust's snake_case fields map to the Go camelCase keys that the
/// frontend expects — no per-field `#[serde(rename)]` needed.
///
/// # Example
/// ```ignore
/// api_response! {
///     struct AuthStatusResponse {
///         authenticated: bool,
///         needs_setup: bool,
///     }
/// }
/// // Serialises to { "authenticated": false, "needsSetup": true }
/// ```
macro_rules! api_response {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field:ident: $ty:ty
            ),+ $(,)?
        }
    ) => {
        #[derive(Debug, serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        $(#[$meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field: $ty
            ),+
        }
    };
}
