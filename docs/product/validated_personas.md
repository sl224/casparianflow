# Validated Personas: Evidence-Based User Profiles

**Status:** Validated via Job Postings & Forums
**Purpose:** Ground product strategy in real-world user workflows
**Version:** 1.1
**Date:** January 21, 2026

---

## v1 Target: DFIR / Incident Response

**Primary target personas for v1:**

| Title | Role | Technical Level | Why They Fit |
|-------|------|-----------------|--------------|
| **DFIR Engineer** | Parse and analyze digital forensic artifacts | Writes Python; CLI-comfortable | Core platform value |
| **Forensic Engineer** | Build evidence timelines; court-defensible analysis | Python/PowerShell; evidence handling | Lineage/quarantine = insurance |
| **IR Engineer** | Incident response; rapid triage | Python; artifact parsing | Speed + reproducibility |
| **Detection Engineer** (consumer) | Consume parsed outputs for detection logic | SQL; queries outputs | Uses outputs, not parsers |

---

## Validation Methodology

Each persona validated through:
1. **Job postings** - If companies are hiring for this role with these responsibilities
2. **Salary data** - Market validation of role value
3. **Forum discussions** - Real practitioners discussing pain points
4. **Industry publications** - Professional recognition of the problem

---

## 0. DFIR: Forensic Consultant (v1 PRIMARY)

### Validation Evidence

**Job Posting Evidence:**

> "Experience with **Python scripting** for automating forensic analysis tasks"
> — DFIR Analyst job descriptions

> "Parse **Windows Event Logs, Shimcache, Amcache, Prefetch** to build attacker timelines"
> — Incident Response job postings

> "Maintain **chain of custody** documentation for all evidence handling"
> — Digital Forensics Analyst requirements

**Market Pain (The "Fragile Scripts" Problem):**

> DFIR practitioners write Python scripts using `construct`, `kaitai struct`, or direct struct unpacking. These scripts crash on corrupted data, have no governance, and provide no audit trail.

**Market Size:**
- Global cybersecurity services: ~$150B (2024)
- Incident response services: ~$15-20B
- Average data breach cost: $4.45M (2023)

**Salary Range:**
- Entry: **$70,000**
- Mid: **$100,000-$130,000**
- Senior: **$150,000+**
- With specialized certs (GREM, GCFA): Premium pay

### Validated Persona

| Attribute | Evidence-Based Reality |
|-----------|------------------------|
| **Job Title** | DFIR Engineer, Forensic Analyst, Incident Responder |
| **Works At** | Boutique IR firms, Big 4 consulting, enterprise SOC/CIRT |
| **Technical skill** | **Python, PowerShell, forensic tools**; highly technical |
| **Pain** | Parsers crash on corrupted data; no governance on custom scripts |
| **Goal** | Build timeline without losing evidence |
| **Buying power** | Can expense tools; firm approves $500-2K/engagement |

### Why DFIR is #1 (The Winner)

| Factor | DFIR | Finance (contrast) |
|--------|------|-------------------|
| **Writes Python?** | YES (binary artifact parsing) | NO (Excel/VBA) |
| **Uses Parser Dev Loop?** | YES | NO |
| **Audit Trail Required?** | **LEGALLY MANDATED** (chain of custody) | Nice-to-have |
| **Urgency** | **EXTREME** (stop breach NOW) | High (T+1) |
| **Sales Cycle** | FAST (practitioners decide) | Medium (operations budget) |

### Casparian Value Proposition

**Pain:** "If my script deletes a row, I destroy evidence."

**Solution:** Casparian quarantines bad rows—nothing is lost. Per-row lineage proves what was processed, when, by which parser version. Reproducibility: same inputs + same parser = identical outputs (court-defensible).

---

## 1. Finance: Trade Support Analyst (DEPRIORITIZED to P3)

> **Note:** Trade Support has been deprioritized to P3 (consultant-delivered only) because they don't write parsers. See STRATEGY.md for full analysis.

### Validation Evidence

**Job Posting Evidence:**

> "Investigation of user queries via database queries using raw SQL, **log files** and process interaction, order issues, **flow breaks**, and booking issues"
> — [Velvet Jobs: Trade Support Analyst](https://www.velvetjobs.com/job-descriptions/trade-support-analyst)

> "Thorough knowledge of **FIX protocol**, FIX trade support and production support/connectivity troubleshooting... Being able to **read and understand the FIX log file** and interpret format and different tag combinations"
> — [HireITPeople: FIX Application Support Resume](https://www.hireitpeople.com/resume-database/64-java-developers-architects-resumes/339773-fix-application-production-support-analyst-resume-tx)

> "Demonstrate strong knowledge of UNIX/Linux and use **UNIX/Linux utilities to parse log files** and diagnose host issues"
> — Trade Support Analyst job requirements

> "Investigation of user queries via **log file and process interaction, session drops, order issues, flow breaks, booking issues**"
> — [Virtu Financial: FIX Connectivity Onboarding & Support](https://www.builtinnyc.com/job/fix-connectivity-onboarding-support/256331)

**Market Size:**
- **516 active Trade Support Analyst jobs** on LinkedIn (Jan 2026)
- **167 FIX Connectivity jobs** on LinkedIn

**Salary Range:**
- 25th percentile: **$73,172**
- Median: **$92,464**
- 75th percentile: **$118,244**
- Top earners (90th): **$146,545**
- Source: [ZipRecruiter](https://www.ziprecruiter.com/Salaries/Trade-Support-Analyst-Salary), [Glassdoor](https://www.glassdoor.com/Salaries/trade-support-analyst-salary-SRCH_KO0,21.htm)

### Validated Persona

| Attribute | Evidence-Based Reality |
|-----------|------------------------|
| **Job Title** | Trade Support Analyst, FIX Connectivity Analyst, Middle Office Analyst |
| **Reports To** | Manager of Operations, Head of Trade Support |
| **Team Size** | Typically 3-15 analysts depending on trading volume |
| **Education** | Bachelor's in Finance, Economics, or related field |
| **Technical Skills** | SQL, Excel, **Unix/Linux log parsing**, VBA; NOT Python experts |
| **Required Knowledge** | FIX protocol, trade lifecycle, OMS/EMS systems |
| **Work Hours** | 7am start (handle overnight breaks); market hours |
| **Salary** | $73K-$118K (mid-range); hedge funds pay $200K+ |

### Validated Workflow

**From job postings, the actual daily workflow:**

```
1. ALERT: "Trade break on ClOrdID 12345"
   Source: Monitoring system, counterparty call, or regulatory report

2. ACCESS LOGS:
   - SSH to log server (or request access)
   - Navigate to FIX log directory (/var/log/fix/)

3. GREP FOR ORDER:
   $ grep 12345 gateway_20260108.log

4. DECODE FIX MESSAGES:
   - 35=D → NewOrderSingle
   - 35=8 → ExecutionReport
   - 35=G → OrderCancelReplaceRequest
   - 39=2 → Filled

5. RECONSTRUCT TIMELINE:
   - Manually piece together order lifecycle
   - Copy to Excel for analysis
   - Time: 30-45 minutes per break

6. RESOLVE:
   - Identify root cause
   - Contact counterparty/venue
   - Update internal systems
```

**Job posting proof of workflow:**
> "Provided level 2 support to traders on sell side by **reviewing executing logs during day**"

> "Leading the **investigation and analysis of application operational failures** including writing up of incident and root cause analysis reports"

### Why They Don't Use Databricks

**From job descriptions - different teams:**
- Trade Support works in **Operations/Middle Office**
- Data teams work in **Technology/Analytics**
- Trade Support uses: Excel, SQL, Unix grep, Bloomberg
- Data teams use: Databricks, Snowflake, Python

**Access issue:** Trade Support typically doesn't have Databricks credentials; they have access to trading infrastructure servers.

**Time constraint:** T+1 settlement = breaks must be resolved in hours. Data engineering projects take months.

### Casparian Value Proposition (Validated)

**Pain from job postings:**
> "Resolving trade discrepancies under tight deadlines and managing high transaction volumes"

**Solution:**
```sql
-- Instead of 30-45 minutes of grep + manual decoding:
SELECT * FROM fix_order_lifecycle
WHERE cl_ord_id = 'ORD12345'
ORDER BY timestamp;

-- Full lifecycle in seconds
```

**ROI Calculation:**
- 40 min/break × 10 breaks/day = 6.5 hours
- Reduced to 10 min/break = 1.7 hours
- **Time saved: 4.8 hours/day per analyst**
- At $50/hour loaded cost = **$24K/year saved per analyst**

---

## 2. Legal: Litigation Support Specialist

### Validation Evidence

**Job Posting Evidence:**

> "Review, troubleshoot and **import various types of load files** into various database applications"
> — [4 Corner Resources: Litigation Support Specialist](https://www.4cornerresources.com/job-descriptions/litigation-support-specialist/)

> "Generate compliant **load files (DAT/OPT)**, text, and Bates/endorsements for downstream use"
> — Litigation Support job descriptions

> "Loading and managing datasets in eDiscovery platforms (e.g., **Relativity, Nuix, Everlaw**), applying metadata filters"
> — [Zippia: Litigation Support Specialist](https://www.zippia.com/litigation-support-specialist-jobs/what-does-a-litigation-support-specialist-do/)

**Market Pain (Pricing Evidence):**

> "Historically, **Relativity has been too expensive for most small and many mid-sized firms** - and even for smaller matters at larger firms. It is an incredibly powerful platform... but it's also incredibly complex."
> — [Proteus Discovery Blog](https://blog.proteusdiscovery.com/ediscovery-for-small-law-firms)

**Processing Costs (ACEDS Data):**
- Historical: **$500-600 per GB**
- Current: **$50 per GB or less** for processing
- Hosting: **$1-20 per GB/month**
- Example: Logikcull charges **$250-500/GB**
- Source: [ACEDS Pricing Survey](https://aceds.org/a-matter-of-pricing-a-running-update-of-semi-annual-ediscovery-pricing-survey-responses/)

**Market Size:**
- **80,000+ law firms** with <10 attorneys in US (can't afford Relativity)
- eDiscovery market: **$15B+ globally**
- Source: Industry research

**Salary Range:**
- 25th percentile: **$81,885**
- Median: **$88,273 - $103,556**
- 75th percentile: **$132,376**
- Source: [ZipRecruiter](https://www.ziprecruiter.com/Salaries/Litigation-Support-Analyst-Salary), [Glassdoor](https://www.glassdoor.com/Salaries/litigation-support-analyst-salary-SRCH_KO0,26.htm)

### Validated Persona

| Attribute | Evidence-Based Reality |
|-----------|------------------------|
| **Job Title** | Litigation Support Specialist, eDiscovery Analyst, Legal Technology Specialist |
| **Works At** | Law firms, corporate legal departments, eDiscovery vendors |
| **Education** | Paralegal certification, IT background, or legal studies |
| **Technical Skills** | Relativity, SQL, Excel, load file formats (DAT/OPT) |
| **Certifications** | CEDS (Certified eDiscovery Specialist), Relativity certifications |
| **Salary** | $82K-$132K; higher in NYC/DC |

### Validated Workflow

**From job postings:**

```
1. RECEIVE COLLECTION:
   - 200GB of PST files from departing employee
   - Or: Slack export from IT department
   - Or: Production from opposing counsel (load files)

2. PROCESS DATA:
   Option A (Expensive): Send to vendor → $5-15K, 2-3 day turnaround
   Option B (Enterprise): Use Relativity → $150K+/year license
   Option C (Per-GB): Use Logikcull → $250-500/GB = $50K-100K for this matter

3. LOAD INTO REVIEW PLATFORM:
   - Import load files (DAT/OPT format)
   - Map metadata fields
   - Run deduplication
   - Apply date filters, keyword searches

4. SUPPORT REVIEW:
   - Help attorneys with searches
   - Generate privilege logs
   - Prepare production load files for opposing counsel

5. PRODUCE:
   - Generate compliant load files
   - BATES stamp documents
   - QC before delivery
```

**Job posting proof:**
> "Administering and supporting Relativity workspaces (workspace creation, **data ingestion and processing**, importing and exporting productions)"

> "Configure and administer review platforms (Relativity, Everlaw, DISCO, Reveal), **map metadata, normalize load files (DAT/OPT)**"

### Why They Can't Use Enterprise Tools

**From industry research:**

> "For most of the past decade, Relativity was the undisputed belle of the ball for document review software in AmLaw 200 and Global 100 law firm settings. But those firms comprise **only a fraction of all litigators**."

> "If your firm has paralegals perform internal document review on a few hundred or maybe a few thousand documents 4-5 times per year, **purchasing your own Relativity account would be overkill**."

**Economic reality:**
- Relativity: $150K+/year
- Nuix: $50K+
- Vendor processing: $5-15K per matter
- Small firm budget: Can't justify for 4-5 matters/year

### Casparian Value Proposition (Validated)

**Solution:**
```bash
# Process PST locally, no vendor
casparian run pst_parser.py /collections/smith_pst/

# Query emails
SELECT * FROM pst_emails
WHERE custodian = 'John Smith'
AND date_sent BETWEEN '2024-01-01' AND '2024-12-31'
AND body_text LIKE '%contract%';

# Export to load file for Relativity
casparian export --format dat --output production_001.dat
```

**ROI:**
- Vendor cost avoided: **$5-15K per matter**
- 10 matters/year = **$50-150K saved**
- Casparian: **$300/month = $3.6K/year**

---

## 3. Healthcare: HL7 Integration Analyst

### Validation Evidence

**Job Posting Evidence:**

> "**Develop and troubleshoot message movement, translation, and integration** between Epic and other healthcare systems"
> — [Indeed: HL7 Interface Analyst jobs](https://www.indeed.com/q-hl7-interface-analyst-jobs.html)

> "Maintain support and **troubleshoot interfaces for healthcare systems, including but not limited to messages such as ADT, ORU, ORM**"
> — Healthcare IT job descriptions

> "Build, document, and support HL7 interfaces using Corepoint integration engine. Work with stakeholders to **analyze, design, document, test, troubleshoot**, and coordinate implementation"
> — [ZipRecruiter: HL7 Interface Analyst](https://www.ziprecruiter.com/Jobs/Hl7-Interface-Analyst)

**Market Context (Mirth Licensing Change):**

> "On **March 19, 2025**, NextGen Healthcare announced changes to the Mirth Connect licensing model, transitioning to a **closed-source, proprietary license** model with the release of version 4.6."
> — [Meditecs: Mirth Connect License Change](https://www.meditecs.com/kb/mirth-connect-license-change/)

> "Commercial Gold license was approximately **$20k annually**... Commercial Platinum license was approximately **$30k annually**"
> — [Mirth Community Forum](https://forums.mirthproject.io/forum/mirth-connect/general-discussion/17348-mirth-license-cost)

**Market Size:**
- Integration engines needed by every hospital, health system, lab
- Mirth Connect: Most widely deployed integration engine
- Organizations now paying $20-30K/year want more value from their HL7 investment
- **Casparian enables analytics on Mirth archives** - complementary, not competitive

**Salary Range:**
- Entry: **$48,360**
- Mid: **$65,000-$80,000**
- Senior: **$119,070**
- Mirth specialists: **$180K-$200K** for senior roles
- Source: [Bureau of Labor Statistics](https://www.bls.gov/), [ZipRecruiter](https://www.ziprecruiter.com/Jobs/Mirth-Connect)

### Validated Persona

| Attribute | Evidence-Based Reality |
|-----------|------------------------|
| **Job Title** | HL7 Interface Analyst, Integration Analyst, Mirth Administrator |
| **Works At** | Hospitals, health systems, HIEs, EHR vendors |
| **Education** | IT/CS degree, healthcare informatics, or equivalent experience |
| **Technical Skills** | HL7 v2.x (ADT, ORU, ORM), Mirth Connect, SQL, JavaScript |
| **Certifications** | HL7 certification, vendor certifications (Epic, Cerner) |
| **Key Knowledge** | Message segments (PID, OBR, OBX), integration patterns |
| **Salary** | $65K-$119K; Mirth specialists up to $200K |

### Validated Workflow

**From job postings:**

```
1. REAL-TIME INTEGRATION (Primary job):
   - Configure Mirth channels
   - Route ADT messages to downstream systems
   - Transform between formats
   - Monitor for errors

2. TROUBLESHOOTING (Reactive):
   - "Why didn't the lab result reach the EHR?"
   - Review message logs in Mirth
   - Check transformation rules
   - Fix and reprocess

3. HISTORICAL ANALYSIS (Pain point - not well served):
   - "How many ADT messages did we process last month?"
   - "Which sending facility had the most errors?"
   - "Show me all messages for patient X"
   - Must: Export from Mirth → manual analysis
   - No native SQL query capability for archives
```

**Job posting proof:**
> "This role is responsible for analyzing HL7 messages with the purpose of **producing actionable insights** to support the provider portal, CMS electronic Clinical Quality Measures, population health reporting"
> — [Civitas Health: HL7 Data Analyst](https://www.civitasforhealth.org/wp-content/uploads/2023/03/HL7-Data-Analyst.pdf)

### Archive Analysis Gap

**From Mirth Community forums:**

> User asking about "**java.lang.OutOfMemoryError when archive enabled**" with 100,000+ messages
> — [Mirth Community](https://forums.mirthproject.io/forum/mirth-connect/support/12335-java-lang-outofmemoryerror-when-archive-enabled)

**Problem:** Mirth is optimized for real-time message routing, not historical analytics. Querying archives is DIY.

### Casparian Value Proposition (Validated)

**Target:** Archive analysis, not real-time routing (don't compete with Mirth)

**Solution:**
```bash
# Point at HL7 archive directory
casparian scan /mirth_archives/ADT_Inbound/ --tag hl7_adt

# Parse to SQL
casparian run hl7_parser.py --tag hl7_adt

# Query
SELECT
  sending_facility,
  COUNT(*) as msg_count,
  COUNT(CASE WHEN ack_code = 'AE' THEN 1 END) as errors
FROM hl7_messages
WHERE msg_type = 'ADT'
GROUP BY sending_facility
ORDER BY errors DESC;
```

**Market Timing:**
- Mirth went commercial March 2025 - organizations paying more for integration
- Organizations want more value from their HL7 data investment
- **Casparian is complementary** - we analyze Mirth's archives, not replace Mirth
- **Casparian fills archive analysis gap** that Mirth was never designed to address

---

## 4. Defense: Intelligence Analyst (DDIL/Edge)

### Validation Evidence

**Job Posting Evidence:**

> "Prior experience with **cyber incident response**, especially on DoD networks, and **digital forensics** is required"
> — [Indeed: Cyber Security PCAP Intelligence Analyst](https://www.indeed.com/q-Cyber-Security-Pcap-Intelligence-Analyst-jobs.html)

> "Exploit target digital networks and produce **network enumeration maps** depicting key nodes and switches"
> — [ClearanceJobs: SIGINT Analyst](https://www.clearancejobs.com/q-sigint-analyst)

> "Experience with **Edge Cloud Computing in a Disconnected, Denied, Intermittent, Limited (DDIL) environment**"
> — DoD job posting via Leidos

**Market Pain (Analyst Tools):**

> "Too much information is being produced too quickly for an intelligence analyst to even comprehend it using **current analysis techniques and software**, much less derive meaningful intelligence from it"
> — [War on the Rocks: ABCs of AI-Enabled Intelligence Analysis](https://warontherocks.com/2020/02/the-abcs-of-ai-enabled-intelligence-analysis/)

> "Nearly all analysis software products in use today — including advanced systems like **Palantir or Analyst Notebook — are closed systems** that do not allow analysts to code custom algorithms"
> — War on the Rocks

**DDIL Challenge:**

> "DDIL environments can **restrict real-time communication, limit data transfer** and make it difficult to coordinate across military units and systems"
> — [FedTech Magazine](https://fedtechmagazine.com/article/2025/03/ddil-environments-managing-cloud-edge-computing-defense-agencies-perfcon)

**Market Size:**
- **4,617 Cyber Security PCAP Intelligence Analyst jobs** on Indeed
- **3,587 Geospatial Intelligence Imagery Analyst jobs** on Indeed
- TAK ecosystem: **500,000+ users** (military + civilian)

**Salary Range:**
- Entry: **$54,000**
- Mid: **$77,494** (GEOINT average)
- Senior: **$175,000+**
- With TS/SCI + Poly: Premium pay
- Source: [ZipRecruiter](https://www.ziprecruiter.com/Jobs/Sigint-Analyst), [ClearanceJobs](https://www.clearancejobs.com/)

### Validated Persona

| Attribute | Evidence-Based Reality |
|-----------|------------------------|
| **Job Title** | Intelligence Analyst, SIGINT Analyst, GEOINT Analyst, All-Source Analyst |
| **Works At** | DoD, IC agencies, defense contractors (Palantir, Leidos, SAIC) |
| **Clearance** | TS/SCI required; some positions require polygraph |
| **Education** | Bachelor's in national security, CS, or 8+ years IC experience |
| **Technical Skills** | SIGINT tools (DataXplorer, ICREACH), GIS (ArcGIS), Python/R |
| **Environment** | Often DDIL - disconnected, limited bandwidth |
| **Salary** | $77K median; $175K+ for senior cleared roles |

### Validated Workflow (DDIL Context)

**From job postings and publications:**

```
1. COLLECTION (Edge/Tactical):
   - Sensors collect data: PCAP, imagery, SIGINT
   - Stored locally on tactical systems
   - Limited/no connectivity to cloud

2. EXPLOITATION (Local Processing Required):
   - Analyst needs to query collected data
   - Tools available: Wireshark (interactive), custom scripts
   - Pain: "Closed systems that do not allow analysts to code custom algorithms"

3. ANALYSIS:
   - Fuse multiple sources
   - Identify patterns
   - Produce intelligence reports

4. DISSEMINATION:
   - When connectivity available, share findings
   - Time-sensitive intelligence has degraded value if delayed
```

**Key constraint from publications:**
> "Because of the DDIL environment, it is **impractical to transport all data** collected from the tactical edge to the cloud for model training, so **edge services are needed**"
> — [LinkedIn: Common Disconnected Tactical Edge Workloads](https://www.linkedin.com/pulse/common-disconnected-tactical-edge-workloads-modern-maccalman-ph-d-)

### Why Palantir Isn't Enough

**From analyst critique:**

> "Nearly all analysis software products in use today — including advanced systems like **Palantir** or Analyst Notebook — are **closed systems** that do not allow analysts to code custom algorithms, use the latest machine-learning algorithms... or even allow analysts to provide feedback"

**Palantir's model:**
- Requires structured data as **input**
- Cloud/server-based
- Custom development requires Palantir engineers
- Multi-million dollar contracts

**Gap:** Who structures the raw data (CoT XML, PCAP, NITF) BEFORE it goes to Palantir?

### Casparian Value Proposition (Validated)

**Position:** Upstream of Palantir - structure raw tactical data

**Solution:**
```bash
# On tactical laptop (DDIL environment)
casparian run cot_parser.py /mission_data/tracks/
casparian run pcap_parser.py /mission_data/network/

# Query locally
SELECT * FROM cot_tracks
WHERE callsign LIKE 'ALPHA%'
AND timestamp > '2026-01-08 06:00:00';

SELECT src_ip, dst_ip, COUNT(*)
FROM pcap_flows
GROUP BY src_ip, dst_ip
ORDER BY COUNT(*) DESC
LIMIT 20;
```

**Differentiators:**
- **Runs on laptop** - no server required
- **Air-gapped** - no network needed
- **Open** - analysts can see exactly what parser does
- **SQL output** - familiar query interface

---

## Summary: Validated Pain Points by Vertical

| Vertical | Job Market | Validated Pain | Evidence |
|----------|------------|----------------|----------|
| **Finance** | 516+ jobs | "Read and understand FIX log file"; 30-45 min per break | Job postings explicitly mention log analysis |
| **Legal** | 80K+ firms | "Relativity too expensive for small/mid firms" | $150K+ vs. $5K budgets |
| **Healthcare** | Active market | Archive analysis gap (Mirth routes, doesn't analyze) | Mirth Community forums; analysts need SQL access to historical HL7 |
| **Defense** | 4,600+ PCAP jobs | "Closed systems"; DDIL constraints | War on the Rocks; DoD job requirements |

---

## References

### Finance
- [Velvet Jobs: Trade Support Analyst](https://www.velvetjobs.com/job-descriptions/trade-support-analyst)
- [ZipRecruiter: Trade Support Analyst Salary](https://www.ziprecruiter.com/Salaries/Trade-Support-Analyst-Salary)
- [LinkedIn: Trade Support Analyst Jobs](https://www.linkedin.com/jobs/trade-support-analyst-jobs)
- [Virtu Financial: FIX Connectivity](https://www.builtinnyc.com/job/fix-connectivity-onboarding-support/256331)

### Legal
- [4 Corner Resources: Litigation Support Specialist](https://www.4cornerresources.com/job-descriptions/litigation-support-specialist/)
- [ACEDS: eDiscovery Pricing Survey](https://aceds.org/a-matter-of-pricing-a-running-update-of-semi-annual-ediscovery-pricing-survey-responses/)
- [Proteus Discovery: eDiscovery for Small Firms](https://blog.proteusdiscovery.com/ediscovery-for-small-law-firms)
- [GoldFynch: PST Processing](https://goldfynch.com/blog/2018/09/17/how-to-open-pst-files-simple-email-ediscovery-for-small-law-firms.html)

### Healthcare
- [Indeed: HL7 Interface Analyst Jobs](https://www.indeed.com/q-hl7-interface-analyst-jobs.html)
- [Meditecs: Mirth Connect License Change](https://www.meditecs.com/kb/mirth-connect-license-change/)
- [Mirth Community Forums](https://forums.mirthproject.io/)
- [CapMinds: Mirth Connect 2025 Guide](https://www.capminds.com/blog/mirth-connect-for-healthcare-integration-a-complete-2025-guide/)

### Defense
- [War on the Rocks: AI-Enabled Intelligence Analysis](https://warontherocks.com/2020/02/the-abcs-of-ai-enabled-intelligence-analysis/)
- [FedTech Magazine: DDIL Environments](https://fedtechmagazine.com/article/2025/03/ddil-environments-managing-cloud-edge-computing-defense-agencies-perfcon)
- [ClearanceJobs: Intelligence Analyst](https://www.clearancejobs.com/)
- [Indeed: PCAP Intelligence Analyst](https://www.indeed.com/q-Cyber-Security-Pcap-Intelligence-Analyst-jobs.html)

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 1.0 | Initial validated personas based on job posting research |
| 2026-01-08 | 1.1 | **Healthcare positioning fix:** Clarified Casparian is complementary to Mirth, not a replacement |
| 2026-01-21 | 1.2 | **DFIR-first update:** Added DFIR Forensic Consultant as Section 0 (v1 PRIMARY); Added v1 target persona table; Marked Finance Trade Support as deprioritized to P3 |
