//! Files command - List discovered files
//!
//! W2 implements this module.

/// Arguments for the files command
#[derive(Debug)]
#[allow(dead_code)]
pub struct FilesArgs {
    pub topic: Option<String>,
    pub status: Option<String>,
    pub untagged: bool,
    pub limit: usize,
}

/// Execute the files command
pub fn run(_args: FilesArgs) -> anyhow::Result<()> {
    todo!("W2 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W2 implements this")]
    fn test_files_not_implemented() {
        let args = FilesArgs {
            topic: None,
            status: None,
            untagged: false,
            limit: 50,
        };
        run(args).unwrap();
    }
}
