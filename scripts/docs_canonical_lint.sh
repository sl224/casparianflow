#!/usr/bin/env bash
#
# docs_canonical_lint.sh - Lint canonical docs for outdated terms and broken links
#
# Usage:
#   ./scripts/docs_canonical_lint.sh         # Full check
#   ./scripts/docs_canonical_lint.sh --terms # Only check outdated terms
#   ./scripts/docs_canonical_lint.sh --links # Only check broken links
#
# Exit codes:
#   0 - All checks passed
#   1 - Found issues

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
TERM_ISSUES=0
LINK_ISSUES=0

# Directories to exclude from checks (archive folders)
EXCLUDE_DIRS=(
    "docs/archive"
    "specs/archive"
)

# Build exclude pattern for grep
build_exclude_pattern() {
    local pattern=""
    for dir in "${EXCLUDE_DIRS[@]}"; do
        pattern="${pattern}--exclude-dir=${dir} "
    done
    echo "$pattern"
}

# Outdated terms that should not appear in canonical docs
OUTDATED_TERMS=(
    "SQLite"
    "sqlx"
    "AF_UNIX"
    "localhost:5000"
    "rusqlite"
    "SqlitePool"
    "casparian_scout"  # It's a module, not a crate
)

# Check for outdated terms in docs
check_outdated_terms() {
    echo "Checking for outdated terms in canonical docs..."
    echo ""

    local exclude_pattern
    exclude_pattern=$(build_exclude_pattern)

    for term in "${OUTDATED_TERMS[@]}"; do
        # Search in markdown files, excluding archive directories
        local results
        results=$(grep -rn --include="*.md" $exclude_pattern "$term" "$REPO_ROOT/docs" "$REPO_ROOT/specs" "$REPO_ROOT/CLAUDE.md" "$REPO_ROOT/ARCHITECTURE.md" "$REPO_ROOT/README.md" 2>/dev/null || true)

        # Also check crate CLAUDE.md files
        local crate_results
        crate_results=$(grep -rn --include="CLAUDE.md" "$term" "$REPO_ROOT/crates" 2>/dev/null || true)

        if [[ -n "$results" ]] || [[ -n "$crate_results" ]]; then
            echo -e "${YELLOW}WARNING:${NC} Found '$term' in canonical docs:"
            if [[ -n "$results" ]]; then
                echo "$results" | while read -r line; do
                    echo "  $line"
                done
            fi
            if [[ -n "$crate_results" ]]; then
                echo "$crate_results" | while read -r line; do
                    echo "  $line"
                done
            fi
            echo ""
            ((TERM_ISSUES++)) || true
        fi
    done

    if [[ $TERM_ISSUES -eq 0 ]]; then
        echo -e "${GREEN}No outdated terms found.${NC}"
    else
        echo -e "${RED}Found $TERM_ISSUES outdated term(s).${NC}"
    fi
    echo ""
}

# Check for broken markdown links
check_broken_links() {
    echo "Checking for broken markdown links..."
    echo ""

    local docs_dirs=("$REPO_ROOT/docs" "$REPO_ROOT/specs")
    local root_docs=("$REPO_ROOT/CLAUDE.md" "$REPO_ROOT/ARCHITECTURE.md" "$REPO_ROOT/README.md")

    # Find all markdown files
    local md_files=()
    for dir in "${docs_dirs[@]}"; do
        while IFS= read -r -d '' file; do
            md_files+=("$file")
        done < <(find "$dir" -name "*.md" -print0 2>/dev/null)
    done
    for file in "${root_docs[@]}"; do
        if [[ -f "$file" ]]; then
            md_files+=("$file")
        fi
    done

    for file in "${md_files[@]}"; do
        local dir
        dir=$(dirname "$file")

        # Extract markdown links [text](path) - only relative paths, skip URLs
        local links
        links=$(grep -oE '\[([^\]]+)\]\(([^)]+)\)' "$file" 2>/dev/null | \
                grep -oE '\(([^)]+)\)' | \
                sed 's/[()]//g' | \
                grep -v '^http' | \
                grep -v '^#' | \
                grep -v '^mailto:' || true)

        while IFS= read -r link; do
            [[ -z "$link" ]] && continue

            # Remove anchor from link
            local path="${link%%#*}"
            [[ -z "$path" ]] && continue

            # Resolve relative path
            local target
            if [[ "$path" == /* ]]; then
                target="$REPO_ROOT$path"
            else
                target="$dir/$path"
            fi

            # Normalize path
            target=$(realpath "$target" 2>/dev/null || echo "")

            if [[ -z "$target" ]] || [[ ! -e "$target" ]]; then
                echo -e "${YELLOW}WARNING:${NC} Broken link in $file"
                echo "  Link: $link"
                echo ""
                ((LINK_ISSUES++)) || true
            fi
        done <<< "$links"
    done

    if [[ $LINK_ISSUES -eq 0 ]]; then
        echo -e "${GREEN}No broken links found.${NC}"
    else
        echo -e "${RED}Found $LINK_ISSUES broken link(s).${NC}"
    fi
    echo ""
}

# Main
main() {
    echo "=========================================="
    echo "Casparian Flow Documentation Lint"
    echo "=========================================="
    echo ""

    local check_terms=true
    local check_links=true

    # Parse arguments
    for arg in "$@"; do
        case "$arg" in
            --terms)
                check_links=false
                ;;
            --links)
                check_terms=false
                ;;
            --help|-h)
                echo "Usage: $0 [--terms] [--links]"
                echo ""
                echo "Options:"
                echo "  --terms  Only check for outdated terms"
                echo "  --links  Only check for broken links"
                echo "  --help   Show this help message"
                exit 0
                ;;
        esac
    done

    if $check_terms; then
        check_outdated_terms
    fi

    if $check_links; then
        check_broken_links
    fi

    # Summary
    echo "=========================================="
    echo "Summary"
    echo "=========================================="

    local total_issues=$((TERM_ISSUES + LINK_ISSUES))
    if [[ $total_issues -eq 0 ]]; then
        echo -e "${GREEN}All checks passed!${NC}"
        exit 0
    else
        echo -e "${RED}Total issues: $total_issues${NC}"
        echo "  Outdated terms: $TERM_ISSUES"
        echo "  Broken links: $LINK_ISSUES"
        exit 1
    fi
}

main "$@"
