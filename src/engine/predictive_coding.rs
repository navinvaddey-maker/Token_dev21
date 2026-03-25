use std::time::Instant;
use super::PrincipleResult;

pub struct RoleFrame {
    pub role:          &'static str,
    pub audience:      &'static str,
    pub output_format: &'static str,
    pub constraints:   &'static str,
}

pub fn get_role_frame(use_case: &str) -> RoleFrame {
    let default_constraints = "- No transitional filler\n- No meta-commentary\n- Strict brevity";
    match use_case {
        "ticket" => RoleFrame { 
            role: "data analyst with expertise in support triage", 
            audience: "engineering team", 
            output_format: "Extract JSON array [{id, priority, category, action}]",
            constraints: default_constraints
        },
        "legal" => RoleFrame { 
            role: "legal auditor", 
            audience: "senior counsel", 
            output_format: "Map bullet list — risk · clause reference · severity",
            constraints: default_constraints
        },
        "resume" => RoleFrame { 
            role: "talent screener", 
            audience: "hiring manager", 
            output_format: "Score scorecard — candidate · fit_score · reasoning",
            constraints: default_constraints
        },
        "code" => RoleFrame { 
            role: "senior software engineer specialising in distributed systems", 
            audience: "software engineers", 
            output_format: "Enumerate markdown — function · purpose · parameters · returns",
            constraints: "- No conversational filler\n- Prioritise precision over prose flourish"
        },
        "research" => RoleFrame { 
            role: "comparative researcher", 
            audience: "academic peers", 
            output_format: "Synthesise structured — finding · evidence · limitation",
            constraints: default_constraints
        },
        "transcript" => RoleFrame { 
            role: "meeting coordinator", 
            audience: "project stakeholders", 
            output_format: "Extract action list — owner · action · deadline",
            constraints: default_constraints
        },
        "financial" => RoleFrame { 
            role: "financial auditor", 
            audience: "executive board", 
            output_format: "Score health report — metric · value · signal",
            constraints: default_constraints
        },
        _ => RoleFrame { 
            role: "analyst", 
            audience: "expert reader", 
            output_format: "Extract structured summary",
            constraints: default_constraints
        },
    }
}

pub fn run(chunks: &[String], _task: &str, use_case: &str) -> PrincipleResult {
    let start  = Instant::now();
    let frame  = get_role_frame(use_case);
    let text   = format!("Role: {}\nAudience: {}\n\n---\n\n{}", frame.role, frame.audience, chunks.join("\n\n"));
    PrincipleResult {
        chunks:        chunks.to_vec(),
        items_removed: 0,
        detail:        format!("Prepended CRISP role frame for: {}", use_case),
        duration_ms:   start.elapsed().as_millis() as u64,
        text,
    }
}
