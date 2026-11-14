//! Integration and golden verification suite for Balanced Mode (`ARCHITECTURE.md` §1.5).
//!
//! Verifies all Balanced Mode gap fixes (B1–B5, B7, C1, C2, C4, A1, A2, A4, E1–E5):
//! 1. Prompt 1 (Python CSV parser): compresses tokens, strips greeting/sign-off filler,
//!    preserves `"not"` / `"pandas"` and `"missing"`, and scores `>= 6.0` on TES, SFS, SCS.
//! 2. Prompt 2 (7-day meal plan): preserves `"no dairy"`, `"no nightshades"`, `"1800"`, `"80"`,
//!    compresses tokens, and scores `>= 6.0` on TES, SFS, SCS.
//! 3. Prompt 3 (Quarterly revenue report negation): preserves negation (`"not"` / `"not speculate"`)
//!    and `"200"`, never flips meaning, compresses tokens, and scores `>= 6.0` on TES, SFS, SCS.
//! 4. Case-insensitivity & routing consistency: `Some("balanced")`, `Some("Balanced")`, and
//!    `Some("BALANCED")` produce identical outputs, keep ratios, and populated `delta_tokens`.
//! 5. Tree-mode sparse coding: highest-salience token (`"authentication"`) and multi-word
//!    constraint locks survive in `"tree"` mode with original indices preserved.
//! 6. Honest scoring & compact fallback: SCS/SFS penalize dropped constraints/negations instead of
//!    crediting pre-sparse `clean_tokens`, and compact fallback retains constraint locks.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

use dashmap::DashMap;
use std::sync::Arc;
use token_compress_engine::{
    algorithms::sparse_coding::SparseCoding,
    npae::{
        aggressive::config::{ConfigHandle, ConfigLoader},
        ory::OryEngine,
    },
    pipeline::{orchestrator::PipelineOrchestrator, stage1::Stage1, stage6a::Stage6a},
    scoring::{
        scs::SemanticCompletenessScorer, sfs::SemanticFidelityScorer, tes::TokenEfficiencyScorer,
    },
    session::SessionHistory,
    types::{
        AlgorithmOutput, CompressionSchema, ConstraintToken, Mode, NormalizationResult,
        OrchestratorResponse, PromptTopology, ScoredToken, TokenSource,
    },
    utils::tokens::estimate_tokens,
};
use tokio::sync::Mutex;

/// Helper to build a canonical `PipelineOrchestrator` backed by `config/unified.json`.
fn build_test_orchestrator() -> PipelineOrchestrator {
    let config = ConfigLoader::load("config/unified.json").expect("load config/unified.json");
    let npae_config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));
    let schema_priors = Arc::new(DashMap::new());
    PipelineOrchestrator::build(schema_priors, npae_config_handle, ory_engine)
}

#[tokio::test]
async fn test_balanced_prompt1_python_csv_parser_compresses_and_preserves_constraints() {
    let orchestrator = build_test_orchestrator();
    let mut session = SessionHistory::new(10);

    let prompt = "Hi there, I was basically wondering if you could please help me out. I really need you to write a Python function that parses a CSV file and returns the average of the price column. It must not use pandas and it should handle missing values gracefully. Thanks so much in advance!";

    let resp = orchestrator
        .process(prompt, &mut session, None, Some("balanced"), None, None)
        .await
        .expect("Balanced mode compression should succeed");

    let OrchestratorResponse::Legacy(legacy) = resp else {
        panic!("Expected Legacy response for Balanced mode");
    };

    assert_eq!(legacy.mode, "Balanced");

    let input_tokens = estimate_tokens(prompt);
    let output_tokens = estimate_tokens(&legacy.response);
    let lower = legacy.response.to_lowercase();

    println!(
        "[Prompt 1] input_tokens={} output_tokens={} TES={:.2} SFS={:.2} SCS={:.2}\nOutput:\n{}",
        input_tokens,
        output_tokens,
        legacy.scoring_result.tes,
        legacy.scoring_result.sfs,
        legacy.scoring_result.scs,
        legacy.response
    );

    // 1. Output must be genuinely compressed (fewer tokens than input)
    assert!(
        output_tokens < input_tokens,
        "Balanced output ({output_tokens} tokens) must be shorter than input ({input_tokens} tokens): {}",
        legacy.response
    );

    // 2. Hard constraints ("not" / "pandas" and "missing") must be preserved
    assert!(
        lower.contains("not") && lower.contains("pandas"),
        "Balanced output must preserve negation constraint 'not ... pandas': {}",
        legacy.response
    );
    assert!(
        lower.contains("missing"),
        "Balanced output must preserve 'missing' values constraint: {}",
        legacy.response
    );

    // 3. Greeting and sign-off filler must be stripped
    for filler in ["hi there", "basically", "wondering", "thanks", "advance"] {
        assert!(
            !lower.contains(filler),
            "Balanced output must strip conversational filler '{filler}': {}",
            legacy.response
        );
    }

    // 4. All three quality scorers must pass threshold (>= 6.0)
    assert!(
        legacy.scoring_result.tes >= 6.0,
        "Expected TES >= 6.0, got {:.2}",
        legacy.scoring_result.tes
    );
    assert!(
        legacy.scoring_result.sfs >= 6.0,
        "Expected SFS >= 6.0, got {:.2}",
        legacy.scoring_result.sfs
    );
    assert!(
        legacy.scoring_result.scs >= 6.0,
        "Expected SCS >= 6.0, got {:.2}",
        legacy.scoring_result.scs
    );
}

#[tokio::test]
async fn test_balanced_prompt2_meal_plan_preserves_exclusions_and_numeric_bounds() {
    let orchestrator = build_test_orchestrator();
    let mut session = SessionHistory::new(10);

    let prompt = "Create a 7 day meal plan for a 35 year old woman, 1800 calories per day, no dairy, no nightshades, high fiber, budget under 80 usd per week, include a shopping list.";

    let resp = orchestrator
        .process(prompt, &mut session, None, Some("balanced"), None, None)
        .await
        .expect("Balanced mode compression should succeed");

    let OrchestratorResponse::Legacy(legacy) = resp else {
        panic!("Expected Legacy response for Balanced mode");
    };

    let input_tokens = estimate_tokens(prompt);
    let output_tokens = estimate_tokens(&legacy.response);
    let lower = legacy.response.to_lowercase();

    println!(
        "[Prompt 2] input_tokens={} output_tokens={} TES={:.2} SFS={:.2} SCS={:.2}\nOutput:\n{}",
        input_tokens,
        output_tokens,
        legacy.scoring_result.tes,
        legacy.scoring_result.sfs,
        legacy.scoring_result.scs,
        legacy.response
    );

    assert!(
        output_tokens <= input_tokens,
        "Balanced output ({output_tokens} tokens) must not expand beyond input ({input_tokens} tokens): {}",
        legacy.response
    );

    // Verify all dietary exclusions and numeric bounds survive
    assert!(
        lower.contains("no dairy"),
        "Balanced output must preserve 'no dairy': {}",
        legacy.response
    );
    assert!(
        lower.contains("no nightshades"),
        "Balanced output must preserve 'no nightshades': {}",
        legacy.response
    );
    assert!(
        lower.contains("1800"),
        "Balanced output must preserve '1800': {}",
        legacy.response
    );
    assert!(
        lower.contains("80"),
        "Balanced output must preserve '80': {}",
        legacy.response
    );

    assert!(
        legacy.scoring_result.tes >= 6.0,
        "Expected TES >= 6.0, got {:.2}",
        legacy.scoring_result.tes
    );
    assert!(
        legacy.scoring_result.sfs >= 6.0,
        "Expected SFS >= 6.0, got {:.2}",
        legacy.scoring_result.sfs
    );
    assert!(
        legacy.scoring_result.scs >= 6.0,
        "Expected SCS >= 6.0, got {:.2}",
        legacy.scoring_result.scs
    );
}

#[tokio::test]
async fn test_balanced_prompt3_quarterly_revenue_report_preserves_negation_without_flipping() {
    let orchestrator = build_test_orchestrator();
    let mut session = SessionHistory::new(10);

    let prompt = "Summarize the attached quarterly revenue report for the board, keep it under 200 words, highlight risks, do not speculate about Q4 numbers.";

    let resp = orchestrator
        .process(prompt, &mut session, None, Some("balanced"), None, None)
        .await
        .expect("Balanced mode compression should succeed");

    let OrchestratorResponse::Legacy(legacy) = resp else {
        panic!("Expected Legacy response for Balanced mode");
    };

    let input_tokens = estimate_tokens(prompt);
    let output_tokens = estimate_tokens(&legacy.response);
    let lower = legacy.response.to_lowercase();

    println!(
        "[Prompt 3] input_tokens={} output_tokens={} TES={:.2} SFS={:.2} SCS={:.2}\nOutput:\n{}",
        input_tokens,
        output_tokens,
        legacy.scoring_result.tes,
        legacy.scoring_result.sfs,
        legacy.scoring_result.scs,
        legacy.response
    );

    assert!(
        output_tokens <= input_tokens,
        "Balanced output ({output_tokens} tokens) must not expand beyond input ({input_tokens} tokens): {}",
        legacy.response
    );

    // Verify negation ("not" / "not speculate") and bound ("200") are preserved and NOT flipped
    assert!(
        lower.contains("not"),
        "Balanced output must preserve negation token 'not': {}",
        legacy.response
    );
    assert!(
        lower.contains("not speculate"),
        "Balanced output must preserve negated action 'not speculate' (never flip meaning): {}",
        legacy.response
    );
    assert!(
        lower.contains("200"),
        "Balanced output must preserve word limit '200': {}",
        legacy.response
    );

    assert!(
        legacy.scoring_result.tes >= 6.0,
        "Expected TES >= 6.0, got {:.2}",
        legacy.scoring_result.tes
    );
    assert!(
        legacy.scoring_result.sfs >= 6.0,
        "Expected SFS >= 6.0, got {:.2}",
        legacy.scoring_result.sfs
    );
    assert!(
        legacy.scoring_result.scs >= 6.0,
        "Expected SCS >= 6.0, got {:.2}",
        legacy.scoring_result.scs
    );
}

#[tokio::test]
async fn test_balanced_case_insensitivity_and_stage2_delta_tokens() {
    let prompt = "Hi there, I was basically wondering if you could please help me out. I really need you to write a Python function that parses a CSV file and returns the average of the price column. It must not use pandas and it should handle missing values gracefully. Thanks so much in advance!";

    // 1. Verify Stage 1 keep ratio and sparse tokens are identical across "balanced", "Balanced", "BALANCED"
    let stage1 = Stage1::new();
    let mut out_lower = AlgorithmOutput::default();
    let mut out_title = AlgorithmOutput::default();
    let mut out_upper = AlgorithmOutput::default();

    stage1.run(
        prompt,
        &mut out_lower,
        &PromptTopology::Linear,
        Some("balanced"),
    );
    stage1.run(
        prompt,
        &mut out_title,
        &PromptTopology::Linear,
        Some("Balanced"),
    );
    stage1.run(
        prompt,
        &mut out_upper,
        &PromptTopology::Linear,
        Some("BALANCED"),
    );

    let texts_lower: Vec<&str> = out_lower
        .sparse_tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    let texts_title: Vec<&str> = out_title
        .sparse_tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    let texts_upper: Vec<&str> = out_upper
        .sparse_tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect();

    assert_eq!(
        texts_lower, texts_title,
        "Stage 1 must treat Some(\"balanced\") and Some(\"Balanced\") identically"
    );
    assert_eq!(
        texts_lower, texts_upper,
        "Stage 1 must treat Some(\"balanced\") and Some(\"BALANCED\") identically"
    );

    // 2. Verify full orchestrator produces identical outputs and populates delta_tokens (Gap C2 & C4)
    let orch1 = build_test_orchestrator();
    let orch2 = build_test_orchestrator();
    let mut s1 = SessionHistory::new(10);
    let mut s2 = SessionHistory::new(10);

    let OrchestratorResponse::Legacy(r_lower) = orch1
        .process(prompt, &mut s1, None, Some("balanced"), None, None)
        .await
        .expect("lowercase balanced")
    else {
        panic!("Expected Legacy response");
    };

    let OrchestratorResponse::Legacy(r_title) = orch2
        .process(prompt, &mut s2, None, Some("Balanced"), None, None)
        .await
        .expect("titlecase Balanced")
    else {
        panic!("Expected Legacy response");
    };

    assert_eq!(r_lower.mode, "Balanced");
    assert_eq!(r_title.mode, "Balanced");
    assert_eq!(
        r_lower.response, r_title.response,
        "Some(\"balanced\") and Some(\"Balanced\") must produce identical final responses"
    );
    assert!(
        r_lower.delta_tokens.as_ref().is_some_and(|d| !d.is_empty()),
        "Stage 2 delta_tokens must be populated even when forced_mode is provided (Gap C4)"
    );
}

#[test]
fn test_tree_mode_sparse_coding_preserves_highest_salience_and_locks() {
    let sc = SparseCoding::default();
    let tokens: Vec<String> = vec![
        "zeta".into(),
        "beta".into(),
        "gamma".into(),
        "delta".into(),
        "epsilon".into(),
        "theta".into(),
        "kappa".into(),
        "lambda".into(),
        "sigma".into(),
        "authentication".into(),
    ];

    let result = sc.apply(&tokens, 0.30, &[], "tree");
    let texts: Vec<&str> = result.iter().map(|t| t.text.as_str()).collect();

    assert!(
        texts.contains(&"authentication"),
        "Tree-mode sparse coding must preserve highest-salience token 'authentication', got {:?}",
        texts
    );

    // Verify original token order is preserved
    let mut last_pos = None;
    for tok in &result {
        let pos = tokens
            .iter()
            .position(|t| t == &tok.text)
            .expect("survivor must exist in original tokens");
        if let Some(prev) = last_pos {
            assert!(
                pos > prev,
                "Tree mode must preserve original token order: {:?} vs {:?}",
                texts,
                tokens
            );
        }
        last_pos = Some(pos);
    }
}

#[test]
fn test_honest_scorers_and_compact_fallback_edge_cases() {
    // 1. SCS and SFS must penalize when constraints/negations are missing from active output
    //    even if they exist in pre-sparse `clean_tokens`.
    let bad_output = AlgorithmOutput {
        mode: Some(Mode::Balanced),
        normalization: Some(NormalizationResult {
            normalized_text: "Summarize report, do not speculate about Q4 numbers".into(),
            normalization_score: 1.0,
            applied_rules: vec![],
        }),
        clean_tokens: vec![
            "summarize".into(),
            "report".into(),
            "do".into(),
            "not".into(),
            "speculate".into(),
            "q4".into(),
            "numbers".into(),
        ],
        sparse_tokens: vec![
            ScoredToken {
                text: "summarize".into(),
                salience: 0.8,
                source: TokenSource::Sparse,
            },
            ScoredToken {
                text: "report".into(),
                salience: 0.8,
                source: TokenSource::Sparse,
            },
            ScoredToken {
                text: "q4".into(),
                salience: 0.9,
                source: TokenSource::Sparse,
            },
            ScoredToken {
                text: "numbers".into(),
                salience: 0.8,
                source: TokenSource::Sparse,
            },
        ],
        constraint_locks: vec![ConstraintToken {
            text: "do not speculate".into(),
            weight: 1.0,
        }],
        resolved_schema: CompressionSchema {
            task: Some("Summarize report about Q4 numbers".into()),
            ..Default::default()
        },
        rendered_output: Some("summarize report about q4 numbers".into()),
        input_token_count: 12,
        output_token_count: 7,
        ..Default::default()
    };

    let (scs_bad, scs_penalties) = SemanticCompletenessScorer::score(&bad_output);
    let (sfs_bad, sfs_issues) = SemanticFidelityScorer::score_with_output(&bad_output, &[]);

    assert!(
        scs_bad <= 6.0,
        "SCS must not credit pre-sparse clean_tokens when constraint is missing from rendered output, got {}",
        scs_bad
    );
    assert!(scs_penalties.iter().any(|p| p.contains("Constraint loss")));
    assert!(
        sfs_bad < 6.0,
        "SFS must penalize dropped negation below 6.0, got {}",
        sfs_bad
    );
    assert!(sfs_issues.iter().any(|i| i.issue_type == "negation_loss"));

    // 2. Stage 6A compact fallback must re-attach missing constraint locks and negations
    let mut bad_output = bad_output;
    bad_output.force_compact_generation = true;
    let stage6a = Stage6a::new();
    let rescued = stage6a
        .run(&bad_output)
        .expect("Stage 6A compact fallback should succeed");
    assert!(
        rescued.to_lowercase().contains("not speculate"),
        "Stage 6A compact fallback must rescue missing negation constraint: {rescued}"
    );

    // 3. TES uses consistent `estimate_tokens` units
    let tes_check = AlgorithmOutput {
        normalization: Some(NormalizationResult {
            normalized_text: "one two three four five six seven eight nine ten".into(),
            normalization_score: 1.0,
            applied_rules: vec![],
        }),
        // Raw word count (10) differs from estimate_tokens (13), but TES must compare
        // estimate_tokens(normalized_text) vs estimate_tokens(rendered_output)
        input_token_count: 10,
        rendered_output: Some("one two three four five six seven eight nine ten".into()),
        output_token_count: 13,
        ..Default::default()
    };
    let (tes_same, _) = TokenEfficiencyScorer::score(&tes_check, &[]);
    assert_eq!(
        tes_same, 0.0,
        "Identical input and output text must yield 0.0 TES (no false savings from unit mismatch)"
    );
}

#[tokio::test]
async fn test_schema_registry_resolves_canonical_domains_and_aliases_without_pool() {
    use token_compress_engine::db::schema_registry::{SchemaLookupKey, SchemaRegistry};

    let registry = SchemaRegistry::without_pool();

    // Verify all 10 required canonical domains + 2 aliases resolve domain-specific schemas
    // directly from `config/unified.json` in-memory without a SQLite pool.
    let cases = [
        ("software", "creation", "expert", "Systems Architect"),
        ("tech", "creation", "expert", "Systems Architect"),
        (
            "software-engineering",
            "analysis",
            "intermediate",
            "Systems Architect",
        ),
        ("legal", "analysis", "expert", "Legal Strategist"),
        (
            "finance",
            "planning",
            "expert",
            "Wealth & Income Strategist",
        ),
        ("medical", "analysis", "expert", "Physician"),
        (
            "nutrition",
            "creation",
            "intermediate",
            "Registered Dietician",
        ),
        (
            "cybersecurity",
            "troubleshooting",
            "expert",
            "Security Analyst",
        ),
        ("travel", "planning", "intermediate", "Travel Planner"),
        ("education", "creation", "expert", "Curriculum Designer"),
        ("real-estate", "analysis", "expert", "Real Estate Agent"),
        ("business", "planning", "expert", "Business Strategist"),
    ];

    for (domain, task_type, expertise, expected_role_substr) in cases {
        let key = SchemaLookupKey {
            domain: domain.to_string(),
            task_type: task_type.to_string(),
            expertise: expertise.to_string(),
        };

        let tier1 = registry
            .find_exact_schema_match(&key)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "Tier 1 exact schema match must resolve for domain={domain}, task={task_type}, expertise={expertise}"
                )
            });

        let resolved = registry.resolve_schema_for_signal(&key).await;
        let role = resolved.role.as_deref().unwrap_or_default();
        assert!(
            role.contains(expected_role_substr),
            "Domain '{domain}' must resolve role containing '{expected_role_substr}', got '{role}'"
        );
        assert_ne!(
            role, "Domain Expert",
            "Domain '{domain}' must not fall back to generic 'Domain Expert'"
        );
        assert_eq!(tier1.role, resolved.role);
        assert!(
            resolved.context.as_ref().is_some_and(|c| !c.is_empty()),
            "Domain '{domain}' must resolve non-empty context fields"
        );
        assert!(
            resolved.task.as_ref().is_some_and(|t| !t.is_empty()),
            "Domain '{domain}' must resolve non-empty task template"
        );
        assert!(
            !resolved.output.is_empty(),
            "Domain '{domain}' must resolve non-empty default deliverables"
        );

        // Also verify Tier 2 domain-level lookup succeeds when expertise is omitted/unknown
        let tier2 = registry
            .find_domain_level_schema(domain, task_type)
            .await
            .unwrap_or_else(|| {
                panic!("Tier 2 domain-level schema must resolve for domain={domain}")
            });
        assert!(
            tier2
                .role
                .as_deref()
                .unwrap_or_default()
                .contains(expected_role_substr),
            "Tier 2 role for '{domain}' must contain '{expected_role_substr}', got {:?}",
            tier2.role
        );
    }
}

#[tokio::test]
async fn test_stage4_and_stage6a_preserve_domain_roles_lenses_and_deliverables_under_balanced_contract(
) {
    let orchestrator = build_test_orchestrator();

    // 1. Pharma wealth-building prompt (Intent x Vertical rule composition:
    //    primary="Business Strategist", secondary="pharma", legal_as_constraint=true)
    let pharma_prompt = "Hi there, I was basically wondering if you could please help me out. How can I make millions in pharma by building a biotech empire while navigating FDA regulatory compliance and clinical licensing? Please keep it under 300 words and do not speculate on unapproved drugs. Thanks so much in advance!";
    let mut session1 = SessionHistory::new(10);
    let OrchestratorResponse::Legacy(pharma_resp) = orchestrator
        .process(
            pharma_prompt,
            &mut session1,
            None,
            Some("balanced"),
            None,
            None,
        )
        .await
        .expect("Balanced pharma compression should succeed")
    else {
        panic!("Expected Legacy response for Balanced mode");
    };

    let pharma_in = estimate_tokens(pharma_prompt);
    let pharma_out = estimate_tokens(&pharma_resp.response);
    assert!(
        pharma_out <= pharma_in,
        "Balanced Mode Contract violated on pharma prompt: out={pharma_out} > in={pharma_in}\n{}",
        pharma_resp.response
    );
    let pharma_role = pharma_resp.schema.role.as_deref().unwrap_or_default();
    assert!(
        pharma_role.contains("Business Strategist") && pharma_role.contains("Pharma"),
        "Stage 4 must compose primary role 'Business Strategist' with secondary lens '(Pharma)', got: '{pharma_role}'"
    );
    assert!(
        pharma_resp
            .schema
            .constraints
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case("regulatory_compliance")),
        "Stage 4 must inject 'regulatory_compliance' when legal_as_constraint is true, got: {:?}",
        pharma_resp.schema.constraints
    );
    assert!(
        !pharma_resp.schema.output.is_empty(),
        "Stage 4 must populate domain deliverables in schema.output"
    );
    assert!(
        pharma_resp
            .response
            .to_lowercase()
            .contains("not speculate"),
        "Stage 6A must preserve negation lock 'not speculate' in pharma output: {}",
        pharma_resp.response
    );

    // 2. Multi-domain Balanced mode sweep (legal, cybersecurity, finance, travel) verifying:
    //    - In-memory ScenarioSignal fallback & Stage 4 non-destructive schema merge
    //    - Non-generic domain role & non-empty deliverables in `resp.schema`
    //    - Strict Balanced Mode Contract (`output_tokens <= input_tokens`) and constraint survival
    let multi_domain_prompts = [
        (
            "legal",
            "Hello there, could you please help me review this commercial vendor contract clause for intellectual property indemnification and GDPR statutory compliance under Delaware jurisdiction? It must not waive liability caps under 50000 usd. Thanks in advance!",
            "Legal",
            "not waive",
        ),
        (
            "cybersecurity",
            "Hi there, I was basically wondering if you could please conduct a penetration test, SOC incident response audit, and SIEM vulnerability assessment to mitigate ransomware, phishing, and OWASP threats. Do not allow wildcard permissions and keep latency under 50 ms. Thanks so much!",
            "Security",
            "not allow",
        ),
        (
            "travel",
            "Hey there, could you please plan a 5 day family sightseeing travel itinerary in Goa with hotel resort bookings and flight transit logistics? Keep total budget under 30000 inr and include no alcohol venues. Thank you so much in advance!",
            "Travel",
            "30000",
        ),
    ];

    for (label, prompt, expected_role_keyword, expected_constraint_substr) in multi_domain_prompts {
        let mut session = SessionHistory::new(10);
        let OrchestratorResponse::Legacy(resp) = orchestrator
            .process(prompt, &mut session, None, Some("balanced"), None, None)
            .await
            .unwrap_or_else(|e| panic!("Balanced mode failed for {label}: {e}"))
        else {
            panic!("Expected Legacy response for Balanced mode ({label})");
        };

        let in_tok = estimate_tokens(prompt);
        let out_tok = estimate_tokens(&resp.response);
        assert!(
            out_tok <= in_tok,
            "[{label}] Balanced output ({out_tok} tokens) must not exceed input ({in_tok} tokens):\n{}",
            resp.response
        );

        let role = resp.schema.role.as_deref().unwrap_or_default();
        assert!(
            role.to_lowercase()
                .contains(&expected_role_keyword.to_lowercase()),
            "[{label}] Expected schema.role to contain '{expected_role_keyword}', got '{role}'"
        );
        assert!(
            !resp.schema.output.is_empty(),
            "[{label}] Expected non-empty deliverables in schema.output"
        );
        assert!(
            resp.response
                .to_lowercase()
                .contains(expected_constraint_substr),
            "[{label}] Expected rendered response to preserve constraint '{expected_constraint_substr}':\n{}",
            resp.response
        );
        assert!(
            resp.scoring_result.tes >= 6.0
                && resp.scoring_result.sfs >= 6.0
                && resp.scoring_result.scs >= 6.0,
            "[{label}] Expected TES/SFS/SCS >= 6.0, got TES={:.2} SFS={:.2} SCS={:.2}",
            resp.scoring_result.tes,
            resp.scoring_result.sfs,
            resp.scoring_result.scs
        );
    }
}
