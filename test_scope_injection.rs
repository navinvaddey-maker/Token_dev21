extern crate token_compress_engine;

use token_compress_engine::{
    algorithms::schema_filling::{DeterminismScopeInjector, SchemaFilling},
    types::{AlgorithmOutput, WmSlot},
};

fn make_slot(content: &str, salience: f32) -> WmSlot {
    WmSlot {
        content: content.into(),
        salience,
        source: token_compress_engine::types::SlotSource::Delta,
    }
}

fn main() {
    // Test gentle mode (should not have scope injections)
    let sf = SchemaFilling::new();
    let wm_slots = vec![
        make_slot("build authentication system", 0.9),
        make_slot("user login", 0.8),
        make_slot("secure access", 0.7),
    ];

    let mut output = AlgorithmOutput::default();
    let result = sf.fill(&wm_slots, 1, &[], &[], Some(&mut output));

    println!("Gentle mode:");
    println!("  Task: {:?}", result.task);
    println!("  Deliverable: {:?}", result.deliverable);
    println!("  Scope injections: {:?}", output.scope_injections);
    println!(
        "  Scope injections length: {}",
        output.scope_injections.len()
    );

    // Test aggressive mode (should have scope injections)
    let wm_slots_aggressive = vec![make_slot("OAuth2", 0.9), make_slot("PKCE", 0.8)];
    let delta_tokens = vec!["multi-tenant".to_string(), "HSM".to_string()];
    let clusters = vec![vec!["auth".to_string(), "security".to_string()]];

    let mut output2 = AlgorithmOutput::default();
    let result2 = sf.fill(
        &wm_slots_aggressive,
        3,
        &delta_tokens,
        &clusters,
        Some(&mut output2),
    );

    println!("\nAggressive mode:");
    println!("  Task: {:?}", result2.task);
    println!("  Deliverable: {:?}", result2.deliverable);
    println!("  Scope injections: {:?}", output2.scope_injections);
    println!(
        "  Scope injections length: {}",
        output2.scope_injections.len()
    );

    // Verify that aggressive mode has scope injections
    if !output2.scope_injections.is_empty() {
        println!("\n✓ SUCCESS: Scope injections were generated in aggressive mode");
        for (i, injection) in output2.scope_injections.iter().enumerate() {
            println!("  {}: {}", i + 1, injection);
        }
    } else {
        println!("\n✗ FAILURE: No scope injections were generated in aggressive mode");
    }

    // Test the DeterminismScopeInjector directly
    println!("\nDirect test of DeterminismScopeInjector:");
    let injections = DeterminismScopeInjector::inject(
        &Some("OAuth2 authentication system".to_string()),
        &Some("Secure API access".to_string()),
        &vec![
            "user management".to_string(),
            "role based access".to_string(),
        ],
    );
    println!("  Injections: {:?}", injections);
    if !injections.is_empty() {
        println!("  ✓ SUCCESS: Direct injector working");
    } else {
        println!("  ✗ FAILURE: Direct injector not working");
    }
}
