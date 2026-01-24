# Casparian Flow Website

Static marketing website for Casparian Flow. Hosted on Netlify.

## Quick Status

| Vertical | Status | Page |
|----------|--------|------|
| Finance (FIX Logs) | **LIVE** | `/finance` |
| Healthcare (HL7) | Coming Q2 2026 | `/healthcare` |
| Legal (PST/eDiscovery) | Coming Q2 2026 | `/legal` |
| Defense (CoT/PCAP) | Coming 2026 | `/defense` |

## Before You Launch

See `DEPLOYMENT.md` for critical blockers:
1. **Demo video** - Record and embed
2. **GitHub Releases** - Build CLI binaries
3. **Stripe Payment Links** - Create 4 links
4. **Tally Forms** - Create 3 waitlist forms

## Marketing & Launch

See `MARKETING_PLAN.md` for:
- LinkedIn/Reddit/HN outreach strategies
- Cold email templates
- Vertical launch criteria
- Pricing rationale

## Documentation

All product documentation lives in the parent directory:

| Topic | Location |
|-------|----------|
| Website spec (pages, copy, structure) | `../specs/website.md` |
| Product specification | `../spec.md` |
| Business strategy | `../STRATEGY.md` |
| v1 scope | `../docs/v1_scope.md` |
| Full LLM context | `../CLAUDE.md` |

## Structure

```
website/
├── index.html          # Homepage (vertical selector)
├── finance.html        # Finance - Trade Break Workbench (LIVE)
├── healthcare.html     # Healthcare - HL7 (Coming Soon)
├── defense.html        # Defense - Air-gapped (Coming Soon)
├── legal.html          # Legal - eDiscovery (Coming Soon)
├── _redirects          # Netlify URL rewrites
├── netlify.toml        # Netlify configuration
├── DEPLOYMENT.md       # Full deployment guide
└── assets/
    └── favicon.svg     # Browser tab icon
```

## Quick Deploy

### Option 1: Drag and Drop
1. Go to [netlify.com](https://netlify.com)
2. Drag this entire `website` folder to the deploy zone
3. Done!

### Option 2: Git Deploy
```bash
# From this directory
git init
git add .
git commit -m "Initial website"

# Create GitHub repo and push
gh repo create casparian-website --private --source=. --push

# Then connect Netlify to GitHub repo
```

### Option 3: Netlify CLI
```bash
# Install CLI
npm install -g netlify-cli

# Deploy
netlify deploy --prod
```

## Before Deploying

Update these placeholders in the HTML files:

### 1. Tally Form IDs
Search for `TALLY_` and replace with your actual Tally form IDs:
- `TALLY_HEALTHCARE_PLACEHOLDER` → your healthcare form ID
- `TALLY_LEGAL_PLACEHOLDER` → your legal form ID
- `TALLY_DEFENSE_PLACEHOLDER` → your defense form ID

### 2. Stripe Payment Links
Search for `buy.stripe.com` and replace placeholder URLs:
- `https://buy.stripe.com/ANALYST_MONTHLY` → your actual link
- `https://buy.stripe.com/TEAM_MONTHLY` → your actual link

### 3. Download/GitHub Links
Search for `#DOWNLOAD_URL_PLACEHOLDER` and `#GITHUB_REPO_PLACEHOLDER`:
- Replace with actual GitHub release URLs when binary is ready

### 4. Demo Video
In `finance.html`, replace the `[Video coming soon]` placeholder with embedded video.

## Custom Domain Setup

1. In Netlify: Site settings → Domain management → Add custom domain
2. In your registrar: Add DNS records as shown by Netlify
3. Wait for DNS propagation (5 min to 48 hours)
4. Netlify auto-provisions SSL

## Local Preview

```bash
# Simple Python server
python3 -m http.server 8000

# Then open http://localhost:8000
```

## Stack

- **HTML** - Static pages
- **Tailwind CSS** - Via CDN (no build step)
- **Netlify** - Hosting (free tier)
- **Tally** - Forms (free tier)
- **Stripe** - Payments (Payment Links)
- **Plausible** - Analytics ($9/mo)

## Cost

| Service | Cost |
|---------|------|
| Netlify | $0 |
| Tally | $0 |
| Plausible | $9/mo |
| Domain | ~$10/yr |
| **Total** | **~$10/yr + $9/mo** |

See `DEPLOYMENT.md` for full setup instructions.
