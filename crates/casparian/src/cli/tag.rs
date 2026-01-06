//! Tag command - Assign topics to files
//!
//! W2 implements this module.

use std::path::PathBuf;

/// Arguments for the tag command
#[derive(Debug)]
#[allow(dead_code)]
pub struct TagArgs {
    pub path: Option<PathBuf>,
    pub topic: Option<String>,
    pub dry_run: bool,
    pub no_queue: bool,
}

/// Execute the tag command
pub fn run(_args: TagArgs) -> anyhow::Result<()> {
    todo!("W2 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W2 implements this")]
    fn test_tag_not_implemented() {
        let args = TagArgs {
            path: None,
            topic: None,
            dry_run: false,
            no_queue: false,
        };
        run(args).unwrap();
    }
}
