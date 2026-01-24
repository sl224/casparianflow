# ADR-020: Tauri GUI for Trade Support Persona

**Status:** Accepted
**Date:** January 20, 2026
**Decision Makers:** Shan

---

## Context

Casparian Flow was originally designed as a CLI/TUI application. The assumption was that Trade Support Analysts would be comfortable with command-line tools because they "grep logs" as part of their workflow.

In January 2026, we conducted deep research to validate this assumption before committing to a launch strategy.

## Research Findings

### Original Assumption

Trade Support Analysts use command-line tools (grep, awk, sed) for FIX log analysis.

### Actual Finding

**FALSE.** Trade Support Analysts are **Excel/VBA users**, not command-line users.

#### Evidence

| Source | Key Quote |
|--------|-----------|
| [Goodman Masson - Day in the Life](https://www.goodmanmasson.com/the-insights-hub/of-a-trade-support-analyst) | "Strong Excel skills are highly sought after, so if you have experience of Macros (VBA) and are comfortable in manipulating data, this will support your case." |
| [Velvet Jobs - Job Description](https://www.velvetjobs.com/job-descriptions/trade-support-analyst) | "Trade support analysts provide ad hoc analysis across various platforms to extract data using SQL, Excel, VBA and a number of internal utilities." |
| [Wall Street Oasis Forum](https://www.wallstreetoasis.com/forum/trading/trading-support-analyst-excel-vba-requirement) | "VBA definitely helps on a trading desk as well as in a trade support capacity. You are dealing with a lot of spreadsheets and data manipulation." |
| Job Postings (JPM, Citi, SocGen) | Technical requirements: Excel, VBA, SQL. Unix/Linux NOT mentioned for Trade Support roles. |

#### Critical Distinction: Two Different Roles

The research revealed that "Trade Support" and "Application Support" are often conflated but are distinct roles:

| Role | Department | Primary Tools | CLI Comfort |
|------|------------|---------------|-------------|
| **Trade Support Analyst** | Operations/Middle Office | Excel, VBA, Bloomberg Terminal | **Low** |
| Application Support Analyst | IT/Technology | Unix, shell scripting, Python | **High** |

Our target persona (Trade Support Analyst) is in Operations, not IT. They investigate trade breaks from a **business/operations perspective**, not a systems perspective.

#### GUI Tools Exist in the Market

FIX log analysis GUI tools already exist, indicating market expectation for visual interfaces:

- [FIXViewer](https://www.fixviewer.com/) - Free .NET desktop application
- [OnixS FIX Analyser](https://www.onixs.biz/fix-analyser.html) - Enterprise GUI tool
- [LogViewPlus](https://www.logviewplus.com/fix-message-parser.html) - Desktop app with FIX parser
- [QuantScopeApp](https://github.com/AquibPy/QuantScopeApp) - Open source GUI

## Decision

**Build a Tauri-based GUI application** as the primary interface for Trade Support Analysts.

- **Tauri** chosen over Electron for smaller binary size and Rust backend compatibility
- CLI remains available for power users and automation
- TUI can be deprecated or maintained as secondary interface

## Rationale

1. **Persona fit:** Trade Support Analysts expect GUI tools (they use Excel and Bloomberg Terminal all day)

2. **Adoption friction:** CLI-only product will face resistance from non-technical operations staff

3. **Demo effectiveness:** GUI provides better "show don't tell" for sales conversations

4. **Competitive positioning:** Existing FIX analysis tools are GUI-based; CLI would be a step backward

5. **LLM-assisted development:** Modern LLMs (Claude) can rapidly generate React/TypeScript UI code, reducing development time

## Consequences

### Positive

- Better product-market fit for Trade Support persona
- Easier onboarding (drag & drop vs. command line)
- More compelling demos for sales
- Aligns with existing tools in the market

### Negative

- Additional development time (estimated 4-8 weeks for MVP)
- Cross-platform packaging complexity (macOS, Windows, Linux)
- Two codebases to maintain (Rust backend + TypeScript frontend)

### Neutral

- CLI remains available for automation and power users
- Backend architecture unchanged (Rust core, Arrow IPC)

## Implementation Notes

### Minimal Viable GUI Features (MVP)

1. **File Import:** Drag & drop FIX log files
2. **Auto-Parse:** Detect FIX format, run parser automatically
3. **Results View:** Display `fix_order_lifecycle` table
4. **Search/Filter:** Filter by ClOrdID, symbol, status
5. **Query Panel:** Basic SQL query interface

### Tech Stack

- **Frontend:** React + TypeScript + Tailwind CSS
- **Backend:** Existing Rust core via Tauri commands
- **IPC:** Tauri's invoke system (JSON over IPC)
- **State:** React Query or Zustand for data fetching

### Migration Path

1. Build Tauri MVP with core FIX workflow
2. Soft launch to early adopters
3. Gather feedback
4. Iterate on UI/UX
5. Deprecate TUI if adoption confirms GUI preference

## Alternatives Considered

| Alternative | Rejected Because |
|-------------|------------------|
| **CLI-only** | Research shows Trade Support analysts are not CLI users |
| **TUI-only** | Still terminal-based; doesn't match Excel/Bloomberg workflow |
| **Electron** | Larger binary size; no Rust backend synergy |
| **Web app (SaaS)** | Violates local-first principle; data sovereignty concerns |

## References

- [strategies/finance.md Section 4.1](../../strategies/finance.md) - Validated persona research
- [Dev.to - Debugging FIX Logs](https://dev.to/aquib_sayyed_0187c2b9dc22/debugging-fix-logs-is-a-pain-heres-a-simple-tool-to-help-19ka) - Market evidence for GUI tools

---

## Revision History

| Date | Change |
|------|--------|
| 2026-01-20 | Initial decision based on persona research |
