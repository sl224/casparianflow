# Non-Dilutive Funding Strategy

**Status:** Vetted & Verified
**Parent:** STRATEGY.md Section 6 (Revenue Model)
**Purpose:** Non-dilutive funding sources compatible with commercial product strategy
**Version:** 1.0
**Date:** January 8, 2026

---

## 1. Executive Summary

Casparian Flow can pursue **non-dilutive funding** (grants that don't take equity) to fund core technology development while maintaining full commercial freedom.

**The Strategy:**
- **Grants fund the infrastructure** (open-source Rust crates, parsers)
- **Commercial sales fund the business** (Pro tier, Enterprise features)

This is the **Open Core** model used by GitLab, Redis, and others.

**Key Constraint:** No military weapons funding. All sources below are civilian/humanitarian focused.

---

## 2. Funding Source Analysis (Verified)

### 2.1 Tier 1: Best Fit (Apply Now)

#### Sovereign Tech Fund (STF) - HIGHEST PRIORITY

| Attribute | Detail |
|-----------|--------|
| **Organization** | German Federal Ministry for Economic Affairs |
| **Funding** | €50,000 - €1,000,000+ |
| **Timeline** | ~6 months from application to decision |
| **Deadline** | Rolling (always open) |
| **URL** | [sovereign.tech](https://www.sovereign.tech/) |

**What They Fund:**
> "Open digital base technologies such as software libraries, protocols, development tools, and related infrastructure."

**Recent Rust Grants:**
- uutils (Rust Coreutils): €99,060
- Rustls: €1,436,729 (via Prossimo)
- Domain (DNS library): €993,600

**Why Casparian Fits:**
- `casparian_worker` - Rust execution engine for data processing
- `casparian_protocol` - Binary protocol for worker communication
- `casparian_schema` - Schema contract system

**The Pitch:**
> "The world's data infrastructure relies on brittle Python scripts. Casparian rebuilds the 'Bronze Layer' (raw file parsing) in memory-safe Rust to prevent supply chain attacks and logic errors in critical systems."

**Strings Attached:**
- Funded code must remain open source (Apache/MIT)
- No restrictions on commercial use of the product
- **Freedom Score: 5/5** - You can sell to anyone

**Application Focus:**
- Security improvements to Rust crates
- Memory safety benefits (cite NSA/CISA guidance)
- Maintenance of open-source parser infrastructure

---

#### NLNet Foundation - NGI Zero Commons Fund

| Attribute | Detail |
|-----------|--------|
| **Organization** | Dutch foundation + EU Horizon Europe |
| **Funding** | €5,000 - €50,000 (scalable) |
| **Timeline** | ~3-4 months |
| **Next Deadline** | February 1, 2026 (12:00 CET) |
| **URL** | [nlnet.nl/funding.html](https://nlnet.nl/funding.html) |

**What They Fund:**
> "Projects that contribute to an open information society... solutions that bring the next generation of the internet closer."

**Recent Grant Stats:**
- 272+ projects funded in NGI Zero Commons Fund
- €21.6 million available through 2027
- 45 projects selected in most recent round

**Why Casparian Fits:**
- Local-first data processing (user control)
- Schema contracts (data governance)
- MCP integration (open AI protocols)

**The Pitch:**
> "Casparian enables citizens and organizations to structure their own data locally, without uploading to centralized cloud platforms. This restores data sovereignty to individuals."

**Strings Attached:**
- Funded work must be open source
- Results should benefit the commons
- **Freedom Score: 5/5** - No commercial restrictions

**Application Focus:**
- Data sovereignty angle
- Local-first architecture
- Open standards (MCP, Parquet)

---

### 2.2 Tier 2: Good Fit (Monitor/Apply)

#### Open Technology Fund (OTF) - Internet Freedom Fund

| Attribute | Detail |
|-----------|--------|
| **Organization** | US Agency for Global Media (USAGM) |
| **Funding** | $50,000 - $900,000 |
| **Timeline** | Concept note first, then full proposal |
| **Deadline** | Rolling |
| **URL** | [opentech.fund/funds/internet-freedom-fund](https://www.opentech.fund/funds/internet-freedom-fund/) |

**What They Fund:**
> "Innovative global internet freedom projects that counter censorship or surveillance."

**Why Casparian Fits:**
- Air-gapped operation (no network required)
- Local data processing (no cloud upload)
- Journalist/activist use case (Panama Papers scenario)

**The "Journalism Shield" Pitch:**
> "Investigative journalists receive massive data leaks but cannot use cloud tools due to source protection risks. Casparian allows them to structure terabytes of chaotic data locally on an air-gapped laptop."

**RISK WARNING:**
> In March 2025, OTF's funding was temporarily terminated by executive order. It was reinstated after lawsuit, but political uncertainty exists. Monitor status before investing significant application effort.

**Strings Attached:**
- Funded code must be open source
- Focus must be on internet freedom use case
- **Freedom Score: 5/5** - No restrictions on selling to other markets

**Application Focus:**
- Privacy-preserving architecture
- Offline/air-gapped capability
- PST/email archive parsing (journalist use case)

---

#### NIH SBIR - Health Data Interoperability

| Attribute | Detail |
|-----------|--------|
| **Organization** | National Institutes of Health |
| **Funding** | Up to $314,363 (Phase I) |
| **Timeline** | 6-12 months for Phase I |
| **Deadlines** | January 5, April 5, September 5 (annual) |
| **URL** | [seed.nih.gov](https://seed.nih.gov/) |

**What They Fund:**
> "Databases, standards for enhanced interoperability... techniques for the integration of heterogeneous data."

**Why Casparian Fits:**
- HL7 parser spec (`healthcare_hl7.md`)
- Interoperability without migration (archive analytics)
- FHIR/TEFCA alignment potential

**The Pitch:**
> "Healthcare organizations have decades of HL7 v2.x archives that cannot be queried. Casparian provides SQL access to historical health data without replacing existing integration engines."

**Strings Attached:**
- US government gets royalty-free license for internal use
- Must be majority US-owned company
- Manufacturing (if any) must be in US
- **Freedom Score: 4/5** - Minor restrictions, full commercial rights

**Application Focus:**
- Healthcare data interoperability
- Local processing (HIPAA compliance angle)
- Integration with existing HL7 infrastructure

---

#### Mozilla Foundation Programs

| Attribute | Detail |
|-----------|--------|
| **Organization** | Mozilla Foundation |
| **Funding** | Varies by program ($50k-$100k typical) |
| **Timeline** | Cohort-based |
| **Next Call** | Incubator opens 2026 |
| **URL** | [mozillafoundation.org/what-we-fund](https://www.mozillafoundation.org/en/what-we-fund/) |

**What They Fund:**
> "Privacy-preserving technologies... critical open source infrastructure... new models of AI development that prioritize accountability."

**Why Casparian Fits:**
- Local-first (no cloud data upload)
- MCP integration (open AI protocol)
- Schema contracts (AI accountability)

**The "Local AI" Pitch:**
> "The current AI data stack forces users to upload sensitive files to centralized clouds. Casparian proves that Agentic Data Engineering can happen entirely on the user's device, preserving data sovereignty."

**Note:** Mozilla programs are competitive and cohort-based. Less predictable than STF/NLNet.

**Strings Attached:**
- Must align with Mozilla's public benefit mission
- Open source preferred
- **Freedom Score: 4/5** - Mission alignment required

---

### 2.3 Tier 3: Participate (Not Grants)

#### Agentic AI Foundation (AAIF)

| Attribute | Detail |
|-----------|--------|
| **Organization** | Linux Foundation |
| **Type** | Standards body, NOT grant program |
| **Members** | Anthropic, OpenAI, Block, Google, Microsoft, AWS |
| **URL** | [linuxfoundation.org](https://www.linuxfoundation.org/) |

**CORRECTION:** The original analysis incorrectly described AAIF as a grant source. It is a **standards governance body** for agentic AI protocols (MCP, Goose, AGENTS.md).

**Why Casparian Should Engage:**
- `casparian_mcp` implements MCP protocol
- Could contribute to MCP specification
- Visibility among major AI players

**How to Engage:**
- Contribute to MCP working groups
- Publish MCP server implementations
- NOT a funding source

---

## 3. The Open Core Strategy

### 3.1 What You Open Source (Grant-Funded)

| Component | Repository | Grant Fit |
|-----------|------------|-----------|
| `casparian_worker` | Rust execution engine | STF, NLNet |
| `casparian_protocol` | Binary protocol | STF |
| `casparian_schema` | Schema contracts | NLNet |
| `hl7_parser.py` | Healthcare parser | NIH SBIR |
| `pst_parser.py` | Email archive parser | OTF |
| `fix_parser.py` | Financial parser | (Commercial focus) |
| `cot_parser.py` | Tactical data parser | STF, NLNet |

### 3.2 What You Sell (Commercial)

| Feature | Tier | Price |
|---------|------|-------|
| Pre-compiled binaries | Pro | $29/month |
| Auto-updates | Pro | Included |
| Priority support | Pro | Included |
| TUI Dashboard | Pro | Included |
| Real-time connectors (MLLP, etc.) | Enterprise | Custom |
| SSO/SAML integration | Enterprise | Custom |
| Audit logs (HIPAA, SOX) | Enterprise | Custom |
| On-prem deployment support | Enterprise | Custom |

### 3.3 Why This Works

**For Grant Committees:**
> "How will you survive after the grant?"
> **Answer:** "We sell a commercial version to Financial Services. The grant funds infrastructure; revenue funds the business."

**For Commercial Customers:**
> "Why should we trust you?"
> **Answer:** "Our core is open source, audited, and funded by STF/NIH. You can inspect every line of code."

---

## 4. Application Prioritization

### Immediate (January 2026)

| Action | Target | Effort |
|--------|--------|--------|
| Submit concept note | NLNet (Feb 1 deadline) | Medium |
| Submit application | Sovereign Tech Fund | Medium |
| Monitor OTF status | OTF (rolling) | Low |

### Q1 2026

| Action | Target | Effort |
|--------|--------|--------|
| Prepare NIH SBIR | April 5 deadline | High |
| Watch Mozilla calls | Incubator opens 2026 | Low |
| Engage AAIF | MCP working groups | Low |

### Narrative Mapping

| Funder | Narrative | Lead Parser |
|--------|-----------|-------------|
| STF | "Rust Infrastructure" | `casparian_worker`, `casparian_protocol` |
| NLNet | "Data Sovereignty" | Schema contracts, MCP integration |
| OTF | "Journalism Shield" | `pst_parser.py`, air-gap mode |
| NIH | "Health Interoperability" | `hl7_parser.py` |
| Mozilla | "Local AI" | MCP integration, BYOK |

---

## 5. Funding Sources NOT Recommended

### 5.1 Excluded by User Preference

| Source | Reason |
|--------|--------|
| **AFWERX** | Military (Air Force) - user prefers no military weapons funding |
| **DARPA** | Defense research - same concern |
| **DoD SBIR** | Military - documented separately for reference only |

### 5.2 Excluded by Poor Fit

| Source | Reason |
|--------|--------|
| **CISA Grants** | Go to governments to BUY tools, not vendors to BUILD them |
| **University Grants** | Often require university IP ownership |
| **Venture Capital** | Dilutive; takes equity; changes company direction |

---

## 6. Budget Projections

### Realistic Grant Revenue (12-month)

| Source | Amount | Probability | Expected |
|--------|--------|-------------|----------|
| Sovereign Tech Fund | €150,000 | 40% | €60,000 |
| NLNet | €40,000 | 50% | €20,000 |
| NIH SBIR Phase I | $314,000 | 20% | $62,800 |
| OTF | $100,000 | 25% | $25,000 |
| **Total Expected** | | | **~$165,000** |

### Grant + Commercial Combined

| Revenue Stream | Year 1 | Year 2 |
|----------------|--------|--------|
| Grant funding | $165K | $200K |
| Commercial (Finance) | $50K | $200K |
| **Total** | **$215K** | **$400K** |

---

## 7. Checklist: Before Applying

### Administrative

- [ ] Register company/entity (if not done)
- [ ] Set up bank account that can receive EUR (for STF/NLNet)
- [ ] Prepare 2-page capability statement
- [ ] Identify open-source license strategy (Apache 2.0 recommended)
- [ ] Document team/founder background

### Technical

- [ ] Ensure core crates are already open source (or ready to open)
- [ ] Prepare demo environment
- [ ] Document security posture (for STF)
- [ ] Prepare architecture diagrams

### Narrative

- [ ] Draft "Rust Infrastructure" pitch (STF)
- [ ] Draft "Data Sovereignty" pitch (NLNet)
- [ ] Draft "Journalism Shield" pitch (OTF)
- [ ] Draft "Health Interoperability" pitch (NIH)

---

## 8. References

### Verified Sources

- [Sovereign Tech Fund](https://www.sovereign.tech/)
- [NLNet Foundation](https://nlnet.nl/funding.html)
- [Open Technology Fund](https://www.opentech.fund/funds/internet-freedom-fund/)
- [NIH SEED (SBIR/STTR)](https://seed.nih.gov/)
- [Mozilla Foundation Programs](https://www.mozillafoundation.org/en/what-we-fund/)
- [Agentic AI Foundation (Linux Foundation)](https://www.linuxfoundation.org/press/linux-foundation-announces-the-formation-of-the-agentic-ai-foundation)

### Background

- [STF Rust Coreutils Grant](https://www.phoronix.com/news/Sovereign-Tech-Fund-Rust-uutils)
- [OTF Legal Challenge (March 2025)](https://en.wikipedia.org/wiki/Open_Technology_Fund)
- [NLNet NGI Zero Commons Fund](https://nlnet.nl/commonsfund/)

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 1.0 | Initial document; vetted external analysis; verified all funding sources; corrected AAIF description; excluded military options per user preference |

