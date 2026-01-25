# Casparian Flow Website

This directory contains the static marketing website for Casparian Flow.

## What This Directory Contains

- **HTML pages**: `index.html`, `finance.html`, `healthcare.html`, `legal.html`, `defense.html`
- **Assets**: `assets/favicon.svg`
- **Deployment config**: `netlify.toml`, `_redirects`
- **Deployment docs**: `README.md`, `DEPLOYMENT.md`, `MARKETING_PLAN.md`

## Documentation (Single Source of Truth)

All product and strategy documentation lives in the **parent directory**. Do not create duplicate docs here.

| Topic | Location |
|-------|----------|
| Product specification | `../spec.md` |
| Website specification | `../specs/website.md` |
| Business strategy | `../STRATEGY.md` |
| Vertical strategies | `../strategies/*.md` |
| v1 scope | `../docs/v1_scope.md` |
| Full LLM context | `../CLAUDE.md` |

## Writing Website Copy

When editing HTML content:

1. Read `../specs/website.md` for page structure and messaging
2. Read `../STRATEGY.md` for positioning and value props
3. Read `../strategies/dfir.md` for DFIR vertical specifics (v1 focus)

## Key Principles

- **Local-first**: Data never leaves the user's machine
- **Premade parsers**: EVTX, Prefetch, HL7, CoT - not generic ETL
- **Schema contracts**: Governance and audit trails built-in
- **v1 = DFIR**: Evidence-grade artifact parsing with lineage, quarantine, reproducibility

## Deployment

See `DEPLOYMENT.md` for Netlify setup and placeholder replacements.
