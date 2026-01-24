# Casparian Flow Website - Deployment Plan

> **Stack:** HTML + Tailwind (CDN) + Netlify
> **Cost:** $0/month (except Plausible $9/mo)

---

## EXECUTION PLAN: Website Ready State

Goal: Get everything set up so when binary + video are ready, just plug in URLs and go live.

---

### PHASE A: Accounts Setup (Claude can verify, you execute)
```
[ ] A1. Purchase domain (casparianflow.com) - Cloudflare ~$10/yr
[ ] A2. Create Stripe account - stripe.com
[ ] A3. Create Tally account - tally.so (free)
[ ] A4. Create Plausible account - plausible.io ($9/mo) - add site "casparianflow.com"
[ ] A5. Create Netlify account - netlify.com (free)
[ ] A6. Create GitHub repo for website: github.com/[you]/casparian-website
```

---

### PHASE B: Stripe Products & Payment Links (Claude can guide)
```
[ ] B1. Stripe → Products → Create "Casparian Flow - Analyst"
[ ] B2.   → Add price: $300/month (recurring)
[ ] B3.   → Add price: $3,000/year (recurring)
[ ] B4. Stripe → Products → Create "Casparian Flow - Team"
[ ] B5.   → Add price: $2,000/month (recurring)
[ ] B6.   → Add price: $20,000/year (recurring)
[ ] B7. Stripe → Payment Links → Create for Analyst Monthly → Copy URL
[ ] B8. Stripe → Payment Links → Create for Team Monthly → Copy URL
```

**Output needed:** 2 Payment Link URLs

---

### PHASE C: Tally Waitlist Forms (Claude can guide)
```
[ ] C1. Tally → Create "Healthcare Waitlist"
        - Email (required)
        - Organization (required)
        - "What's your biggest HL7 pain point?" (optional, long text)
        - Thank you message: "Thanks! We'll notify you when Healthcare is live."

[ ] C2. Tally → Create "Legal Waitlist"
        - Email (required)
        - Firm/Organization (required)
        - "Average PST volume per matter?" (optional)
        - Thank you message: "Thanks! We'll notify you when Legal is live."

[ ] C3. Tally → Create "Defense Waitlist"
        - Email (required)
        - Organization (required)
        - "Primary format?" (dropdown: CoT, PCAP, NITF, Other)
        - Thank you message: "Thanks! We'll reach out about pilot opportunities."

[ ] C4. Publish all 3 forms → Copy embed URLs
```

**Output needed:** 3 Tally embed URLs

---

### PHASE D: Update Website HTML (Claude will do this)
```
[ ] D1. finance.html: Insert Stripe Analyst link (line 298)
[ ] D2. finance.html: Insert Stripe Team link (line 320)
[ ] D3. healthcare.html: Insert Tally form ID (line 82)
[ ] D4. legal.html: Insert Tally form ID (line 82)
[ ] D5. defense.html: Insert Tally form ID (line 82)
[ ] D6. All HTML files: Update Plausible domain if different from casparianflow.com
[ ] D7. Fix favicon: Update HTML to reference favicon.svg (not .ico)
```

---

### PHASE E: Deploy to Netlify (Claude can guide)
```
[ ] E1. git init && git add . && git commit -m "Initial website"
[ ] E2. Push to GitHub repo
[ ] E3. Netlify → Add new site → Import from GitHub
[ ] E4. Build settings: publish directory = "." (just a dot)
[ ] E5. Deploy (get temporary netlify.app URL)
[ ] E6. Add custom domain: casparianflow.com
[ ] E7. Update DNS at Cloudflare (CNAME or Netlify DNS)
[ ] E8. Verify HTTPS certificate provisioned
```

---

### PHASE F: Configure Analytics (Claude can guide)
```
[ ] F1. Plausible → Goals → Add "Download" (custom event)
[ ] F2. Plausible → Goals → Add "Start Trial" (custom event)
[ ] F3. Plausible → Goals → Add "Waitlist Healthcare" (custom event)
[ ] F4. Plausible → Goals → Add "Waitlist Legal" (custom event)
[ ] F5. Plausible → Goals → Add "Waitlist Defense" (custom event)
```

---

### PHASE G: Verify Ready State
```
[ ] G1. https://casparianflow.com loads
[ ] G2. https://casparianflow.com/finance loads
[ ] G3. "Start Trial" buttons open Stripe checkout (test mode)
[ ] G4. Healthcare/Legal/Defense waitlist forms submit successfully
[ ] G5. Plausible shows pageviews
[ ] G6. Download buttons show placeholder (will update when binary ready)
[ ] G7. Demo section shows placeholder (will update when video ready)
```

---

### PHASE H: License Automation Setup
```
[ ] H1. Choose automation method:
        Option A: Stripe → Zapier → Email (easiest, $20/mo)
        Option B: Stripe webhook → Netlify Function → SendGrid (free, more work)

[ ] H2. Set up Stripe webhook for checkout.session.completed
[ ] H3. Create email template with license key placeholder
[ ] H4. Test with Stripe test mode purchase
[ ] H5. Verify email arrives with license key
```

---

## PLUG-IN LATER (When Binary + Video Ready)

```
[ ] P1. Build CLI binaries (macOS ARM, macOS x64, Linux, Windows)
[ ] P2. Create GitHub Release with binaries
[ ] P3. Update finance.html download URLs (lines 80, 282, 403)
[ ] P4. Record demo video (60-90 sec)
[ ] P5. Upload to YouTube/Vimeo
[ ] P6. Embed video in finance.html (line 259)
[ ] P7. git commit && git push (Netlify auto-deploys)
[ ] P8. Switch Stripe to live mode
[ ] P9. Start outreach (see MARKETING_PLAN.md)
```

---

## WHAT CLAUDE CAN DO NOW

Give me the following and I'll update all the HTML files:

1. **Stripe Payment Link URLs** (2 URLs)
   - Analyst Monthly: `https://buy.stripe.com/...`
   - Team Monthly: `https://buy.stripe.com/...`

2. **Tally Form IDs** (3 IDs from embed URLs)
   - Healthcare: `https://tally.so/embed/XXXXXX`
   - Legal: `https://tally.so/embed/XXXXXX`
   - Defense: `https://tally.so/embed/XXXXXX`

3. **Domain confirmation** (if different from casparianflow.com)

Then I'll update all HTML files, fix the favicon reference, and the site will be ready to deploy.

---

## CRITICAL BLOCKERS (Do These First)

These items **block all marketing outreach**. Complete before any launch activities.

### 1. Demo Video (BLOCKER)
```
Status: [ ] NOT DONE
Location: finance.html line 259 shows "[Video coming soon]"

Action:
- Record 60-90 second screen recording
- Show: scan FIX logs → query order → see lifecycle
- Tools: OBS (free) or Loom
- Upload to: YouTube (unlisted) or Vimeo
- Update: finance.html demo section with embed
```

### 2. GitHub Releases (BLOCKER)
```
Status: [ ] NOT DONE
Links in finance.html point to non-existent releases

Action:
- Build CLI binaries for: macOS ARM64, macOS x64, Linux x64, Windows x64
- Create GitHub repo: github.com/casparian/casparian-flow
- Create release v0.1.0 with binaries attached
- Update download links in finance.html (lines 80, 281, 403)
```

### 3. Stripe Payment Links (BLOCKER)
```
Status: [ ] NOT DONE
Placeholders: YOUR_STRIPE_LINK_ANALYST_MONTHLY, YOUR_STRIPE_LINK_TEAM_MONTHLY

Action:
- Create 4 Stripe Payment Links:
  1. Analyst Monthly: $300/mo recurring
  2. Analyst Annual: $3,000/yr recurring
  3. Team Monthly: $2,000/mo recurring
  4. Team Annual: $20,000/yr recurring
- Update finance.html lines 298, 320
```

### 4. Tally Waitlist Forms (BLOCKER for other verticals)
```
Status: [ ] NOT DONE
Placeholders: YOUR_TALLY_FORM_ID_HEALTHCARE, YOUR_TALLY_FORM_ID_DEFENSE, YOUR_TALLY_FORM_ID_LEGAL

Action:
- Create 3 Tally forms with fields:
  - Email (required)
  - Organization (required)
  - Pain point question (optional)
- Update healthcare.html, defense.html, legal.html with form IDs
```

### 5. Testimonial (IMPORTANT but not blocking)
```
Status: [ ] PLACEHOLDER
Location: finance.html line 357 shows "[Testimonial placeholder]"

Action:
- Replace with real customer quote after first paying customer
- Or remove section until testimonial available
```

### 6. Favicon Mismatch (MINOR)
```
Status: [ ] MISMATCH
HTML references: /assets/favicon.ico
Actually exists: /assets/favicon.svg

Action:
- Either convert favicon.svg to favicon.ico
- Or update HTML files to reference favicon.svg
```

---

## Pre-Flight Checklist

Before starting, you need:

```
[ ] Domain purchased (casparianflow.com or similar)
[ ] Stripe account created
[ ] Tally account created (free)
[ ] Plausible account created ($9/mo)
[ ] GitHub account (for version control)
[ ] Netlify account (free)
```

---

## Phase 1: Domain & Accounts (30 min)

### 1.1 Purchase Domain

**Recommended registrars:**
- Cloudflare Registrar (cheapest, $9-10/yr for .dev)
- Namecheap
- Porkbun

**Domain options:**
| Domain | Availability | Notes |
|--------|--------------|-------|
| casparianflow.com | Check | Professional, .dev forces HTTPS |
| casparianflow.com | Check | Longer but clear |
| casparian.io | Check | Tech-friendly TLD |

**Action:**
```
1. Go to Cloudflare Registrar
2. Search for domain
3. Purchase (don't set up DNS yet - Netlify will handle it)
```

### 1.2 Create Accounts

**Stripe** (payments)
```
1. Go to stripe.com → Create account
2. Complete business verification (can use personal for now)
3. Note: You'll create Payment Links later
```

**Tally** (forms)
```
1. Go to tally.so → Sign up free
2. No setup needed yet - we'll create forms later
```

**Plausible** (analytics)
```
1. Go to plausible.io → Start trial
2. Add site: casparianflow.com (or your domain)
3. Copy the script tag for later:
   <script defer data-domain="casparianflow.com" src="https://plausible.io/js/script.js"></script>
```

**Netlify** (hosting)
```
1. Go to netlify.com → Sign up with GitHub
2. No setup needed yet - we'll deploy after building
```

---

## Phase 2: Build Website (1-2 hours)

### 2.1 File Structure

Your website folder should contain:
```
website/
├── index.html          # Homepage (vertical selector)
├── finance.html        # Full finance page with pricing
├── healthcare.html     # Coming soon + waitlist
├── defense.html        # Coming soon + waitlist
├── legal.html          # Coming soon + waitlist
├── _redirects          # Netlify clean URLs
├── netlify.toml        # Netlify configuration
└── assets/
    └── favicon.ico     # Browser tab icon
```

### 2.2 Create Tally Forms

Before finalizing HTML, create 3 Tally forms:

**Healthcare Waitlist Form**
```
1. Tally → Create form
2. Add fields:
   - Email (required)
   - Organization (required)
   - "What's your biggest HL7 pain point?" (long text, optional)
3. Settings → Thank you page → Custom message:
   "Thanks! We'll notify you when Healthcare is live."
4. Publish → Copy embed URL
```

**Defense Waitlist Form**
```
1. Tally → Create form
2. Add fields:
   - Email (required)
   - Organization (required)
   - "Primary format" (dropdown: CoT, PCAP, NITF, Other)
3. Settings → Thank you page → Custom message:
   "Thanks! We'll reach out about pilot opportunities."
4. Publish → Copy embed URL
```

**Legal Waitlist Form**
```
1. Tally → Create form
2. Add fields:
   - Email (required)
   - Firm / Organization (required)
   - "Average PST volume per matter?" (short text, optional)
3. Settings → Thank you page → Custom message:
   "Thanks! We'll notify you when Legal is live."
4. Publish → Copy embed URL
```

**Update HTML files** with your Tally form URLs (search for `YOUR_TALLY_FORM_ID`).

### 2.3 Create Stripe Payment Links

In Stripe Dashboard:

**Free Tier** (not needed - direct download)

**Analyst Tier - $300/month**
```
1. Stripe → Payment Links → Create
2. Product name: "Casparian Flow - Analyst"
3. Price: $300/month (recurring)
4. After payment: Redirect to https://casparianflow.com/success
5. Copy link
```

**Analyst Tier - $3,000/year**
```
1. Same as above, but $3,000/year
2. Copy link
```

**Team Tier - $2,000/month**
```
1. Product: "Casparian Flow - Team"
2. Price: $2,000/month
3. Copy link
```

**Team Tier - $20,000/year**
```
1. Same, $20,000/year
2. Copy link
```

**Trading Desk** - Use "Contact Us" (manual sales)

**Update HTML files** with your Stripe Payment Links (search for `YOUR_STRIPE_LINK`).

### 2.4 Add Plausible Analytics

In each HTML file, add before `</head>`:
```html
<script defer data-domain="casparianflow.com" src="https://plausible.io/js/script.js"></script>
```

### 2.5 Configure Plausible Goals

In Plausible dashboard → Goals:
```
1. Add goal: "Download" (Pageview: /download)
2. Add goal: "Start Trial" (Custom event: start-trial)
3. Add goal: "Waitlist Healthcare" (Custom event: waitlist-healthcare)
4. Add goal: "Waitlist Defense" (Custom event: waitlist-defense)
5. Add goal: "Waitlist Legal" (Custom event: waitlist-legal)
```

---

## Phase 3: Deploy to Netlify (15 min)

### 3.1 Push to GitHub (Recommended)

```bash
# From website directory
cd /path/to/website

# Initialize git
git init
git add .
git commit -m "Initial website"

# Create GitHub repo and push
gh repo create casparian-website --private --source=. --push
```

### 3.2 Connect Netlify to GitHub

```
1. Netlify → Add new site → Import existing project
2. Connect to GitHub
3. Select your repository
4. Build settings:
   - Build command: (leave empty)
   - Publish directory: . (just a dot)
5. Deploy site
```

### 3.3 Configure Custom Domain

```
1. Netlify → Site settings → Domain management
2. Add custom domain → Enter your domain (e.g., casparianflow.com)
3. Netlify will show DNS records to add

4. In your domain registrar (Cloudflare/Namecheap):
   - Add CNAME record: @ → your-site-name.netlify.app
   - Or use Netlify DNS (recommended - they handle everything)

5. Wait for DNS propagation (5 min to 48 hours, usually ~30 min)

6. Netlify → Domain settings → HTTPS → Verify
   - Netlify auto-provisions Let's Encrypt SSL
```

### 3.4 Verify Deployment

```
[ ] https://casparianflow.com loads correctly
[ ] https://casparianflow.com/finance loads correctly
[ ] https://casparianflow.com/healthcare loads correctly
[ ] https://casparianflow.com/defense loads correctly
[ ] https://casparianflow.com/legal loads correctly
[ ] All forms submit successfully
[ ] Stripe links work (test with Stripe test mode first)
[ ] Plausible showing pageviews
```

---

## Phase 4: Post-Deploy Setup (30 min)

### 4.1 Set Up Email

**Option A: Cloudflare Email Routing (Free)**
```
1. Cloudflare → Email → Email Routing
2. Create route: support@casparianflow.com → your-personal@email.com
3. Create route: defense@casparianflow.com → your-personal@email.com
```

**Option B: Fastmail/Google Workspace ($5-6/mo)**
```
1. Sign up for Fastmail or Google Workspace
2. Add custom domain
3. Configure DNS records
```

### 4.2 Set Up Stripe Webhooks (For License Keys)

For now, manual process:
```
1. Customer pays via Stripe
2. You receive Stripe notification email
3. You manually generate license key
4. You email license key to customer
```

Later, automate with:
- Stripe webhook → Netlify Function → Generate key → Email via SendGrid

### 4.3 Create GitHub Releases for Downloads

```bash
# Build your CLI
cargo build --release

# Create release on GitHub
gh release create v0.1.0 \
  ./target/release/casparian-macos-arm64 \
  ./target/release/casparian-macos-x64 \
  ./target/release/casparian-linux-x64 \
  ./target/release/casparian-windows-x64.exe \
  --title "v0.1.0" \
  --notes "Initial release"
```

Update download links in finance.html to point to GitHub Releases.

---

## Phase 5: Testing Checklist

### Functional Tests
```
[ ] Homepage loads in < 2 seconds
[ ] All navigation links work
[ ] Finance page pricing displays correctly
[ ] Download button links to GitHub Releases
[ ] "Start Trial" opens Stripe checkout
[ ] Healthcare waitlist form submits
[ ] Defense waitlist form submits
[ ] Legal waitlist form submits
[ ] Back to homepage links work on all pages
[ ] Mobile responsive (test on phone)
```

### SEO Tests
```
[ ] Each page has unique <title>
[ ] Each page has unique meta description
[ ] Open Graph tags present (for social sharing)
[ ] favicon.ico loads
```

### Analytics Tests
```
[ ] Plausible script loads (check Network tab)
[ ] Pageviews recording
[ ] Goals tracking (test a form submit)
```

### Payment Tests
```
[ ] Stripe test mode works
[ ] Stripe live mode works (do a real $1 test charge, refund)
```

---

## Maintenance Runbook

### To Update Content
```bash
# Edit HTML files
# Commit and push
git add .
git commit -m "Update pricing"
git push

# Netlify auto-deploys on push
```

### To Check Analytics
```
1. Go to plausible.io/casparianflow.com
2. Check daily/weekly views
3. Check goal conversions
```

### To Check Waitlist Signups
```
1. Go to tally.so
2. Click each form
3. View responses
4. Export to CSV if needed
```

### To Process a Sale
```
1. Receive Stripe notification
2. Generate license key:
   CSP-{tier}-{random_hex_16}
   Example: CSP-TEAM-a1b2c3d4e5f67890
3. Email customer with:
   - License key
   - Download link
   - Quick start instructions
```

---

## Cost Summary

| Service | Cost | Notes |
|---------|------|-------|
| Domain | ~$10/year | .dev from Cloudflare |
| Netlify | $0 | Free tier (100GB bandwidth) |
| Tally | $0 | Free tier (unlimited forms) |
| Stripe | 2.9% + $0.30 | Per transaction |
| Plausible | $9/month | Privacy-first analytics |
| **Total** | **~$10/year + $9/mo** | Before payment fees |

---

## Emergency Procedures

### Site Down
```
1. Check Netlify status: netlifystatus.com
2. Check your deployment: Netlify → Deploys
3. Rollback if needed: Netlify → Deploys → Select previous → Publish
```

### Form Not Working
```
1. Check Tally status
2. Verify embed URL is correct
3. Test in incognito mode (ad blockers can interfere)
```

### Stripe Issues
```
1. Check Stripe Dashboard for errors
2. Verify Payment Link is active
3. Test in Stripe test mode
```

---

## Next Steps After Launch

### Week 1
- [ ] Announce to pilot prospects via email
- [ ] Post on LinkedIn
- [ ] Monitor analytics daily

### Week 2-4
- [ ] Collect feedback from first users
- [ ] Add testimonial when available
- [ ] Iterate on copy based on questions received

### Month 2+
- [ ] Consider Google Ads for "FIX log analysis"
- [ ] Build out next vertical based on waitlist demand
- [ ] Automate license key delivery

---

## File Reference

| File | Purpose |
|------|---------|
| `index.html` | Homepage with vertical selector |
| `finance.html` | Full Trade Break Workbench page |
| `healthcare.html` | HL7 waitlist page |
| `defense.html` | Defense/Intel waitlist page |
| `legal.html` | eDiscovery waitlist page |
| `_redirects` | Netlify URL rewrites |
| `netlify.toml` | Netlify configuration |

All files are ready to deploy. Just update:
1. Tally form IDs
2. Stripe Payment Link URLs
3. Plausible domain (if different)
4. GitHub Release download URLs

---

## Related Documents

| Document | Purpose |
|----------|---------|
| `MARKETING_PLAN.md` | Launch strategy, outreach templates, channel tactics |
| `README.md` | Quick deployment guide |

See `MARKETING_PLAN.md` for:
- LinkedIn/Reddit/HN outreach strategies
- Cold email templates
- Vertical launch criteria (when to go live with Healthcare, Legal, Defense)
- Pricing rationale
- Competitive positioning
