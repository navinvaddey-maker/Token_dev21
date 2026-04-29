use crate::npae::ory::types::LearnedIntent;
use anyhow::Result;

pub struct OryLearner;

impl OryLearner {
    pub fn learn(raw: &str) -> Result<LearnedIntent> {
        let lower = raw.to_lowercase();
        
        // 1. Extract basic intent
        let core_objective = extract_core_objective(&lower);
        
        // 2. Deep domain detection
        let inferred_domain = detect_deep_domain(&lower);
        
        // 3. Identify novel signals (things the standard engine might miss)
        let novel_signals = identify_novel_signals(&lower);
        
        // 4. Unpack hidden dependencies (Intent Unpacking)
        let hidden_dependencies = unpack_dependencies(&lower, &inferred_domain);
        
        Ok(LearnedIntent {
            raw_prompt: raw.to_string(),
            core_objective,
            inferred_domain,
            novel_signals,
            hidden_dependencies,
            confidence_score: 0.85, // Placeholder for semantic confidence
        })
    }
}

fn extract_core_objective(lower: &str) -> String {
    // Higher-level intent extraction
    if lower.contains("roadmap") || lower.contains("strategy") {
        "Strategic Planning".to_string()
    } else if lower.contains("debug") || lower.contains("fix") || lower.contains("why") {
        "Problem Analysis".to_string()
    } else if lower.contains("build") || lower.contains("create") || lower.contains("implement") {
        "Execution & Construction".to_string()
    } else if lower.contains("explain") || lower.contains("understand") {
        "Conceptual Understanding".to_string()
    } else {
        "Information Retrieval".to_string()
    }
}

fn detect_deep_domain(lower: &str) -> String {
    // This should eventually query a semantic registry or LLM
    // For now, it provides a more granular domain than the standard engine
    if lower.contains("quantum") || lower.contains("physics") {
        "Theoretical Science".to_string()
    } else if lower.contains("marathon") || lower.contains("ultra") {
        "Endurance Athletics".to_string()
    } else if lower.contains("startup") || lower.contains("venture") {
        "High-Growth Business".to_string()
    } else {
        "General Knowledge".to_string()
    }
}

fn identify_novel_signals(lower: &str) -> Vec<String> {
    let mut signals = Vec::new();
    
    // Look for "Meta-Instructions" or unconventional constraints
    if lower.contains("unconventional") || lower.contains("contrarian") {
        signals.push("Non-Standard Methodology".to_string());
    }
    if lower.contains("first principles") || lower.contains("from scratch") {
        signals.push("Foundational Rebuilding".to_string());
    }
    if lower.contains("at scale") || lower.contains("millions") {
        signals.push("High-Scale Constraints".to_string());
    }
    
    signals
}

fn unpack_dependencies(lower: &str, domain: &str) -> Vec<String> {
    let mut deps = Vec::new();
    
    // Domain-specific dependency inference (The "What else do they need?" logic)
    match domain {
        "Endurance Athletics" => {
            deps.push("Nutritional Timing".to_string());
            deps.push("Injury Prevention".to_string());
            deps.push("Tapering Strategy".to_string());
        },
        "High-Growth Business" => {
            deps.push("Unit Economics".to_string());
            deps.push("Customer Acquisition Cost".to_string());
            deps.push("Scaling Bottlenecks".to_string());
        },
        "Theoretical Science" => {
            deps.push("Mathematical Foundations".to_string());
            deps.push("Experimental Verification".to_string());
        },
        _ => {}
    }
    
    deps
}
