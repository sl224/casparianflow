# Mid-Size Business Market Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 2 (Target Market)
**Version:** 0.1
**Date:** January 8, 2026

---

## 1. Executive Summary

This substrategy details how Casparian Flow captures the mid-size business (100-500 employees) data integration market by positioning against enterprise ETL tools (Fivetran, Airbyte) and manual CSV/Excel workflows.

**Core Insight:** Mid-size businesses are trapped between enterprise solutions they can't afford and manual Excel processes that don't scale. They have ERP/accounting exports but no easy way to consolidate and analyze them.

**Primary Attack Vector:** "Data Team of One" - enable the single analyst or IT person responsible for reporting to automate data pipelines without a data engineering team.

---

## 2. Market Overview

### 2.1 Key Format Landscapes

| Format | Source | Prevalence | Complexity |
|--------|--------|------------|------------|
| **QuickBooks exports** | Accounting | 80%+ of US SMBs | Low |
| **Sage exports** | Accounting/ERP | 20%+ of mid-market | Low-Medium |
| **NetSuite exports** | ERP | Growing mid-market share | Medium |
| **CSV/Excel** | Everything | Universal | Variable |
| **CRM exports** | Salesforce, HubSpot | 60%+ adoption | Low |
| **Payroll exports** | ADP, Gusto, Paychex | Universal | Low |

### 2.2 Market Size

| Segment | Size | Growth |
|---------|------|--------|
| Mid-size business IT spending | $150B+ | 6-8% CAGR |
| SMB data integration market | $5B+ | 15%+ CAGR |
| Accounting software market | $20B+ | 8%+ CAGR |
| BI/Analytics tools (SMB) | $8B+ | 12%+ CAGR |

### 2.3 The Excel Problem

90% of Excel spreadsheets contain errors (research studies). Mid-size businesses rely on manual Excel processes that are:
- **Error-prone**: Manual copy-paste introduces mistakes
- **Undocumented**: "Bob knows how to run the report"
- **Slow**: Monthly close takes weeks
- **Risky**: Finance person leaves, reporting breaks

**Window of Opportunity:** Growing mid-size businesses need to graduate from Excel but can't justify enterprise ETL costs ($500K+ for Fivetran at scale).

---

## 3. Where Mid-Size Business Data Lives (Domain Intelligence)

### 3.1 Accounting System Exports

Every mid-size business exports from QuickBooks, Sage, or NetSuite:

```
Accounting exports:
C:\Users\Controller\Downloads\
├── qb_trial_balance_202601.xlsx       # Manual export
├── qb_ar_aging_202601.csv             # Accounts receivable
├── qb_ap_aging_202601.csv             # Accounts payable
├── chart_of_accounts.iif              # QuickBooks IIF format
└── journal_entries_202601.xlsx        # GL details
```

**Characteristics:**
- Manual exports (click "Export" → save to Downloads)
- Inconsistent naming conventions
- Multiple formats (IIF, QBO, CSV, Excel)
- Timestamps in filenames (or not)

**Implications for Casparian:**
- Parser must handle chaotic file naming
- Support IIF format (QuickBooks-specific)
- Schema must normalize across QB/Sage/NetSuite
- Handle partial exports and duplicates

### 3.2 ERP System Exports

Growing businesses on NetSuite, SAP Business One, or Microsoft Dynamics:

```
ERP exports:
\\server\finance\monthly_reports\
├── netsuite/
│   ├── saved_search_customers.csv     # Saved search export
│   ├── financial_report_202601.xlsx   # Built-in report
│   └── suiteql_export.csv             # Custom SQL export
├── inventory_valuation.xlsx           # Inventory report
└── open_orders.csv                    # Sales pipeline
```

**Characteristics:**
- More structured than QB exports
- Saved searches and custom reports
- REST API available (but rarely used)
- Larger data volumes

**Implications for Casparian:**
- NetSuite-specific parsers for common exports
- Handle multi-subsidiary structures
- Support incremental processing

### 3.3 CRM Exports

Sales and marketing data from Salesforce, HubSpot, Pipedrive:

```
CRM exports:
C:\Users\SalesOps\Desktop\CRM_Data\
├── sf_opportunities_202601.csv        # Salesforce export
├── sf_accounts_all.csv                # Account master
├── hubspot_contacts.xlsx              # Marketing contacts
└── closed_won_q4.xlsx                 # Custom report
```

**Characteristics:**
- Opportunity/deal data with custom fields
- Contact and account hierarchies
- Activity logs (calls, emails, meetings)
- Marketing attribution data

**Implications for Casparian:**
- Flexible schema for custom CRM fields
- Handle Salesforce-specific export quirks
- Normalize across CRM platforms

### 3.4 Payroll and HR Exports

HR data from ADP, Gusto, Paychex:

```
Payroll exports:
\\server\hr\payroll\
├── adp_payroll_202601.csv             # Pay register
├── adp_benefits_202601.xlsx           # Benefits detail
├── employee_census.xlsx               # Active employees
└── 401k_contributions.csv             # Retirement plan
```

**Characteristics:**
- Sensitive PII (SSN, salary)
- Bi-weekly or monthly cycles
- Compliance requirements (tax, benefits)
- Multiple payroll systems in M&A scenarios

**Implications for Casparian:**
- Security: PII handling, access controls
- Normalize across payroll providers
- Support multi-entity structures

---

## 4. Target Personas

### 4.1 Primary: Finance/FP&A Analyst

| Attribute | Description |
|-----------|-------------|
| **Role** | FP&A Analyst, Staff Accountant, Controller |
| **Technical skill** | Excel expert, SQL beginner, maybe Python curious |
| **Pain** | Monthly close takes weeks; reports are manual |
| **Goal** | Automate reporting; reduce close timeline |
| **Buying power** | Discretionary budget for tools; influences larger purchases |

**Current Workflow (painful):**
1. Export data from QuickBooks/NetSuite (manual)
2. Copy into "Master Workbook" (Excel)
3. VLOOKUP hell to join datasets
4. Fix errors when formulas break
5. Email report to executives
6. Repeat monthly, forever

**Casparian Workflow:**
1. Export to shared folder (one-time process setup)
2. `casparian scan \\server\finance --tag monthly_close`
3. `casparian process --tag monthly_close`
4. Query consolidated data, auto-refresh dashboards

### 4.2 Secondary: IT Manager / Business Systems Admin

| Attribute | Description |
|-----------|-------------|
| **Role** | IT Manager, Systems Administrator, "IT Person" |
| **Technical skill** | Generalist, some scripting |
| **Pain** | Finance requests for data; no data engineering skills |
| **Goal** | Enable self-service analytics without custom dev |
| **Buying power** | Budget for tools; IT decision maker |

### 4.3 Tertiary: Operations / Business Analyst

| Attribute | Description |
|-----------|-------------|
| **Role** | Operations Manager, Business Analyst, COO |
| **Technical skill** | Excel proficient, wants dashboards |
| **Pain** | Data is scattered; no single source of truth |
| **Goal** | Consolidated view across systems |
| **Buying power** | Budget for operational tools |

---

## 5. Competitive Positioning

### 5.1 Fivetran/Airbyte vs Casparian Flow

| Feature | Fivetran/Airbyte | Casparian Flow |
|---------|------------------|----------------|
| **Cost** | $500-5,000+/month at scale | **$50-200/month** |
| **Setup** | API connectors, cloud infra | **Local, file-based** |
| **Custom Formats** | Limited | **Unlimited (Python)** |
| **Data Residency** | Cloud | **Local-first** |
| **Technical Skill** | Data engineering | **Analyst-friendly** |
| **AI Integration** | None | **Optional AI (future)** |

**Positioning:** "Fivetran for the 95% without data engineers."

### 5.2 Where We Fight

**DO NOT** compete on Day 1:
- Real-time API connectors
- Data warehouse hosting
- BI/Visualization
- Enterprise data catalog

**DO** compete on:
- **Export processing** - Turn manual exports into automated pipelines
- **Multi-system joins** - QB + Salesforce + ADP in one query
- **Cost** - 10x cheaper than enterprise ETL
- **Simplicity** - No cloud infrastructure required

### 5.3 Other Competitors

| Competitor | Strength | Weakness | Our Angle |
|------------|----------|----------|-----------|
| **Fivetran** | 500+ connectors | Expensive, cloud-only | Local, 10x cheaper |
| **Airbyte** | Open source | Complex self-host | Simpler, focused |
| **Stitch** | Lightweight ETL | Limited transforms | Full Python flexibility |
| **Excel + Power Query** | Free, familiar | Fragile, undocumented | Versioned, auditable |
| **Manual Python** | Free | Requires eng skills | AI-assisted, batteries included |

---

## 6. Attack Strategies

### 6.1 Strategy A: "QuickBooks Liberation" (Accounting Play)

**Positioning:** "Get your data out of QuickBooks."

**How it works:**
1. Ship QB parser with Casparian (IIF, QBO, CSV)
2. Controller exports to shared folder
3. Casparian builds queryable database

**Value proposition:**
- "QuickBooks Online limits your exports. Casparian doesn't."
- Consolidated GL across multiple QB files
- Historical trend analysis (QB Online purges data)

**Why we win:**
- QuickBooks API is limited and expensive
- Manual exports are common but painful
- Casparian makes exports useful

**Revenue model:**
- Free: Basic QB CSV parsing
- Pro: IIF format, multi-file consolidation
- Team: Multi-entity, audit trails

**Best for:** Accounting firms, multi-entity businesses

### 6.2 Strategy B: "Data Team of One" ⭐ RECOMMENDED

**Positioning:** "Enterprise data integration without the enterprise."

**How it works:**
1. Analyst identifies data sources (QB, Salesforce, ADP)
2. Casparian AI suggests schemas and joins
3. Scheduled exports → automated processing
4. Query layer feeds existing dashboards

**Value proposition:**
- "You don't need a data engineer. You need Casparian."
- One person can build what used to require a team
- From Excel hell to SQL queries

**Why we win:**
- Fivetran costs $50K+/year for mid-size businesses
- Airbyte requires DevOps skills to self-host
- Casparian runs on analyst's laptop, scales to server

**Revenue model:**
- Free: 3 parsers, local only
- Pro: Unlimited parsers, Scout, team sharing
- Team: Scheduled processing, audit trails

**Best for:** Growing mid-size businesses (100-500 employees)

### 6.3 Strategy C: "ERP Migration Assistant" (Transition Play)

**Positioning:** "Migrate your data, keep your sanity."

**How it works:**
1. Export from legacy ERP (QB, Sage)
2. Casparian normalizes to standard schema
3. Import into new ERP (NetSuite, etc.)

**Value proposition:**
- "ERP migrations fail because of data. We fix that."
- Validate data before migration
- Historical data preservation

**Why we win:**
- Migration services cost $50-100K+
- DIY migrations fail 60%+ of the time
- Casparian reduces migration risk

**Revenue model:**
- Project-based: $5-10K per migration
- Ongoing: Pro tier for continued use

**Best for:** ERP implementation partners, IT consultants

---

## 7. Premade Parsers

### 7.1 QuickBooks Parser (`quickbooks_parser.py`)

**Input:** QuickBooks Desktop exports (IIF, CSV), QuickBooks Online exports (CSV, Excel)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `qb_accounts` | Chart of accounts |
| `qb_customers` | Customer master |
| `qb_vendors` | Vendor master |
| `qb_transactions` | All transactions (normalized) |
| `qb_invoices` | Invoice details |
| `qb_bills` | Bill details |
| `qb_journal_entries` | Journal entries |

**Key Fields:**
- `account_type`: Asset, Liability, Equity, Income, Expense
- `account_number`, `account_name`
- `transaction_date`, `transaction_type`
- `debit`, `credit`, `amount`
- `customer_name`, `vendor_name`

**Features:**
- IIF format parsing (QuickBooks Desktop)
- Multi-company file consolidation
- Elimination entries for intercompany

### 7.2 Salesforce Export Parser (`salesforce_parser.py`)

**Input:** Salesforce data export (CSV from Reports, Data Loader)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `sf_accounts` | Account master |
| `sf_contacts` | Contact details |
| `sf_opportunities` | Opportunity pipeline |
| `sf_activities` | Tasks, events, calls |
| `sf_custom` | Custom object data |

**Key Fields:**
- `salesforce_id`: 18-character Salesforce ID
- `account_name`, `owner_name`
- `stage`, `amount`, `close_date` (opportunities)
- `created_date`, `last_modified_date`

**Features:**
- Handle custom fields dynamically
- Relationship mapping (Account → Contact → Opportunity)
- Activity timeline construction

### 7.3 Generic ERP Parser (`erp_export_parser.py`)

**Input:** NetSuite, Sage, Dynamics exports (CSV, Excel)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `erp_gl_detail` | General ledger transactions |
| `erp_customers` | Customer master (normalized) |
| `erp_vendors` | Vendor master (normalized) |
| `erp_inventory` | Inventory items and quantities |
| `erp_orders` | Sales orders / purchase orders |

**Key Fields:**
- `source_system`: NetSuite, Sage, Dynamics
- `entity_id`: Customer/Vendor identifier
- `document_number`: Invoice, PO, SO number
- `posting_date`, `amount`, `currency`

### 7.4 Payroll Parser (`payroll_parser.py`)

**Input:** ADP, Gusto, Paychex exports (CSV, Excel)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `payroll_employees` | Employee master (anonymized option) |
| `payroll_earnings` | Earnings by employee, period |
| `payroll_deductions` | Deductions by type |
| `payroll_taxes` | Tax withholdings |
| `payroll_summary` | Period totals |

**Key Fields:**
- `employee_id`: Internal identifier (not SSN)
- `pay_period_start`, `pay_period_end`
- `gross_pay`, `net_pay`
- `regular_hours`, `overtime_hours`

**Features:**
- PII anonymization option
- Multi-state tax handling
- Benefits allocation

---

## 8. Go-to-Market

### 8.1 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **Accounting communities** | CPA forums, r/Accounting, LinkedIn | Month 1-3 |
| **FP&A communities** | FP&A trends, CFO.com, AFP | Month 1-6 |
| **QuickBooks ecosystem** | QuickBooks Marketplace, ProAdvisor network | Month 3-6 |
| **IT consultants** | MSP channel (see STRATEGY.md) | Month 3-9 |

### 8.2 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "Automate your QuickBooks exports" video | Top-of-funnel | High |
| "FP&A without a data team" blog | SEO, persona targeting | High |
| "Excel to SQL migration guide" | Education, trust | Medium |
| "Monthly close in 3 days, not 3 weeks" | ROI case study | Medium |

### 8.3 Pricing (Mid-Size Business Vertical)

| Tier | Price | Features | Target |
|------|-------|----------|--------|
| **Free** | $0 | QuickBooks parser, 3 custom parsers | Individual analysts |
| **Pro** | $50/user/month | Unlimited parsers, Scout, team sharing | FP&A teams |
| **Business Team** | $300/month | Multi-entity, scheduled runs, audit logs | Finance departments |
| **Enterprise** | Custom | SSO, advanced security, SLA | Large mid-market |

---

## 9. Success Metrics

### 9.1 Adoption Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| QuickBooks parser users | 500 | 2,500 |
| Multi-system users (QB + CRM + etc) | 100 | 500 |
| Files processed (mid-size biz) | 200K | 2M |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Mid-size business MRR | $8K | $40K |
| Mid-size business customers | 75 | 400 |
| Average revenue per customer | $100/month | $100/month |

### 9.3 Competitive Metrics

| Metric | Target |
|--------|--------|
| "QuickBooks data export" search ranking | Top 10 |
| "FP&A automation tools" search ranking | Top 10 |
| QuickBooks Marketplace listing | In Year 1 |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| QuickBooks/Intuit changes export formats | Medium | Version detection, quick updates |
| Mid-size buyers have no budget | Medium | Prove ROI quickly; free tier adoption |
| Fivetran releases mid-market tier | Medium | Stay 10x cheaper; local-first advantage |
| Too horizontal, lose focus | High | Focus on finance use cases first |
| Support burden from non-technical users | High | AI-assisted troubleshooting; docs |

---

## 11. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Initial attack vector | Data Team of One (B) | Broadest appeal; clearest pain |
| Primary parser | QuickBooks | 80%+ market share in SMB |
| Secondary parsers | Salesforce, ADP | Common combinations |
| NetSuite API connector | Deferred | Export-first approach |
| BI integration | Deferred | Focus on data prep, not viz |

---

## 12. References

- [QuickBooks IIF Format](https://quickbooks.intuit.com/learn-support/en-us/help-article/iif-overview)
- [Fivetran Pricing](https://www.fivetran.com/pricing)
- [Airbyte](https://airbyte.com/)
- [Stitch Data](https://www.stitchdata.com/)
- [EdgarTools](https://edgartools.readthedocs.io/)
- [FP&A Trends](https://fpa-trends.com/)

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft |
