# Casparian Flow Website Spec

## Overview

Single-page marketing site for Casparian Flow - an AI-native CLI tool that transforms messy files into queryable datasets.

**Target**: Data engineers at mid-market companies who process custom file formats and have compliance/governance needs.

**Scope**: Minimal (Carrd-level). Single page, embedded payment, ship fast.

**Product Form**: CLI + TUI (Rust binary with optional terminal UI mode)

---

## Core Value Proposition

**One-liner**: "AI-generated parsers. Human-approved schemas. Local-first."

**The Problem**: Data engineers spend weeks writing custom parsers for proprietary file formats. When formats change, parsers break. No governance trail.

**The Solution**: Optional AI assistance (future) helps draft parsers. Humans approve schemas as immutable contracts. Everything runs locally - no cloud, no data leaving your machine.

---

## Page Structure

### Hero Section

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Turn messy files into queryable datasets.                      │
│  AI generates the parsers. You approve the schemas.             │
│                                                                 │
│  [Download CLI]          [Watch Demo →]                         │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  $ casparian run parser.py messy_data.csv               │    │
│  │  ✓ Schema locked: 12 columns, 3 constraints             │    │
│  │  ✓ Output: output/sales_001.parquet (2.4 MB)            │    │
│  │  Processing complete in 1.2s                            │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key elements**:
- Product name (no logo needed for v1)
- One-liner tagline
- Two CTAs: Download (primary), Watch Demo (secondary)
- Terminal animation or static screenshot showing the CLI in action

### How It Works (3 Steps)

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  1. Discover        2. Develop          3. Execute              │
│                                                                 │
│  Point at your      Draft a parser      Run locally.            │
│  messy files.       (AI assist is       Output to Parquet,      │
│  CSVs, JSON,        optional). You      SQLite, or CSV.         │
│  logs, custom       review and          Full lineage.           │
│  formats.           approve.            Air-gapped.             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Visual**: Three columns with icons. Keep it simple.

### Key Features (Bullets)

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  ✓ AI-Assist (Future): Optional parser drafting                │
│  ✓ Schema Contracts: Approved = immutable. No silent coercion.  │
│  ✓ Local-First: Your data never leaves your machine             │
│  ✓ BYOK: Bring your own Anthropic API key                       │
│  ✓ Lineage: Every row traced to source file + parser version    │
│  ✓ Fail-Fast: Test high-failure files first                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Demo Video Embed

30-60 second screen recording showing:
1. Point CLI at messy CSV
2. Claude Code generates parser
3. User approves schema in TUI
4. Output appears as Parquet
5. Quick DuckDB query showing clean data

**Placement**: After features, before pricing.

### Pricing Section

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Free                          Pro                              │
│  $0                            $29/month or $199/year           │
│                                                                 │
│  • 3 parsers                   • Unlimited parsers              │
│  • BYOK (your API key)         • BYOK (your API key)            │
│  • Parquet output              • All output formats             │
│  • Community support           • Priority support               │
│                                • Schema amendments              │
│                                • Backfill command               │
│                                                                 │
│  [Download Free]               [Buy Pro →]                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Payment**: Gumroad embed for Pro tier. License key delivered via email.

### Footer

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Docs  •  GitHub  •  Discord  •  support@casparian.dev          │
│                                                                 │
│  © 2025                                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Technical Requirements

### Platform
- **Builder**: Carrd.co (Pro tier for custom domain + Gumroad embed)
- **Domain**: TBD (suggestions: casparian.dev, casparianflow.com)
- **Payment**: Gumroad (handles payment, license key generation, delivery)

### Distribution
- **Binary hosting**: GitHub Releases (public repo for releases only)
- **Platforms**: macOS (arm64, x86_64), Linux (x86_64), Windows (x86_64)
- **Installation**: Direct download + optional shell script installer

### License Activation (v1 - Simple)
1. User purchases on Gumroad → receives license key via email
2. User runs: `casparian activate <license-key>`
3. CLI validates key against Gumroad API (one-time, cached locally)
4. Pro features unlocked

**Offline mode**: License cached in `~/.casparian_flow/license.json`. Re-validates weekly if online.

---

## Content Requirements

### Copy Tone
- Technical but not academic
- Direct, no fluff ("we" not "our innovative solution")
- Assumes reader knows what Parquet/SQL/CLI means
- Casey Muratori / Jon Blow energy - respect for craft

### Required Assets
1. **Terminal screenshot/animation**: Real CLI output, not mockup
2. **Demo video**: 30-60s screen recording (can be lo-fi for v1)
3. **Favicon**: Simple "CF" or flow icon

### SEO Basics
- Title: "Casparian Flow - AI-Generated Data Parsers"
- Description: "Transform messy files into queryable datasets. AI generates parsers, humans approve schemas. Local-first CLI for data engineers."
- No blog for v1 (ship fast)

---

## Analytics & Feedback

### Philosophy

Privacy is a feature, not a limitation. Target market (regulated industries, defense, healthcare) is allergic to tracking. All telemetry is opt-in, anonymous, and transparent.

**Trust statement** (show on website + CLI):
> Your data stays yours. Telemetry is opt-in and anonymous. Crash reports are opt-in. No file contents ever leave your machine. Works fully offline.

### Website Analytics

**Tool**: Plausible ($9/mo)
- Single script tag in Carrd
- No cookies, no banner needed, GDPR-compliant
- Tracks: pageviews, referrers, countries, devices, goal conversions

**Goals to track**:
1. Download button clicks
2. "Watch Demo" clicks
3. Gumroad checkout initiated
4. Gumroad purchase completed (via redirect URL)

### Crash Reporting (CLI)

**Tool**: Sentry (free tier: 5K errors/mo)

**Behavior**:
```
Error: Failed to parse schema

Would you like to send an anonymous crash report?
This helps us fix bugs. No file contents are included.

[y/N]:
```

**What's captured**:
- Stack trace
- CLI version
- OS/platform
- Anonymized error message (file paths stripped)

**Storage**: Preference saved in `~/.casparian_flow/config.toml`

### Usage Telemetry (CLI) - Opt-in

**Tool**: PostHog (free tier: 1M events/mo)

**First-run prompt**:
```
Casparian collects anonymous usage statistics to improve the product.

This includes: commands run, success/failure, CLI version, OS.
This does NOT include: file contents, paths, or any data you process.

Enable anonymous telemetry? [y/N]:
```

**Default**: OFF (critical for air-gapped market positioning)

**Events tracked** (if opted in):
| Event | Properties |
|-------|------------|
| `cli_started` | version, os, is_pro |
| `command_run` | command name, success/failure |
| `schema_approved` | column_count (no names) |
| `parser_limit_hit` | current_count (free tier signal) |
| `error_occurred` | error_type (no message) |

**User control**:
```bash
casparian config set telemetry false   # Disable
casparian config set telemetry true    # Enable
casparian config show                  # See all settings
```

### Feedback Collection

**Channels** (layered by friction):

| Channel | Trigger | Implementation |
|---------|---------|----------------|
| `casparian feedback` | User-initiated | Opens mailto: or Tally form |
| `[f]` key in TUI | User-initiated | Same as above |
| Error prompt | On failure | "Report this? [r] Report [i] Ignore" |
| NPS prompt | After 10 successful runs | "Recommend to colleague? [1-10]" |
| Email | Always available | support@casparian.dev |

**NPS prompt logic**:
- Trigger after 10 successful `casparian run` completions
- Show once, respect answer
- Low scores (1-6) → flag for founder follow-up email
- Don't ask again for 90 days

**Feedback command implementation**:
```bash
$ casparian feedback
# Option 1: Opens default mail client
#   To: support@casparian.dev
#   Subject: Feedback - Casparian v1.0.0 (macOS)
#   Body: [cursor here]

# Option 2: Opens browser to Tally/Typeform
```

### License Analytics

**v1 approach**: Custom activation endpoint + Gumroad dashboard

**Activation flow**:
```
POST https://api.casparian.dev/v1/activate
{
  "license_key": "GUM-xxxxx",
  "machine_id": "sha256(hostname+username)",  // Not PII
  "cli_version": "1.0.0",
  "os": "macos-arm64"
}

Response: { "valid": true, "tier": "pro", "expires": null }
```

**What this gives you**:
- Active install count (unique machine_ids)
- Version distribution
- Platform distribution
- Pro vs Free ratio

**Storage**: Simple Postgres or SQLite on a $5/mo VPS, or serverless (Supabase free tier)

### Monthly Cost

| Service | Cost |
|---------|------|
| Plausible | $9/mo |
| Sentry | Free |
| PostHog | Free |
| Activation endpoint | Free (Supabase) or $5/mo (VPS) |
| **Total** | **$9-14/mo** |

---

## What's NOT in v1

- Custom domain email (use Gmail/Fastmail)
- Blog
- Changelog page (link to GitHub releases)
- Documentation site (link to GitHub README for now)
- User accounts / dashboard
- Automatic updates
- Discord community (email-only support)
- In-app update notifications

---

## Success Metrics

### Primary (Weekly Review)
1. **Downloads**: GitHub release download count
2. **Activations**: Unique machine_ids hitting activation endpoint
3. **Pro conversions**: Gumroad purchases
4. **Crash rate**: Sentry error count / activations

### Secondary (Monthly Review)
5. **Retention proxy**: Repeat commands per machine_id (if telemetry opted in)
6. **Limit hits**: `parser_limit_hit` events (upgrade signal)
7. **NPS scores**: Average and distribution

### Vanity (Glance occasionally)
8. **Page visits**: Plausible dashboard
9. **Referrer breakdown**: Where are people coming from?

---

## Open Questions

These need answers before building:

1. **Domain**: What domain to use? (casparian.dev, casparianflow.com, other?)
2. **Demo video**: Record now or placeholder "Coming soon"?
3. **GitHub**: Public repo for releases, or private with download links?
4. **Discord**: Set up community Discord, or email-only support for v1?
5. **Docs**: GitHub README sufficient, or need a docs site?

---

## Implementation Checklist

### Website & Payment
```
[ ] Purchase domain
[ ] Set up Carrd Pro account ($19/yr)
[ ] Create Gumroad product (Free + Pro tiers)
[ ] Record demo video (or take screenshots)
[ ] Write final copy (hero, features, pricing)
[ ] Build page in Carrd
[ ] Connect domain to Carrd
[ ] Embed Gumroad payment
[ ] Set up GitHub releases for binary distribution
```

### Analytics & Feedback
```
[ ] Set up Plausible account, add script to Carrd
[ ] Configure Plausible goals (download, demo, checkout)
[ ] Set up Sentry project for Rust CLI
[ ] Integrate Sentry SDK in CLI (opt-in prompt)
[ ] Set up PostHog project
[ ] Implement telemetry opt-in prompt in CLI
[ ] Implement `casparian feedback` command
[ ] Implement `[f]` feedback key in TUI
[ ] Implement NPS prompt (after 10 runs)
[ ] Set up support@casparian.dev email
```

### License & Activation
```
[ ] Create activation endpoint (Supabase or VPS)
[ ] Implement `casparian activate <key>` command
[ ] Implement license caching in ~/.casparian_flow/
[ ] Implement weekly re-validation (when online)
[ ] Implement `casparian config` commands
[ ] Test purchase → activation flow end-to-end
```

### Launch
```
[ ] Verify all analytics flowing
[ ] Test crash reporting manually
[ ] Test feedback command
[ ] Launch
```

---

## Appendix: Competitive Positioning

**Not competing with**: Fivetran, Airbyte, dbt (cloud-first, connector catalogs)

**Competing with**: Custom Python scripts, Pandas notebooks, "we'll do it manually"

**Differentiation**:
- AI generates the parser, not you
- Schema contracts = governance for regulated industries
- Local-first = air-gapped deployments possible
- BYOK = you control LLM costs

---

## Appendix: Target User Persona

**Name**: Alex, Senior Data Engineer

**Company**: 200-person logistics company

**Problem**: Receives weekly Excel exports from 15 vendors, each with slightly different formats. Spends 2 days/week maintaining parser scripts. No documentation. When formats change, production breaks.

**Why Casparian**:
- Claude Code generates parsers in minutes
- Schema contracts catch format changes before production
- Audit trail for compliance
- Runs on their air-gapped on-prem server

**Objections**:
- "AI-generated code? What about quality?" → Human approval step, schema contracts
- "Do I need to send data to the cloud?" → No, local-first, BYOK
- "What if the parser breaks?" → Fail-fast backtest, high-failure tracking
