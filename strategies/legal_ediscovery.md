# Legal Tech & eDiscovery Market Strategy

**Status:** Draft (Phase 2/3 Target)
**Parent:** STRATEGY.md Section 2 (Target Market → Legal Tech)
**Related Specs:** (future) specs/pst_parser.md, specs/loadfile_parser.md
**Version:** 0.1
**Date:** January 8, 2026
**Priority:** Phase 2/3 - After Finance vertical traction

---

## 1. Executive Summary

The eDiscovery market processes billions of documents annually for litigation, investigations, and compliance. Data arrives in arcane formats (PST email archives, Concordance/Relativity load files, chat exports) that require expensive specialized tools to process.

**The Opportunity:**
Casparian Flow fills a gap: **Pre-review data structuring without $50K+ eDiscovery platform licenses.**

A litigation support specialist can process a PST archive, extract metadata, and create a searchable index without RelativityOne or Concordance—enabling small firms and solo practitioners to handle eDiscovery in-house.

**Key Insight:**
The "small law" market (80,000+ firms with <10 attorneys) cannot afford enterprise eDiscovery platforms but must handle discovery. They currently outsource or use manual methods.

**Strategic Positioning:**
"Pre-processing tier" - We structure the raw data before it goes into review platforms. We are **upstream** of Relativity, not competing with it.

---

## 2. Market Context

### 2.1 The eDiscovery Cost Problem

eDiscovery follows an inverted pyramid:

```
           ┌─────────┐
           │ Review  │ ← $$$$ (Attorney review: $50-500/hour)
          ┌┴─────────┴┐
          │ Processing│ ← $$$ (Platform fees: $0.10-0.50/GB)
         ┌┴───────────┴┐
         │  Collection │ ← $$ (Forensic collection)
        ┌┴─────────────┴┐
        │ Identification │ ← $ (Find where data lives)
        └───────────────┘
```

**The pain:** Even small matters can cost $50K+ just for processing fees. Small firms either:
1. Outsource to vendors ($$$)
2. Use manual methods (slow, error-prone)
3. Under-collect and risk sanctions

### 2.2 Market Size

| Segment | Size | Growth | Casparian Relevance |
|---------|------|--------|---------------------|
| Global eDiscovery | $15B+ (2024) | 10%+ CAGR | Core target |
| Legal tech | $30B+ | 8% CAGR | Adjacent |
| Small law (<10 attorneys) | 80,000+ firms | Stable | Underserved segment |
| Litigation support services | $5B+ | 6% CAGR | Channel opportunity |

### 2.3 The Format Problem

| Format | Origin | Pain |
|--------|--------|------|
| **PST/OST** | Outlook archives | Giant, proprietary, nested folders |
| **MBOX** | Gmail/Thunderbird | Concatenated text, no structure |
| **Load Files (.dat, .opt)** | Concordance/Relativity exports | Tab-delimited with image pointers |
| **Slack JSON** | Workspace exports | Nested channels, threads, attachments |
| **Teams Export** | Microsoft 365 | Complex JSON with threading |
| **WhatsApp** | Chat exports | HTML or text, no standard |

**Nobody does this well:**
- Relativity is $150K+/year
- Concordance is legacy (Thomson Reuters)
- Nuix starts at $50K+
- Open source tools are fragmented

### 2.4 Regulatory Pressure

| Regulation | Impact |
|------------|--------|
| FRCP Rule 26/34 | Requires production in "reasonably usable" form |
| Model Rules 1.1 | Competence includes technology |
| State bars | Increasing tech CLE requirements |
| GDPR/CCPA | Cross-border discovery complications |

**Trend:** Courts increasingly sanction parties for eDiscovery failures. Small firms need tools or face malpractice risk.

---

## 3. Where Legal Data Lives

### 3.1 The PST Graveyard

Every Windows-based organization has PST files scattered everywhere:

```
Legal data locations:
\\fileserver\departing_employees\
├── jsmith_2019/
│   ├── outlook_archive.pst      # 15GB email archive
│   └── desktop_backup.zip
├── mwilliams_2020/
│   └── archive_2015_2020.pst    # 8GB
└── ...

Local machines:
C:\Users\{user}\Documents\Outlook Files\
├── archive.pst
├── archive1.pst                 # Outlook creates numbered copies
└── old_emails.pst
```

**Characteristics:**
- Often contain years of email history
- Frequently the ONLY copy of key communications
- PST files can exceed 50GB (performance degrades)
- No searchability without specialized tools

### 3.2 Modern Collaboration Exports

Post-2020, collaboration platforms dominate:

```
Slack export structure:
slack_export_20260101/
├── channels/
│   ├── general/
│   │   └── 2026-01-01.json
│   ├── legal-team/
│   │   └── *.json
│   └── ...
├── users.json
└── channels.json

Teams export structure (compliance export):
teams_export/
├── Messages/
│   ├── team_channel_messages.json
│   └── 1on1_chat_messages.json
├── Files/
└── ...
```

**Challenge:** Threading, reactions, edits, deletions must be reconstructed.

### 3.3 Load File Ecosystem

Review platforms export/import via load files:

```
Production structure:
production_001/
├── DATA/
│   └── production_001.dat        # Tab-delimited metadata
├── IMAGES/
│   ├── 0001/
│   │   ├── DOC0001_0001.tif
│   │   └── DOC0001_0002.tif
│   └── ...
├── NATIVES/
│   └── *.pdf, *.docx, *.xlsx
└── TEXT/
    └── extracted_text/
```

**Load file format (.dat):**
```
þBEGDOCþþENDDOCþþBEGATTþþPARENTIDþþCUSTODIANþþDATE_SENTþ
þDOC0001þþDOC0001þþþþJohn SmithþþSmithþþ01/15/2024þ
```

**Note:** `þ` (thorn) is the standard field delimiter. Yes, really.

---

## 4. Target Personas

### 4.1 Primary: Litigation Support Specialist

| Attribute | Description |
|-----------|-------------|
| **Role** | Litigation Support Manager, eDiscovery Coordinator |
| **Technical skill** | SQL, some Python, comfortable with command line |
| **Pain** | Processing costs eat into matter budgets; platform licenses are expensive |
| **Goal** | Process collections in-house; reduce vendor dependency |
| **Buying power** | Operations budget; can approve tools under $10K |

**Current Workflow (painful):**
1. Receive 200GB PST collection
2. Call vendor for processing quote ($5-15K)
3. Wait 2-3 days for processing
4. Load into review platform
5. Repeat for every matter

**Casparian Workflow:**
1. `casparian scan /collections/smith_matter --tag pst_files`
2. `casparian process --tag pst_files`
3. Query: `SELECT * FROM pst_emails WHERE custodian = 'John Smith' AND date_sent > '2024-01-01'`
4. Export to load file for review platform (if needed)
5. 30 minutes, $0 vendor cost

### 4.2 Secondary: Small Firm Attorney

| Attribute | Description |
|-----------|-------------|
| **Role** | Solo practitioner, small firm partner |
| **Technical skill** | Low - uses GUI tools only |
| **Pain** | Can't afford Relativity; clients won't pay eDiscovery premiums |
| **Goal** | Handle discovery without outsourcing |
| **Buying power** | Limited; needs clear ROI |

### 4.3 Tertiary: Legal Technology Consultant

| Attribute | Description |
|-----------|-------------|
| **Role** | eDiscovery consultant, legal tech advisor |
| **Technical skill** | High - implements platforms |
| **Pain** | Clients have budget constraints; need flexible tools |
| **Goal** | Deliver solutions without enterprise platform dependency |
| **Buying power** | Recommends tools; influences $100K+ decisions |

---

## 5. Target Formats (Priority Order)

### 5.1 Priority 1: PST/OST Email Archives

**What:** Microsoft Outlook Personal Storage Table format. The standard for email archives in enterprises.

**Output Schema: `pst_emails`**

| Column | Source | Description |
|--------|--------|-------------|
| message_id | PR_INTERNET_MESSAGE_ID | Unique message identifier |
| subject | PR_SUBJECT | Email subject |
| sender | PR_SENDER_EMAIL_ADDRESS | From address |
| recipients_to | PR_DISPLAY_TO | To field |
| recipients_cc | PR_DISPLAY_CC | CC field |
| date_sent | PR_CLIENT_SUBMIT_TIME | Send timestamp |
| date_received | PR_MESSAGE_DELIVERY_TIME | Receive timestamp |
| body_text | PR_BODY | Plain text body |
| body_html | PR_BODY_HTML | HTML body |
| has_attachments | PR_HASATTACH | Boolean |
| folder_path | Folder hierarchy | /Inbox/Projects/Smith Matter |
| custodian | Derived | Source custodian name |
| pst_source | File path | Source PST file |

**Output Schema: `pst_attachments`**

| Column | Source | Description |
|--------|--------|-------------|
| attachment_id | Generated | Unique attachment ID |
| message_id | Parent message | FK to pst_emails |
| filename | PR_ATTACH_FILENAME | Original filename |
| extension | Derived | File extension |
| size_bytes | PR_ATTACH_SIZE | Attachment size |
| content_type | PR_ATTACH_MIME_TAG | MIME type |
| extracted_path | Local path | Where attachment was extracted |

**Implementation:** [libpff](https://github.com/libyal/libpff) (via pypff) or [extract_msg](https://pypi.org/project/extract-msg/)

**Why Priority 1:**
- Universal in enterprise environments
- High pain (no good open source options)
- Clear value (replace $5-15K processing fees)

### 5.2 Priority 2: Load Files (.dat, .opt, .lfp)

**What:** Standard eDiscovery interchange format. Tab-delimited metadata with image/native file pointers.

**Variants:**
- **Concordance DAT** - Thorn (þ) delimited, quote (þ) text qualifier
- **Relativity DAT** - Similar, with BATES numbering
- **IPRO LFP** - Image load format with page-level data
- **OPT files** - Image cross-reference (BATES → image path)

**Output Schema: `loadfile_documents`**

| Column | Source | Description |
|--------|--------|-------------|
| doc_id | BEGDOC/DOCID | Document identifier |
| bates_begin | BEGDOC | Starting BATES number |
| bates_end | ENDDOC | Ending BATES number |
| custodian | CUSTODIAN | Source custodian |
| date_sent | DATE_SENT/DATESENT | Send date (if email) |
| date_created | DATECREATED | File creation date |
| author | AUTHOR | Document author |
| subject | SUBJECT | Email subject or title |
| file_type | FILETYPE | Document type |
| native_path | NATIVE/NATIVEFILE | Path to native file |
| text_path | TEXT/TEXTPATH | Path to extracted text |
| parent_id | PARENTID | Parent document ID |
| attachment_range | BEGATTACH-ENDATTACH | Attachment BATES range |

**Implementation:** Custom parser (simple TSV with thorn delimiter)

**Why Priority 2:**
- Standard interchange format
- Enables import INTO review platforms
- Enables transformation BETWEEN platforms
- Simple to parse

### 5.3 Priority 3: Slack/Teams JSON Exports

**What:** JSON exports from modern collaboration platforms. Increasingly important for eDiscovery.

**Output Schema: `slack_messages`**

| Column | Source | Description |
|--------|--------|-------------|
| message_id | ts (timestamp) | Unique message ID |
| channel_id | Channel folder | Slack channel ID |
| channel_name | channels.json | Human-readable channel name |
| user_id | user | Sender user ID |
| user_name | users.json lookup | Sender display name |
| timestamp | ts | Message timestamp |
| text | text | Message content |
| thread_ts | thread_ts | Parent thread timestamp |
| is_reply | Derived | Is this a thread reply |
| has_attachments | files[] | Contains file attachments |
| reactions | reactions[] | Emoji reactions JSON |
| edited | edited | Edit metadata |

**Output Schema: `slack_files`**

| Column | Source | Description |
|--------|--------|-------------|
| file_id | id | Unique file ID |
| message_id | FK | Parent message |
| filename | name | Original filename |
| filetype | filetype | File type |
| size_bytes | size | File size |
| url | url_private | Download URL (may be expired) |

**Implementation:** Standard JSON parsing

**Why Priority 3:**
- Growing importance in litigation
- Courts now routinely require Slack production
- Manual review of JSON exports is painful

### 5.4 Future: Chat Exports (WhatsApp, Signal, iMessage)

**Complexity:** Variable formats, encryption issues, attachment handling

**Recommendation:** Defer to Phase 3+. Focus on enterprise collaboration first.

---

## 6. Competitive Positioning

### 6.1 The Landscape

| Player | What They Do | Price | Gap |
|--------|--------------|-------|-----|
| [Relativity](https://www.relativity.com/) | Full eDiscovery platform | $150K+/year | Overkill for small matters |
| [Nuix](https://www.nuix.com/) | Forensic processing | $50K+ | Enterprise-focused |
| [Logikcull](https://www.logikcull.com/) | Cloud eDiscovery | $250-500/GB | Per-GB gets expensive |
| [Everlaw](https://www.everlaw.com/) | Cloud review | Similar | Review, not processing |
| Manual / vendor | Outsource processing | $5-15K/matter | Expensive, slow |
| **Casparian** | Pre-processing layer | $X | Structure before review |

### 6.2 Correct Positioning

**Wrong:** "We compete with Relativity."

**Right:** "We are upstream of Relativity."

```
Raw Data → [CASPARIAN] → Structured Data → [Relativity/Everlaw] → Review
               ↑                                    ↑
          WE ARE HERE                         THEY ARE HERE
```

**Value Proposition:**
> "Casparian structures your PSTs and exports before they go into your review platform. Process in-house, pay less, move faster."

### 6.3 The "Small Law" Opportunity

| Firm Size | Can Afford Relativity? | Current Solution | Casparian Fit |
|-----------|------------------------|------------------|---------------|
| Am Law 100 | Yes | Full platform | Low (already served) |
| Mid-size (50-100 attorneys) | Sometimes | Mix of platform + vendor | Medium |
| Small (10-50 attorneys) | Rarely | Vendor outsourcing | High |
| Solo/Small (<10 attorneys) | No | Manual or decline matters | Very High |

**Target:** 80,000+ firms in the "cannot afford enterprise tools" segment.

---

## 7. Attack Strategies

### 7.1 Strategy A: "PST Liberation" (Primary Recommended)

**Positioning:** "Process your own PSTs. Stop paying vendors."

**How it works:**
1. User points Casparian at PST collection
2. Parser extracts all emails, attachments, metadata
3. Query by custodian, date range, keywords
4. Export to load file for review platform (optional)

**Value proposition:**
- "Process a 50GB PST in 30 minutes for $0 vendor cost"
- ROI: One 100GB matter saves $5-15K in processing fees

**Why we win:**
- pypff is open source but requires expertise
- No integrated solution for small firms
- Clear cost savings

**Revenue model:**
- Pro: $75/user/month (unlimited PST processing)
- Team: $300/month (multi-custodian matters)

### 7.2 Strategy B: "Load File Bridge"

**Positioning:** "Move data between platforms without vendor lock-in."

**How it works:**
1. Import Concordance DAT from legacy platform
2. Transform to Relativity DAT format
3. Export for new platform

**Value proposition:**
- Platform migration without re-processing
- Combine productions from multiple sources

**Why we win:**
- Vendors charge for conversion
- Format knowledge is tribal
- Casparian makes it self-service

### 7.3 Strategy C: "Slack Compliance"

**Positioning:** "Make your Slack export review-ready."

**How it works:**
1. Export Slack workspace
2. Casparian parses JSON, reconstructs threads
3. Query by user, channel, date range
4. Export to load file for attorney review

**Value proposition:**
- Courts require Slack production
- Native JSON is unreadable
- Casparian makes it searchable

---

## 8. Go-to-Market

### 8.1 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **Legal tech conferences** | ILTACON, LegalTech, Relativity Fest | Month 6-12 |
| **Litigation support communities** | ACEDS, legal ops groups | Month 3-6 |
| **Legal tech consultants** | Partner program | Month 6-12 |
| **Small firm associations** | State bar tech sections | Month 6-12 |

### 8.2 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "Process a PST in 5 minutes" video | Top-of-funnel | High |
| "Load file format cheat sheet" | SEO, education | Medium |
| "Slack eDiscovery guide" | Thought leadership | Medium |
| "Small firm eDiscovery toolkit" | Lead gen | High |

### 8.3 Pricing (Legal Vertical) - Value-Based

> **Pricing Philosophy:** Price by the value created, not by cost. See [STRATEGY.md](../STRATEGY.md#value-based-pricing-strategy) for framework.

#### Value Analysis

| Cost Item | Current Spend | Casparian Savings |
|-----------|---------------|-------------------|
| Vendor processing (per matter) | $5,000-15,000 | 80-90% reduction |
| Annual vendor spend (20 matters) | $100,000-300,000 | **$80,000-270,000 saved** |
| Relativity license | $150,000+/year | Not applicable (different use case) |
| Litigation support salary | $75,000-100,000 | Time savings (20+ hrs/month) |

**Additional value:** Faster turnaround (hours vs. days), better control (no data leaving premises), ability to take smaller matters profitably.

#### Pricing Tiers (Capturing 5-15% of Value)

| Tier | Price | Value Capture | Features | Target |
|------|-------|---------------|----------|--------|
| **Solo** | Free | N/A | 3 parsers, 1GB/month | Solo practitioners, evaluation |
| **Firm** | $500/user/month | ~5% | PST + load file parsers, 100GB/month, email support | Small firms (2-10 attorneys) |
| **Litigation Team** | $20,000/year | ~10% | Unlimited matter support, multi-custodian, priority support, export to any format | Active litigation teams |
| **Enterprise** | $75,000+/year | Custom | Multi-office, SSO, SLA, white-label, dedicated success manager | Large firms, consulting firms |

#### Pricing Justification

**Litigation Team tier ($20,000/year):**
- Average matter processing cost with vendors: $10,000
- Matters per year: 20-50
- Annual vendor spend: **$200,000-500,000**
- $20K captures 4-10% of vendor cost savings
- Plus faster turnaround, control, and ability to take smaller matters

**Comparison to alternatives:**
- Relativity: $150K+/year (overkill for small firms)
- GoldFynch/Logikcull: $0.25-0.50/GB = $25K-50K for 100GB matter
- Vendor processing: $5-15K per matter
- **Casparian at $20K/year: Fixed cost, unlimited matters**

#### Why Not Price Lower?

Per Andreessen's framework:
1. **$300/month ($3,600/year) doesn't prove the moat** - Firms won't trust critical litigation to "cheap" tool
2. **$300/month can't fund sales** - Legal tech sales require conference presence, demos, consultants
3. **$300/month signals "hobbyist tool"** - Litigation support managers need enterprise signals
4. **$300/month doesn't fund customer success** - Legal matters require white-glove support

#### Revenue Projection (Legal Vertical)

| Metric | 6-Month | 12-Month | 24-Month |
|--------|---------|----------|----------|
| Firm customers | 20 | 50 | 150 |
| Litigation Team customers | 5 | 20 | 60 |
| Enterprise customers | 1 | 3 | 10 |
| Avg contract value | $8,000 | $12,000 | $15,000 |
| Legal MRR | $13,333 | $38,333 | $116,667 |
| Legal ARR | $160,000 | $460,000 | $1,400,000 |

---

## 9. Implementation Roadmap

### Phase 1: PST Parser (Month 1-2 of legal vertical work)

- [ ] Implement `legal_pst.py` parser
- [ ] pypff integration for PST reading
- [ ] Output: `pst_emails`, `pst_attachments` tables
- [ ] Attachment extraction to filesystem
- [ ] Test with real-world PST samples
- [ ] Deduplication by message_id

### Phase 2: Load File Parser/Generator (Month 2-3)

- [ ] Implement `legal_loadfile.py` parser
- [ ] Support Concordance DAT format
- [ ] Support Relativity DAT format
- [ ] Output: `loadfile_documents` table
- [ ] Bidirectional: parse AND generate load files
- [ ] OPT file handling for images

### Phase 3: Slack Parser (Month 3-4)

- [ ] Implement `legal_slack.py` parser
- [ ] Thread reconstruction
- [ ] User enrichment from users.json
- [ ] Output: `slack_messages`, `slack_files` tables
- [ ] Export to load file format

### Phase 4: Teams Parser (Month 4-5)

- [ ] Implement `legal_teams.py` parser
- [ ] Handle compliance export format
- [ ] Thread/reply reconstruction
- [ ] Integration with M365 export workflows

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| PST format changes | Low | libpff is actively maintained |
| Legal sales cycle too long | High | Focus on litigation support (faster buyer) |
| Relativity adds similar features | Medium | Focus on pre-processing niche |
| Regulatory requirements we miss | Medium | Partner with legal tech consultants |
| Small firms don't have technical staff | High | GUI/TUI required; not CLI-only |

---

## 11. Success Metrics

### 11.1 Technical Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| PST files processed | 1,000 | 10,000 |
| Total email messages indexed | 10M | 100M |
| Load file conversions | 500 | 5,000 |

### 11.2 Business Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Legal vertical MRR | $5K | $25K |
| Legal customers | 25 | 150 |
| Consultant partners | 3 | 15 |

---

## 12. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Phase priority | Phase 2/3 (after Finance) | Finance has faster sales cycle; legal needs GUI |
| PST library | pypff/libpff | Most mature open source option |
| Load file format | Concordance DAT primary | Most common interchange format |
| Enterprise legal | Not target | Relativity owns this; focus on underserved |
| GUI requirement | Required for legal | Attorneys won't use CLI |

---

## 13. Open Questions

1. **pypff reliability:** How well does it handle corrupted PSTs?
2. **Attachment handling:** Extract inline or export to separate folder?
3. **Unicode issues:** How to handle email encoding edge cases?
4. **Legal hold integration:** Is this a requirement or nice-to-have?
5. **Chain of custody:** What documentation is needed for court admissibility?

---

## 14. Glossary

| Term | Definition |
|------|------------|
| **BATES number** | Sequential numbering for legal documents |
| **Concordance** | Legacy eDiscovery platform (Thomson Reuters) |
| **Custodian** | Person whose data is being collected |
| **DAT file** | Metadata load file format |
| **eDiscovery** | Electronic discovery - finding ESI for litigation |
| **ESI** | Electronically Stored Information |
| **Load file** | Metadata file for importing to review platforms |
| **MBOX** | Unix mailbox format |
| **OPT file** | Image cross-reference file |
| **PST** | Personal Storage Table (Outlook) |
| **Relativity** | Leading eDiscovery platform |
| **Review** | Attorney examination of documents for relevance |

---

## 15. References

- [libpff - PST Library](https://github.com/libyal/libpff)
- [pypff Python Bindings](https://github.com/libyal/libpff/wiki/Building#python-bindings)
- [Concordance DAT Format](https://help.relativity.com/RelativityOne/Content/Relativity/Processing/Processing_data_files.htm)
- [Slack Export Guide](https://slack.com/help/articles/201658943-Export-your-workspace-data)
- [EDRM (eDiscovery Reference Model)](https://edrm.net/)
- [ACEDS (Association of Certified eDiscovery Specialists)](https://aceds.org/)

---

## 16. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft from gap analysis |
