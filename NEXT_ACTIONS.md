# Next Actions - Prioritized

**Last Updated:** January 2025

Quick reference for what to do next. Details in `GAP_ANALYSIS.md` and `GO_TO_MARKET.md`.

---

## Track 1: Ship & Sell (Revenue First)

The product works. Get it in front of people.

### This Week

```
[ ] 1. Record demo video (2 min)
       - Flow: drag folder → AI generates parser → query data
       - Tool: OBS or Loom
       - No fancy editing, just clarity

[ ] 2. Carrd landing page (2-3 hours)
       - Hero + demo video
       - 3 features
       - Download links
       - Pricing

[ ] 3. Gumroad setup (1 hour)
       - Product: "Casparian Flow Pro"
       - Price: $29/mo or $199/yr
       - License key delivery: automatic

[ ] 4. Basic license check in app (1 day)
       - Tauri command: activate_license, get_license_status
       - UI: Settings → License
       - Gate: AI generation requires Pro OR user's API key
```

### Next Week

```
[ ] 5. Test purchase flow end-to-end
[ ] 6. Clean up GitHub repo (README, .gitignore)
[ ] 7. Launch on Hacker News (Show HN)
[ ] 8. Post to r/dataengineering, r/selfhosted
[ ] 9. Product Hunt submission
```

---

## Track 2: Product Improvements (After Launch Feedback)

Based on GAP_ANALYSIS.md. Only do these after you have users.

### Priority 1: Output Preview (Most Requested, Probably)

```
[ ] Structured validation output (backend)
    - Return JSON not raw text
    - Include: rows[], errors[], schema{}
    - File: ui/src-tauri/src/scout.rs

[ ] Raw file preview pane (frontend)
    - Show first 100 rows of input
    - File: ui/src/lib/components/parser-lab/FileEditor.svelte

[ ] Parsed output as table (frontend)
    - Render parsed data with types
    - Show errors inline
```

### Priority 2: Backtest

```
[ ] parser_lab_backtest command
    - Run parser against all matching files
    - Return per-file results

[ ] Backtest results UI
    - File list with success/failure
    - Error drill-down
```

### Priority 3: AI Generation (Core Differentiator)

```
[ ] parser_lab_generate_parser command
    - Read file sample
    - Call LLM (Anthropic API)
    - Return generated Python

[ ] Connect ParserChat.svelte to backend
    - Currently a stub
    - Needs actual AI integration
```

---

## Decision Log

| Decision | Choice | Date |
|----------|--------|------|
| Website | Carrd ($19/yr) | Jan 2025 |
| Payments | Gumroad | Jan 2025 |
| Free tier AI | BYOK (user's API key) | Jan 2025 |
| Data collection | No (revisit Phase 3) | Jan 2025 |
| Licensing | Online + 30-day cache | Jan 2025 |
| Launch channel | HN first | Jan 2025 |

---

## Don't Do Yet

These are documented but not priority:

- [ ] Custom website (Carrd is fine)
- [ ] Team tier (need individual customers first)
- [ ] Data collection / contributor program (need users first)
- [ ] Schema drift detection (Phase 7 feature)
- [ ] Auto-approve / red button (Phase 5 feature)
- [ ] Hardware locking (not worth the UX cost)

---

## Reference Docs

| Doc | Purpose |
|-----|---------|
| `PRODUCT_NORTH_STAR.md` | Vision, UX principles, feature roadmap |
| `GAP_ANALYSIS.md` | Technical gaps vs vision |
| `GO_TO_MARKET.md` | Business strategy, pricing, launch |
| `CLAUDE.md` | Development instructions |
