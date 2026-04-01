use super::PrincipleResult;
use once_cell::sync::Lazy;
use regex::Regex;
use std::time::Instant;

pub struct RoleFrame {
    pub role: &'static str,
    pub audience: &'static str,
    pub schema: &'static str,
    pub constraints: &'static str,
    pub synthesis: &'static str,
}

// Precompiled verb rewrite patterns (used to transform tasks into "CRISP verbs").
static RE_COMPARE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bcompare\s*and\s*contrast\b").expect("valid compare regex"));
static RE_DISCUSS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bdiscuss\b").expect("valid discuss regex"));
static RE_EXPLORE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bexplore\b").expect("valid explore regex"));
static RE_EXPLAIN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bexplain\b").expect("valid explain regex"));
static RE_COVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bcover\b").expect("valid cover regex"));

/// Heuristic Use Case Auto-Detection
/// This function scans the raw input text for domain-specific keywords
/// to automatically determine the most appropriate CRISP Role Frame.
/// Returns the detected use-case name or "generic" as a fallback.
pub fn detect_use_case(text: &str) -> String {
    let lower = text.to_lowercase();

    if lower.contains("ticket")
        || lower.contains("bug")
        || lower.contains("issue")
        || lower.contains("support")
    {
        return "ticket".to_string();
    }
    if lower.contains("contract")
        || lower.contains("legal")
        || lower.contains("clause")
        || lower.contains("risk")
    {
        return "legal".to_string();
    }
    if lower.contains("resume")
        || lower.contains("cv")
        || lower.contains("candidate")
        || lower.contains("hiring")
        || lower.contains("salary")
    {
        return "resume".to_string();
    }
    if lower.contains("code")
        || lower.contains("function")
        || lower.contains("rust")
        || lower.contains("javascript")
        || lower.contains("api")
    {
        return "code".to_string();
    }
    if lower.contains("research")
        || lower.contains("study")
        || lower.contains("abstract")
        || lower.contains("academic")
    {
        return "research".to_string();
    }
    if lower.contains("meeting")
        || lower.contains("transcript")
        || lower.contains("agenda")
        || lower.contains("minutes")
    {
        return "transcript".to_string();
    }
    if lower.contains("financial")
        || lower.contains("audit")
        || lower.contains("budget")
        || lower.contains("revenue")
        || lower.contains("expense")
    {
        return "financial".to_string();
    }

    "generic".to_string()
}

pub fn get_role_frame(use_case: &str) -> RoleFrame {
    let default_constraints = "- Strict 220-280 words per unit block (hard ceiling)\n- Equal depth across all units regardless of topic familiarity\n- No transitional filler\n- No meta-commentary";

    // Fallback logic for auto-detection
    // If the use_case is empty or explicitly "auto", we use generic as the default
    // (Actual auto-detection happens at the pipeline run level if requested)
    match use_case {
        "ticket" => RoleFrame { 
            role: "data analyst with expertise in support triage", 
            audience: "engineering team", 
            schema: "## [Ticket ID]\n**Priority:** [High/Medium/Low]\n**Category:** [Category]\n**Action Required:** [1-2 sentences]",
            constraints: default_constraints,
            synthesis: "Close with a 3-column table: Ticket ID | Category | Owner."
        },
        "legal" => RoleFrame { 
            role: "legal auditor", 
            audience: "senior counsel", 
            schema: "## [Clause Name]\n**Risk Level:** [High/Medium/Low]\n**Reference:** [Clause Reference]\n**Severity Justification:** [1-2 sentences]",
            constraints: default_constraints,
            synthesis: "Close with a 3-column table: Clause | Risk Level | Recommended Action."
        },
        "resume" => RoleFrame { 
            role: "talent screener", 
            audience: "hiring manager", 
            schema: "## [Candidate Name]\n**Fit Score:** [1-10]\n**Key Strengths:** [2-3 bullets]\n**Red Flags:** [1-2 bullets]\n**Reasoning:** [1-2 sentences]",
            constraints: default_constraints,
            synthesis: "Close with a ranking list of top 3 candidates based on fit score."
        },
        "code" => RoleFrame { 
            role: "senior software engineer specialising in distributed systems", 
            audience: "software engineers", 
            schema: "## [Function Name]\n**Purpose:** [1-2 sentences]\n**Parameters:**\n- [Param 1]: [Type] - [Description]\n**Returns:** [Type] - [Description]",
            constraints: "- Strict 220-280 words per unit block (hard ceiling)\n- Equal depth across all units regardless of topic familiarity\n- No conversational filler",
            synthesis: "Close with a summary paragraph of the overall system interactions."
        },
        "research" => RoleFrame { 
            role: "comparative researcher", 
            audience: "academic peers", 
            schema: "## [Era Name / Finding]\n**Core Ideas:** [3-5 bullets]\n**Key Thinkers:** [Name - contribution - 1 sentence]\n**Lasting Influence:** [2-3 sentences]\n**Internal Contradiction:** [1 specific named tension]\n**Domain Lens:**\n- Philosophical: [1 sentence]\n- Economic: [1 sentence]\n- Political: [1 sentence]\n- Scientific: [1 sentence]",
            constraints: default_constraints,
            synthesis: "Close with a 3-column table: Era/Finding | Biggest Legacy | Biggest Failure."
        },
        "transcript" => RoleFrame { 
            role: "meeting coordinator", 
            audience: "project stakeholders", 
            schema: "## [Action Item]\n**Owner:** [Name]\n**Deadline:** [Date/Time]\n**Context:** [1 sentence]",
            constraints: default_constraints,
            synthesis: "Close with an unordered list of distinct owners and their total action item count."
        },
        "financial" => RoleFrame { 
            role: "financial auditor", 
            audience: "executive board", 
            schema: "## [Metric Name]\n**Value:** [Number/Percent]\n**Signal:** [Positive/Neutral/Negative]\n**Context:** [1 sentence]",
            constraints: default_constraints,
            synthesis: "Close with a 3-column table: Metric | Signal | Recommended Adjustment."
        },
        _ => RoleFrame {
            role: "analyst", 
            audience: "expert reader", 
            schema: "## [Item]\n**Summary:** [2-3 sentences]\n**Key Points:** [2-3 bullets]",
            constraints: default_constraints,
            synthesis: "Close with a high-level summary paragraph synthesizing across all items."
        },
    }
}

fn rewrite_verbs(task: &str) -> String {
    let mut optimized_task = task.to_string();
    optimized_task = RE_COMPARE.replace_all(&optimized_task, "map").to_string();
    optimized_task = RE_DISCUSS
        .replace_all(&optimized_task, "enumerate")
        .to_string();
    optimized_task = RE_EXPLORE.replace_all(&optimized_task, "score").to_string();
    optimized_task = RE_EXPLAIN.replace_all(&optimized_task, "table").to_string();
    optimized_task = RE_COVER.replace_all(&optimized_task, "extract").to_string();
    optimized_task
}

pub fn run(
    chunks: &[String],
    task: &str,
    deliverables: &str,
    constraints: &str,
    reproducibility: &str,
    use_case: &str,
    model: &str,
) -> PrincipleResult {
    let start = Instant::now();
    let combined_text = chunks.join("\n\n");

    // Auto-detect use case if not explicitly provided
    let detected = use_case == "auto" || use_case.is_empty();
    let effective_use_case = if detected {
        detect_use_case(&combined_text)
    } else {
        use_case.to_string()
    };

    let frame = get_role_frame(&effective_use_case);
    let optimized_task = rewrite_verbs(task);

    // Final neuro-assembly: Role/Audience -> Task -> Deliverable -> Constraints -> Content
    let mut output_parts = Vec::new();

    if model == "Claude" {
        // Explicitly bind the role prefix for Claude as requested
        output_parts.push(format!("Role: {}", frame.role));
        output_parts.push(format!("Audience: {}", frame.audience));
    } else {
        // For other models, maybe we just include the role without the explicit prefix or use a simpler format
        output_parts.push(format!("Role: {}", frame.role));
    }

    output_parts.push(format!("Task: {}", optimized_task));

    if !deliverables.is_empty() {
        output_parts.push(format!("Deliverable: {}", deliverables));
    }
    if !constraints.is_empty() {
        output_parts.push(format!("Constraints: {}", constraints));
    }
    if !reproducibility.is_empty() {
        output_parts.push(format!("Reproducibility: {}", reproducibility));
    }

    output_parts.push("---".to_string());
    output_parts.push(combined_text);

    let raw_bound = output_parts.join("\n\n");

    // Remove emojis, icons, and non-essential special characters.
    // Keep ASCII, alphanumeric, and common markdown punctuation.
    let cleaned = raw_bound
        .chars()
        .filter(|c| {
            c.is_ascii() || c.is_alphanumeric() || c.is_whitespace() || ".!?,;:-[]()#*".contains(*c)
        })
        .collect::<String>();

    // Compact output: trim lines and remove redundant empty lines.
    let bound = cleaned
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let final_tokens = (bound.split_whitespace().count() as f64 * 1.3).ceil() as usize;

    PrincipleResult {
        text:          bound.clone(),
        chunks:        vec![bound],
        items_removed: 0,
        detail:        format!(
            "Applied CRISP role frame: {} (detected: {}) | Final compact CRISP assembly. Tokens: {}",
            effective_use_case,
            detected,
            final_tokens
        ),
        duration_ms:   start.elapsed().as_millis() as u64,
    }
}
