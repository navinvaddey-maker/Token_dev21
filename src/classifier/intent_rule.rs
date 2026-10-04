//! Deterministic intent × vertical role composition.
//!
//! Runs **before** embedding/hash classifiers. New domains and roles extend
//! the tables below; they do not require new match arms in `generate_role`.
//!
//! Layers:
//! 1. Intent class (what the user is trying to do)
//! 2. Vertical (industry)
//! 3. Role compose: generic primary = f(intent), secondary = vertical
//! 4. Precedence: legal owns the role only when the *question* is legal
//! 5. Fallback: caller uses hash classifier + margin gate

/// Goal-level intent. Independent of industry vertical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalIntent {
    WealthBuild,
    LegalQuery,
}

impl GoalIntent {
    pub fn as_str(self) -> &'static str {
        match self {
            GoalIntent::WealthBuild => "wealth-building",
            GoalIntent::LegalQuery => "legal-query",
        }
    }

    /// Generic primary role. One role scales across N verticals.
    pub fn primary_role(self) -> &'static str {
        match self {
            GoalIntent::WealthBuild => "Business Strategist",
            GoalIntent::LegalQuery => "Legal Advisor",
        }
    }

    fn pipeline_domain(self) -> &'static str {
        match self {
            GoalIntent::WealthBuild => "business",
            GoalIntent::LegalQuery => "legal",
        }
    }

    fn task_type(self) -> &'static str {
        match self {
            GoalIntent::WealthBuild => "creation",
            GoalIntent::LegalQuery => "analysis",
        }
    }
}

/// Resolved verdict from the rule layer. `None` from [`resolve_prompt_rules`]
/// means "no rule fired — use hash classifier with margin gate".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleRuleVerdict {
    pub intent: GoalIntent,
    pub vertical: Option<&'static str>,
    pub primary_role: &'static str,
    pub legal_as_constraint: bool,
}

impl RoleRuleVerdict {
    pub fn pipeline_domain(&self) -> &'static str {
        self.intent.pipeline_domain()
    }

    pub fn secondary_domain(&self) -> Option<&'static str> {
        self.vertical
    }

    pub fn intent_class(&self) -> &'static str {
        self.intent.as_str()
    }

    pub fn task_type(&self) -> &'static str {
        self.intent.task_type()
    }

    pub fn display_secondary(&self) -> Option<String> {
        self.vertical.map(title_case_slug)
    }

    pub fn persona_anchor(&self) -> String {
        match (self.intent, self.vertical) {
            (GoalIntent::WealthBuild, Some(v)) => format!(
                "Senior Business Strategist specializing in {} venture building, capital strategy, and industry structure. Finance is a sub-skill, not the owning frame.",
                title_case_slug(v)
            ),
            (GoalIntent::WealthBuild, None) => {
                "Senior Business Strategist specializing in venture building, capital strategy, and scalable enterprise value.".into()
            }
            (GoalIntent::LegalQuery, Some(v)) => format!(
                "Senior Legal Advisor specializing in {} regulatory and commercial questions. Cite jurisdiction; do not invent statutes.",
                title_case_slug(v)
            ),
            (GoalIntent::LegalQuery, None) => {
                "Senior Legal Advisor specializing in regulatory compliance, contract law, and risk. Cite jurisdiction; do not invent statutes.".into()
            }
        }
    }
}

/// Venture-scale / business-building class. Not a single keyword.
const VENTURE_SCALE: &[&str] = &[
    "billionaire",
    "billionaires",
    "billionaier",
    "billionaiers",
    "tycoon",
    "mogul",
    "magnate",
    "unicorn",
    "decacorn",
    "empire",
    "conglomerate",
    "fortune 500",
    "venture-backed",
    "venture backed",
    "ipo",
    "go public",
    "build a unicorn",
    "build an empire",
    "build a conglomerate",
    "wealthiest",
    "wealthiest man",
    "wealthiest men",
    "richest",
    "richest man",
    "richest men",
    "top earner",
    "top earners",
    "baron",
    "titan",
    "market leader",
    "industry leader",
    "dominant player",
    "multi-billionaire",
    "centimillionaire",
];

/// Wealth language that is ambiguous until a non-personal-finance vertical appears.
const WEALTH_CLASS: &[&str] = &[
    "get rich",
    "become rich",
    "become wealthy",
    "make a fortune",
    "fortune in",
    "wealth in",
    "rich in",
    "get wealthy",
    "become wealthiest",
    "become richest",
    "make billions",
    "make millions",
    "amass wealth",
    "build wealth in",
    "accumulate wealth",
    "generate fortune",
];

/// Personal-finance anchors. These keep the hash/finance path for earn/portfolio asks.
const PERSONAL_FINANCE_ANCHORS: &[&str] = &[
    "portfolio",
    "index fund",
    "401k",
    "401(k)",
    "ira ",
    "roth",
    "retirement",
    "retire early",
    "passive income",
    "savings rate",
    "budgeting",
    "budget",
    "net worth spreadsheet",
    "asset allocation",
    "earn more money",
    "earn millions",
    "salary",
    "paycheck",
];

/// Legal *question* phrasing. Industry words like "regulation" do not count.
const LEGAL_QUERY_PHRASES: &[&str] = &[
    "is it legal",
    "is it lawful",
    "is this legal",
    "are we allowed",
    "can i legally",
    "can we legally",
    "legally allowed",
    "legal to",
    "contract clause",
    "clause for",
    "draft an nda",
    "draft a contract",
    "draft an agreement",
    "review this contract",
    "indemnification",
    "does this violate",
    "would this violate",
    "breach of contract",
    "what licenses do i need to legally",
    "file a lawsuit",
    "sue for",
    "attorney for",
    "statute of",
    "gdpr apply",
    "is it compliant",
    "compliance requirements for",
    "legal risks of",
    "legal consequences",
];

/// Regulatory language that is a constraint lens, not a primary legal question.
const LEGAL_CONSTRAINT_TERMS: &[&str] = &[
    "regulation",
    "regulatory",
    "compliance",
    "license",
    "licensing",
    "fda",
    "ema",
    "approval",
    "approvals",
    "patent",
    "liability",
    "jurisdiction",
    "statute",
    "gdpr",
    "hipaa",
    "sec filing",
];

/// Industry verticals. Append a row to support a new domain — do not add roles.
const VERTICALS: &[(&str, &[&str])] = &[
    (
        "pharma",
        &[
            "pharma",
            "pharmaceutical",
            "biotech",
            "biopharma",
            "biosimilar",
            "drugs",
            "drug company",
            "therapeutics",
            "clinical trial",
            "ind filing",
            "nda filing",
            "compounded drug",
            "dharma",
            "biomedical",
            "pharmacology",
            "drug development",
            "drug manufacturing",
            "life sciences",
        ],
    ),
    (
        "fintech",
        &[
            "fintech",
            "neobank",
            "payments company",
            "payment rails",
            "lending platform",
            "crypto exchange",
            "wealthtech",
            "insurtech",
            "paytech",
            "crypto",
            "defi",
            "decentralized finance",
            "banking tech",
        ],
    ),
    (
        "energy",
        &[
            "energy",
            "oil and gas",
            "renewables",
            "solar farm",
            "wind farm",
            "utilities",
            "grid storage",
            "cleantech",
            "clean energy",
            "renewable energy",
            "green energy",
            "petroleum",
            "ev charging",
            "battery storage",
        ],
    ),
    (
        "real-estate",
        &[
            "real estate",
            "real-estate",
            "property development",
            "reit",
            "multifamily",
            "brokerage empire",
            "proptech",
            "commercial real estate",
            "residential real estate",
            "property investment",
        ],
    ),
    (
        "software",
        &[
            "saas",
            "software company",
            "devtools",
            "enterprise software",
            "cloud platform",
            "b2b saas",
            "microservices",
            "tech startup",
            "software startup",
        ],
    ),
    (
        "ecommerce",
        &[
            "ecommerce",
            "e-commerce",
            "dtc brand",
            "marketplace empire",
            "direct-to-consumer",
            "online store",
            "drop shipping",
            "retail tech",
        ],
    ),
];

const HASH_MARGIN_GAP: f32 = 0.05;

/// Minimum cosine/similarity gap between top-1 and top-2 hash scores.
pub fn hash_margin_gap() -> f32 {
    HASH_MARGIN_GAP
}

/// Returns true when the hash classifier may emit a domain.
pub fn hash_margin_allows(top: f32, second: Option<f32>, min_top: f32) -> bool {
    if top < min_top {
        return false;
    }
    match second {
        None => true,
        Some(s) => (top - s) >= HASH_MARGIN_GAP,
    }
}

/// Detect intent, vertical, and composed role. `None` = fall through to hash.
pub fn resolve_prompt_rules(raw: &str) -> Option<RoleRuleVerdict> {
    let lower = raw.to_lowercase();
    let vertical = detect_vertical(&lower);
    let legal_query = detect_legal_query(&lower);
    let wealth_build = detect_wealth_build(&lower, vertical);
    let legal_lens = contains_any(&lower, LEGAL_CONSTRAINT_TERMS);

    let intent = precedence(legal_query, wealth_build)?;
    Some(RoleRuleVerdict {
        intent,
        vertical,
        primary_role: intent.primary_role(),
        legal_as_constraint: intent == GoalIntent::WealthBuild && (legal_lens || legal_query),
    })
}

fn precedence(legal_query: bool, wealth_build: bool) -> Option<GoalIntent> {
    if legal_query {
        return Some(GoalIntent::LegalQuery);
    }
    if wealth_build {
        return Some(GoalIntent::WealthBuild);
    }
    None
}

fn detect_legal_query(lower: &str) -> bool {
    contains_any(lower, LEGAL_QUERY_PHRASES)
}

fn detect_wealth_build(lower: &str, vertical: Option<&str>) -> bool {
    if detect_legal_query(lower) {
        return false;
    }
    let venture = contains_any(lower, VENTURE_SCALE);
    if venture {
        return true;
    }
    let wealth = contains_any(lower, WEALTH_CLASS);
    if !wealth {
        return false;
    }
    if personal_finance_only(lower, vertical) {
        return false;
    }
    vertical.is_some()
}

fn personal_finance_only(lower: &str, vertical: Option<&str>) -> bool {
    if !contains_any(lower, PERSONAL_FINANCE_ANCHORS) {
        return false;
    }
    matches!(vertical, None | Some("fintech"))
        && !contains_any(lower, VENTURE_SCALE)
        && vertical.is_none()
}

fn detect_vertical(lower: &str) -> Option<&'static str> {
    let mut hit: Option<(&'static str, usize)> = None;
    for (name, synonyms) in VERTICALS {
        for syn in *synonyms {
            if !phrase_hit(lower, syn) {
                continue;
            }
            let rank = syn.len();
            if hit.map(|(_, r)| rank > r).unwrap_or(true) {
                hit = Some((*name, rank));
            }
        }
    }
    hit.map(|(n, _)| n)
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| phrase_hit(haystack, n))
}

/// Phrase match: multi-word uses substring; single tokens use word boundaries.
fn phrase_hit(haystack: &str, needle: &str) -> bool {
    if needle.contains(' ') || needle.contains('-') {
        return haystack.contains(needle);
    }
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == needle)
}

fn title_case_slug(slug: &str) -> String {
    slug.split(|c: char| c == '-' || c == '_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_role(prompt: &str, role: &str, vertical: Option<&str>) {
        let v = resolve_prompt_rules(prompt).expect(prompt);
        assert_eq!(v.primary_role, role, "{prompt}");
        assert_eq!(v.vertical, vertical, "{prompt}");
    }

    #[test]
    fn wealth_build_synonyms_use_generic_strategist() {
        expect_role(
            "How do I become a billionaire in pharma",
            "Business Strategist",
            Some("pharma"),
        );
        expect_role(
            "Become a pharma tycoon",
            "Business Strategist",
            Some("pharma"),
        );
        expect_role(
            "Build a unicorn biotech",
            "Business Strategist",
            Some("pharma"),
        );
        expect_role("Get rich in drugs", "Business Strategist", Some("pharma"));
        expect_role(
            "How to get rich in fintech",
            "Business Strategist",
            Some("fintech"),
        );
        expect_role(
            "Build an energy empire",
            "Business Strategist",
            Some("energy"),
        );
        expect_role(
            " I want to become a billionaier in pharma industry",
            "Business Strategist",
            Some("pharma"),
        );
        expect_role(
            "I want to become wealthiest men in the pharma industry. how where can I start from",
            "Business Strategist",
            Some("pharma"),
        );
        expect_role(
            "I want to become wealthiest men in the dharma industry",
            "Business Strategist",
            Some("pharma"),
        );
        expect_role(
            "Become the richest man in life sciences",
            "Business Strategist",
            Some("pharma"),
        );
        expect_role(
            "How to become a titan in renewable energy",
            "Business Strategist",
            Some("energy"),
        );
        expect_role(
            "Build a multi-billionaire SaaS startup",
            "Business Strategist",
            Some("software"),
        );
    }

    #[test]
    fn legal_query_outranks_wealth_only_when_question_is_legal() {
        let v = resolve_prompt_rules("Is it legal to sell compounded drugs").unwrap();
        assert_eq!(v.intent, GoalIntent::LegalQuery);
        assert_eq!(v.primary_role, "Legal Advisor");
        assert_eq!(v.vertical, Some("pharma"));

        let v = resolve_prompt_rules("Contract clause for a pharma licensing deal").unwrap();
        assert_eq!(v.intent, GoalIntent::LegalQuery);
        assert_eq!(v.vertical, Some("pharma"));
    }

    #[test]
    fn regulation_is_constraint_not_owner_on_wealth_path() {
        let v = resolve_prompt_rules(
            "How do I become a pharma billionaire given FDA approvals and licensing",
        )
        .unwrap();
        assert_eq!(v.intent, GoalIntent::WealthBuild);
        assert_eq!(v.primary_role, "Business Strategist");
        assert!(v.legal_as_constraint);
        assert_eq!(v.vertical, Some("pharma"));
    }

    #[test]
    fn personal_finance_does_not_fire_wealth_build() {
        assert!(resolve_prompt_rules("I want earn millions").is_none());
        assert!(resolve_prompt_rules("How to build wealth and generate passive income").is_none());
        assert!(resolve_prompt_rules("How do I build a million-dollar index portfolio").is_none());
    }

    #[test]
    fn uncovered_prompts_abstain_to_hash() {
        assert!(resolve_prompt_rules("fix the rust deadlock").is_none());
        assert!(resolve_prompt_rules("hello").is_none());
    }

    #[test]
    fn margin_gate_rejects_close_scores() {
        assert!(!hash_margin_allows(0.72, Some(0.71), 0.60));
        assert!(hash_margin_allows(0.80, Some(0.60), 0.60));
        assert!(!hash_margin_allows(0.50, Some(0.10), 0.60));
    }
}
