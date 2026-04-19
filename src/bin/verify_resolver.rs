use token_compress_engine::npae::aggressive::{
    config::{ConfigHandle, ConfigLoader},
    resolver::{PromptResolver, StructurerRouter},
    structurer::{HttpStructurer, CliStructurer},
    intent::{IntentProfile, IntentClass, KnowledgeLevel},
    metrics::MetricsRegistry,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("--- PromptResolver System Verification ---");

    // 1. Load Config
    let config_path = "config/unified.json";
    let config = ConfigLoader::load(config_path)?;
    let config_handle = Arc::new(ConfigHandle::new(config));

    // 2. Initialize Resolver
    let resolver = PromptResolver::new(config_handle.clone());
    let mut router = StructurerRouter::new(resolver);

    // 3. Create a Mock Profile
    let profile = IntentProfile {
        primary_intent: IntentClass::Explain,
        domain: "nutrition".into(),
        user_knowledge: KnowledgeLevel::Intermediate,
        temporal_scope: "immediate".into(),
        output_preference: "markdown".into(),
        confidence: 0.9,
    };

    // 4. Case 1: High Confidence Match (Nutrition)
    println!("\n[Test 1] Testing high-confidence nutrition match...");
    let http_source = HttpStructurer {
        route: "/resolve".into(),
        remote_addr: "127.0.0.1".into(),
        body: "I need to avoid gluten and start high carb loading for a race".into(),
    };

    let result = router.dispatch(&http_source, &profile)?;
    println!("Status: {}", result.status);
    println!("Confidence: {:.2}", result.confidence);
    println!("Role: {}", result.role);
    println!("Inclusions: {:?}", result.inclusions);
    println!("Forbidden: {:?}", result.forbidden);

    // 5. Case 2: Untrusted Source / Low Confidence
    println!("\n[Test 2] Testing untrusted source with unknown terms...");
    let prompt_text = "how to make a mystery compound with fda approval";
    // Using a manual call to resolver to simulate complex dispatch
    let res = config_handle.read();
    let resolver_untrusted = PromptResolver::new(config_handle.clone());
    
    // We'll simulate a low confidence by providing terms that don't match well or are unknown
    let result_low = resolver_untrusted.resolve(prompt_text, &profile, "cli");
    println!("Status: {}", result_low.status);
    println!("Confidence: {:.2}", result_low.confidence);

    // 6. Metrics Check
    println!("\n[Test 3] Checking Metrics...");
    let metrics = MetricsRegistry::gather();
    println!("Metrics sample:\n{}", metrics.lines().take(10).collect::<Vec<_>>().join("\n"));

    println!("\nVerification Complete!");
    Ok(())
}
