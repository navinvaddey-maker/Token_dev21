use token_compress_engine::npae::aggressive::{
    config::{ConfigHandle, ConfigLoader},
    resolver::{PromptResolver, StructurerRouter},
    structurer::HttpStructurer,
    intent::{IntentProfile, IntentClass, KnowledgeLevel},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("--- AI/ML Domain & Focus Extraction Verification ---");

    // 1. Load Config
    let config_path = "config/unified.json";
    let config = ConfigLoader::load(config_path)?;
    let config_handle = Arc::new(ConfigHandle::new(config));

    // 2. Initialize Resolver
    let resolver = PromptResolver::new(config_handle.clone());
    let mut router = StructurerRouter::new(resolver);

    // 3. Create a Mock Profile for AI/ML
    let profile = IntentProfile {
        primary_intent: IntentClass::Explain,
        domain: "ai-ml".into(),
        user_knowledge: KnowledgeLevel::Intermediate,
        temporal_scope: "immediate".into(),
        output_preference: "markdown".into(),
        confidence: 0.9,
        // Added fields from recent updates
        has_timeline: false,
        has_phases: false,
        has_validation: false,
        detected_geography: None,
        detected_risk: None,
        detected_biz_type: None,
        detected_team: None,
    };

    // 4. Test AI Alignment and AGI Focus
    println!("\n[Test 1] AI Systems Educator for alignment and agi...");
    let http_source = HttpStructurer {
        route: "/resolve".into(),
        remote_addr: "127.0.0.1".into(),
        body: "I am looking for an AI Systems Educator to explain general intelligence, alignment, and real-world implications".into(),
    };

    let result = router.dispatch(&http_source, &profile)?;
    println!("Role: {}", result.role);
    
    if result.role.contains("focus: alignment") || result.role.contains("focus: general intelligence") {
        println!("✅ Focus extraction successful!");
    } else {
        println!("❌ Focus extraction failed.");
    }

    // 5. Test LLM Engineer with transformer focus
    let profile_llm = IntentProfile {
        primary_intent: IntentClass::Build,
        domain: "ai-ml".into(),
        user_knowledge: KnowledgeLevel::Expert,
        temporal_scope: "immediate".into(),
        output_preference: "markdown".into(),
        confidence: 0.9,
        has_timeline: false,
        has_phases: false,
        has_validation: false,
        detected_geography: None,
        detected_risk: None,
        detected_biz_type: None,
        detected_team: None,
    };

    println!("\n[Test 2] LLM Engineer with transformer focus...");
    let http_source_llm = HttpStructurer {
        route: "/resolve".into(),
        remote_addr: "127.0.0.1".into(),
        body: "Implement a transformer based LLM with rlhf".into(),
    };

    let result_llm = router.dispatch(&http_source_llm, &profile_llm)?;
    println!("Role: {}", result_llm.role);
    
    if result_llm.role.contains("focus: transformer") || result_llm.role.contains("focus: rlhf") {
        println!("✅ Focus extraction successful!");
    } else {
        println!("❌ Focus extraction failed.");
    }

    println!("\nVerification Complete!");
    Ok(())
}
