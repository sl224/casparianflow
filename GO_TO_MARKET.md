# Casparian Flow - Go-To-Market Strategy

**Last Updated:** January 2025
**Purpose:** Business strategy, pricing, licensing, and launch plan for solo developer.

---

## Executive Summary

**Product:** Desktop app that transforms messy files into queryable data using AI-assisted parser generation.

**Target:** Data engineers and analysts with scattered files (CSVs, JSON, logs) who need to query them.

**Model:** Freemium with feature gating. Free tier is genuinely useful. Pro tier adds convenience (hosted AI, no API key needed).

**Launch Strategy:** Carrd landing page + Gumroad payments + direct community outreach. No custom website needed.

---

## Table of Contents

1. [Pricing Strategy](#1-pricing-strategy)
2. [Feature Gating](#2-feature-gating)
3. [Licensing Implementation](#3-licensing-implementation)
4. [Website & Distribution](#4-website--distribution)
5. [Launch Plan](#5-launch-plan)
6. [Data Collection Decision](#6-data-collection-decision)
7. [Phase Roadmap](#7-phase-roadmap)
8. [Key Decisions](#8-key-decisions)
9. [Next Actions](#9-next-actions)

---

## 1. Pricing Strategy

### Tiers

| Tier | Price | Target User |
|------|-------|-------------|
| **Free** | $0 | Evaluators, hobbyists, privacy-conscious |
| **Pro** | $29/mo or $199/yr | Individual professionals |
| **Team** | $99/mo | Small teams (future) |

### Why These Prices

- **$29/mo** is standard for developer tools (similar to Retool, Postman paid tiers)
- **$199/yr** gives 43% discount, encourages annual commitment
- Data tools that save time are worth money - don't underprice

### Alternative: Lifetime Deal (Launch Only)

```
First 100 customers: $299 lifetime
- Good for initial cash + validation
- Creates urgency
- Risk: support burden forever

After 100: Switch to subscription only
```

---

## 2. Feature Gating

### What's in Each Tier

| Feature | Free | Pro |
|---------|------|-----|
| **Parsers** | 3 | Unlimited |
| **Source folders** | 1 | Unlimited |
| **AI generation** | BYOK (own API key) | Hosted (no key needed) |
| **Export formats** | CSV | Parquet, SQLite, cloud |
| **File size** | 100MB | Unlimited |
| **Rows per file** | 50,000 | Unlimited |
| **Support** | Community (GitHub) | Email (48hr response) |
| **Updates** | Yes | Yes |

### Natural Gates (Can't Be Bypassed)

These features inherently require your infrastructure:

1. **Hosted AI** - Uses your API key, your optimized prompts
2. **Cloud sync** - Requires your server
3. **Priority support** - Requires you (human)

### Soft Gates (Enforced in Code)

These are checked before operations:

1. **Parser count** - Check before `create_parser`
2. **Source count** - Check before `add_source`
3. **Export formats** - Feature flag per format
4. **File/row limits** - Check during parse operation

### Implementation Note

Free tier should be **genuinely useful**, not crippled. 3 parsers + 1 folder + 50K rows is enough to:
- Evaluate the product thoroughly
- Handle small personal projects
- Decide if Pro is worth it

---

## 3. Licensing Implementation

### Recommended Approach: Hybrid Validation

```
┌─────────────────────────────────────────────────────────────────┐
│                     LICENSE FLOW                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  User purchases on Gumroad                                      │
│           ↓                                                     │
│  Receives license key via email                                 │
│           ↓                                                     │
│  Enters key in app Settings                                     │
│           ↓                                                     │
│  App validates online (Gumroad API)                             │
│           ↓                                                     │
│  Success → Cache locally (signed)                               │
│           ↓                                                     │
│  Pro features unlocked                                          │
│           ↓                                                     │
│  Re-validate every 30 days                                      │
│  (7-day grace period if offline)                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Validation Strategy

| Aspect | Decision | Rationale |
|--------|----------|-----------|
| **Initial validation** | Online (Gumroad API) | Simple, handles edge cases |
| **Ongoing validation** | Every 30 days | Catches cancelled subscriptions |
| **Offline support** | 7-day grace period | Respects users on planes, etc. |
| **Hardware locking** | No | Annoying, not worth the protection |
| **Obfuscation** | Minimal | Technical users can crack anyway |

### Storage

License data stored in:
- Primary: `~/.casparian_flow/casparian_flow.sqlite3` (existing DB)
- Backup: OS keychain (macOS Keychain / Windows Credential Manager)

### Key Files to Implement

```
ui/src-tauri/src/license.rs        - License validation logic
ui/src-tauri/src/scout.rs          - Add license check to gated commands
ui/src/lib/components/LicenseSettings.svelte - UI for activation
```

### Gumroad Integration

```rust
// Validate against Gumroad API
POST https://api.gumroad.com/v2/licenses/verify
{
  "product_id": "YOUR_PRODUCT_ID",
  "license_key": "USER_KEY",
  "increment_uses_count": "true"
}

// Response includes:
// - success: bool
// - purchase.email: string
// - purchase.refunded: bool
// - purchase.subscription_ended_at: string | null
```

---

## 4. Website & Distribution

### Recommended Stack

| Component | Tool | Cost | Time to Setup |
|-----------|------|------|---------------|
| Landing page | Carrd | $19/year | 2-3 hours |
| Payments | Gumroad | 10% + fees | 1 hour |
| License delivery | Gumroad (automatic) | Included | 0 |
| Domain | Namecheap/Cloudflare | ~$12/year | 30 min |
| Analytics | Plausible or none | $0-9/mo | 30 min |

**Total: ~$40/year + payment fees**

### Landing Page Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                     CASPARIAN FLOW                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [HERO]                                                         │
│  "Turn messy files into queryable data."                        │
│  One line: what it does                                         │
│                                                                 │
│  [DEMO VIDEO - 60-90 seconds]                                   │
│  Show: drag folder → AI generates parser → query data           │
│                                                                 │
│  [3 KEY FEATURES]                                               │
│  Scout: Auto-discover files                                     │
│  Parser Lab: AI generates parsers from samples                  │
│  Query: SQL your data in seconds                                │
│                                                                 │
│  [DOWNLOAD]                                                     │
│  macOS (Apple Silicon) | macOS (Intel) | Windows | Linux        │
│                                                                 │
│  [PRICING]                                                      │
│  Free: 3 parsers, bring your own API key                        │
│  Pro $29/mo: Unlimited, hosted AI, priority support             │
│                                                                 │
│  [SOCIAL PROOF - add later]                                     │
│  Testimonials, GitHub stars, company logos                      │
│                                                                 │
│  [ABOUT]                                                        │
│  Built by [Name] - [Twitter] [GitHub] [Email]                   │
│  Solo developer = fast iteration, direct support                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### What NOT to Build

- Custom website with Next.js/React - overkill
- Blog with 50 posts - time sink
- Fancy animations - doesn't convert
- LLM-generated generic copy - looks soulless

### Domain Options

- `casparian.dev` (clean, technical)
- `casparianflow.com` (full name)
- `getparsers.dev` (action-oriented)

---

## 5. Launch Plan

### Week 1: Pre-Launch

```
Day 1-2: Record demo video (2 minutes max)
         - Show complete flow: folder → parser → query
         - No fancy editing, just screen recording + voiceover
         - Tools: OBS (free) or Loom

Day 3:   Set up Carrd landing page
         - Embed demo video
         - Download links (GitHub releases or direct)
         - Gumroad "Buy" button

Day 4:   Set up Gumroad product
         - Product description
         - Pricing tiers
         - License key delivery

Day 5:   Test the flow
         - Buy your own product
         - Activate license
         - Make sure everything works
```

### Week 2: Launch

```
Day 1:   Submit to Hacker News
         - Title: "Show HN: Casparian Flow – AI-powered file parser for messy data"
         - Be online to answer comments for 4-6 hours

Day 2:   Post to Reddit
         - r/dataengineering
         - r/selfhosted
         - r/commandline

Day 3:   Tweet/LinkedIn
         - Thread showing the demo
         - Tag relevant people/accounts

Day 4:   Product Hunt
         - Schedule launch (Tuesday-Thursday best)
         - Prepare assets (logo, screenshots)

Day 5-7: Respond to feedback
         - Fix bugs reported
         - Answer questions
         - Note feature requests
```

### Launch Checklist

```
Pre-launch:
[ ] Demo video recorded and edited
[ ] Carrd page live
[ ] Gumroad product configured
[ ] Download links working (all platforms)
[ ] License activation tested end-to-end
[ ] README updated with clear install instructions
[ ] GitHub repo cleaned up (no secrets, good .gitignore)

Launch day:
[ ] HN post ready (save draft)
[ ] Reddit posts ready
[ ] Twitter thread drafted
[ ] Calendar blocked for 6 hours
[ ] Coffee ready
```

---

## 6. Data Collection Decision

### Options Considered

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **BYOK + silent data capture** | Free AI, training data | Trust erosion, complexity, uncertain value | **No** |
| **BYOK + explicit opt-in** | Transparent | Low opt-in rate | Maybe later |
| **Contributor program** | High-quality data, community | Requires user base first | Phase 3 |
| **No collection, just charge** | Simple, trustworthy | No training data | **Yes (for now)** |

### Decision: No Silent Collection

**Reasons:**
1. Trust is the moat for solo devs - don't erode it
2. Data isn't valuable until you have 1000+ data points
3. Complexity is certain, value is uncertain
4. There's a better path (see Phase Roadmap)

### Future: Contributor Program (Phase 3)

After reaching 100+ active users, launch explicit program:

```
CASPARIAN CONTRIBUTORS

What you share:
- Anonymized schema patterns (types, not names)
- Parser fixes you make
- Success/failure data

What you get:
- Pro tier free forever ($228/year value)
- Name in contributors list
- Early access to features
- Direct Slack channel

Limited to 100 contributors.
```

---

## 7. Phase Roadmap

### Phase 1: Launch (Now → 100 Users)

**Focus:** Get the product in front of people, validate demand.

```
Goals:
- 100 active users (free + paid)
- 10 paying customers
- Understand real use cases

Actions:
- Ship MVP with feature gating
- Carrd + Gumroad setup
- HN/Reddit/PH launch
- Respond to all feedback

Don't:
- Build custom website
- Add data collection
- Over-engineer licensing
```

### Phase 2: Revenue (100 → 500 Users)

**Focus:** Convert free users to paid, find product-market fit.

```
Goals:
- 50 paying customers (~$1,450/mo MRR)
- 4.5+ star rating
- Clear understanding of "why people pay"

Actions:
- Improve Pro tier value (hosted AI, better prompts)
- Add requested features
- Testimonials on landing page
- Maybe: affiliate/referral program

Don't:
- Premature optimization
- Enterprise features
- Big marketing spend
```

### Phase 3: Scale (500 → 2000 Users)

**Focus:** Sustainable growth, community building.

```
Goals:
- 200 paying customers (~$5,800/mo MRR)
- Contributor program with 50 active members
- Self-sustaining word-of-mouth

Actions:
- Launch Contributor Program
- Use collected data to improve AI
- Maybe: Team tier
- Maybe: Custom website (now justified)

Don't:
- Hire before profitable
- VC if lifestyle business works
```

---

## 8. Key Decisions

### Decided

| Question | Decision | Rationale |
|----------|----------|-----------|
| Website platform | Carrd | Fast, cheap, good enough |
| Payment processor | Gumroad | Simple, handles VAT, license keys |
| Free tier | Yes, genuinely useful | Trust builder, evaluation path |
| AI in free tier | BYOK only | Natural gate, user pays API cost |
| Data collection | Not yet | Trust > data, revisit in Phase 3 |
| Licensing | Online + cached | Balance of security and UX |
| Hardware locking | No | Annoys paying customers |
| Launch channel | HN first | Technical audience, free |

### Open Questions

| Question | Options | Decide By |
|----------|---------|-----------|
| Lifetime deal? | Yes (first 100) / No | Before launch |
| Annual discount | 30% / 40% / 50% | Before launch |
| Team tier pricing | $79 / $99 / $149 | Phase 2 |
| Custom domain | casparian.dev / other | This week |

---

## 9. Next Actions

### Immediate (This Week)

```
[ ] Record 2-minute demo video
    - Script the flow: folder → AI parser → query
    - Use OBS or Loom
    - Keep it simple, no fancy editing

[ ] Set up Carrd landing page
    - Get template
    - Add copy, embed video, download links
    - Connect to domain

[ ] Set up Gumroad
    - Create product
    - Configure pricing
    - Set up license key format

[ ] Implement basic license checking
    - Add license.rs to Tauri backend
    - Add activation UI
    - Gate Pro features
```

### Before Launch

```
[ ] Test full purchase flow
[ ] Test license activation on fresh install
[ ] Prepare HN post draft
[ ] Prepare Reddit posts
[ ] Clean up GitHub repo
[ ] Update README with install instructions
```

### After Launch

```
[ ] Monitor HN comments (6+ hours)
[ ] Fix critical bugs same-day
[ ] Collect feedback in one place
[ ] Follow up with early customers
[ ] Write retrospective
```

---

## Appendix: Pricing Comparison

### Similar Products

| Product | Free Tier | Paid Tier | Notes |
|---------|-----------|-----------|-------|
| **Retool** | Limited | $10/user/mo | Dev tools |
| **Postman** | Limited | $14/user/mo | API tools |
| **DBeaver** | CE (free) | $25/user/mo | Database |
| **TablePlus** | Trial | $89 lifetime | Database |
| **DataGrip** | None | $25/mo | JetBrains |

### Positioning

Casparian Flow at **$29/mo** is:
- Cheaper than DataGrip ($25/mo per tool)
- Similar to Retool/Postman
- Premium to free-only tools

**Value proposition:** "Saves 2+ hours per week on data wrangling = easily worth $29/mo for professionals."

---

## Appendix: License Key Format

### Gumroad Keys

Gumroad generates keys like: `XXXXXXXX-XXXXXXXX-XXXXXXXX-XXXXXXXX`

These can be validated via API without storing anything on your server.

### If You Need Custom Keys Later

```
Format: BASE64(payload).BASE64(signature)

Payload (JSON):
{
  "email": "user@example.com",
  "tier": "pro",
  "issued_at": 1704412800,
  "expires_at": null,  // null = lifetime
  "machine_limit": 3
}

Signature: Ed25519 sign(payload, private_key)

Validation: Ed25519 verify(payload, signature, public_key)
```

Only implement this if Gumroad's keys become insufficient.

---

## Changelog

| Date | Change |
|------|--------|
| 2025-01 | Initial strategy from product discussion |
