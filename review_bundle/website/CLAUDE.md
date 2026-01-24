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
3. Read `../strategies/finance.md` for finance vertical specifics (v1 focus)

## Key Principles

- **Local-first**: Data never leaves the user's machine
- **Premade parsers**: FIX, HL7, PST, CoT - not generic ETL
- **Schema contracts**: Governance and audit trails built-in
- **v1 = Finance**: Trade Break Workbench for FIX logs

## Deployment

See `DEPLOYMENT.md` for Netlify setup and placeholder replacements.
