/// Dev-only toggle for destructive database resets.
///
/// Pre-v1 allows dropping state store tables when schemas change, but only when
/// explicitly enabled. This prevents accidental data loss once users rely on
/// tags/rules/workspaces.
pub fn dev_allow_destructive_reset() -> bool {
    match std::env::var("CASPARIAN_DEV_ALLOW_RESET") {
        Ok(value) => {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        }
        Err(_) => false,
    }
}
