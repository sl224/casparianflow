//! Jobs command - List processing jobs
//!
//! W4 implements this module.

/// Arguments for the jobs command
#[derive(Debug)]
#[allow(dead_code)]
pub struct JobsArgs {
    pub topic: Option<String>,
    pub pending: bool,
    pub running: bool,
    pub failed: bool,
    pub done: bool,
    pub limit: usize,
}

/// Execute the jobs command
pub fn run(_args: JobsArgs) -> anyhow::Result<()> {
    todo!("W4 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W4 implements this")]
    fn test_jobs_not_implemented() {
        let args = JobsArgs {
            topic: None,
            pending: false,
            running: false,
            failed: false,
            done: false,
            limit: 50,
        };
        run(args).unwrap();
    }
}
