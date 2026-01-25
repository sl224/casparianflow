use super::app::TuiMode;

#[derive(Debug, Clone, Copy)]
pub struct NavItem {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub mode: TuiMode,
}

pub const NAV_ITEMS: &[NavItem] = &[
    NavItem {
        key: "0",
        label: "Home",
        description: "Return to home hub",
        mode: TuiMode::Home,
    },
    NavItem {
        key: "1",
        label: "Discover",
        description: "File discovery and tagging",
        mode: TuiMode::Discover,
    },
    NavItem {
        key: "2",
        label: "Parser Bench",
        description: "Parser development workbench",
        mode: TuiMode::ParserBench,
    },
    NavItem {
        key: "3",
        label: "Jobs",
        description: "Job queue management",
        mode: TuiMode::Jobs,
    },
    NavItem {
        key: "4",
        label: "Sources",
        description: "Sources management",
        mode: TuiMode::Sources,
    },
    NavItem {
        key: "5",
        label: "Approvals",
        description: "MCP approval management",
        mode: TuiMode::Approvals,
    },
    NavItem {
        key: "6",
        label: "Query",
        description: "SQL query console",
        mode: TuiMode::Query,
    },
    NavItem {
        key: "7",
        label: "Sessions",
        description: "Intent pipeline workflows",
        mode: TuiMode::Sessions,
    },
    NavItem {
        key: ",",
        label: "Settings",
        description: "Application settings",
        mode: TuiMode::Settings,
    },
];

pub fn nav_index_for_mode(mode: TuiMode) -> Option<usize> {
    NAV_ITEMS.iter().position(|item| item.mode == mode)
}

pub fn nav_mode_for_index(index: usize) -> TuiMode {
    NAV_ITEMS
        .get(index)
        .map(|item| item.mode)
        .unwrap_or(TuiMode::Home)
}

pub fn nav_max_index() -> usize {
    NAV_ITEMS.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn nav_items_have_unique_modes_and_keys() {
        let mut mode_set = HashSet::new();
        let mut key_set = HashSet::new();
        for item in NAV_ITEMS {
            assert!(
                mode_set.insert(std::mem::discriminant(&item.mode)),
                "Duplicate mode in NAV_ITEMS: {:?}",
                item.mode
            );
            assert!(
                key_set.insert(item.key),
                "Duplicate key in NAV_ITEMS: {}",
                item.key
            );
        }
    }

    #[test]
    fn nav_index_roundtrips() {
        for (idx, item) in NAV_ITEMS.iter().enumerate() {
            assert_eq!(
                nav_index_for_mode(item.mode),
                Some(idx),
                "nav_index_for_mode mismatch for {:?}",
                item.mode
            );
            assert_eq!(
                nav_mode_for_index(idx),
                item.mode,
                "nav_mode_for_index mismatch for {}",
                idx
            );
        }
        assert_eq!(nav_max_index(), NAV_ITEMS.len().saturating_sub(1));
    }
}
