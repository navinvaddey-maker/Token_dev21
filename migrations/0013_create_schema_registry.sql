-- ============================================================
-- Migration 0013: ScenarioClassifier support tables
-- Safe to re-run (IF NOT EXISTS guards)
-- No modifications to existing tables
-- ============================================================

-- Schema template store
-- Each row = one prompt schema template for a domain+task+expertise combination
CREATE TABLE IF NOT EXISTS schema_index (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    domain        TEXT    NOT NULL,
    task_type     TEXT    NOT NULL,
    expertise     TEXT    NOT NULL CHECK(expertise IN ('beginner','intermediate','expert')),
    intent_class  TEXT    NOT NULL,
    template_json TEXT    NOT NULL,
    version       INTEGER NOT NULL DEFAULT 1,
    created_at    DATETIME         DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(domain, task_type, expertise, intent_class)
);

-- Scenario archetype vector store
-- Each row = one archetype with its pre-embedded L2-normalized vector
-- vector_blob starts empty; populated by seed_archetype_vectors_on_startup()
CREATE TABLE IF NOT EXISTS scenario_index (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    label         TEXT NOT NULL UNIQUE,
    domain        TEXT NOT NULL,
    task_type     TEXT NOT NULL,
    expertise     TEXT NOT NULL,
    intent_class  TEXT NOT NULL,
    vector_blob   BLOB NOT NULL DEFAULT X'',
    example_query TEXT NOT NULL
);

-- Lookup indexes
CREATE INDEX IF NOT EXISTS idx_schema_domain_task
    ON schema_index(domain, task_type);

CREATE INDEX IF NOT EXISTS idx_scenario_domain
    ON scenario_index(domain, intent_class);


-- ============================================================
-- SEED: Scenario Archetypes (7 archetypes covering primary domains)
-- ============================================================

INSERT OR IGNORE INTO scenario_index
    (label, domain, task_type, expertise, intent_class, example_query)
VALUES
    ('real-estate::wealth-building::intermediate',
     'real-estate', 'creation', 'intermediate', 'wealth-building',
     'I want to become rich in real estate guide me'),

    ('real-estate::compliance::intermediate',
     'real-estate', 'analysis', 'intermediate', 'compliance',
     'What licenses do I need to legally sell property in Texas'),

    ('finance::wealth-building::intermediate',
     'finance', 'creation', 'intermediate', 'wealth-building',
     'How do I build a million dollar investment portfolio in 10 years'),

    ('legal::compliance::expert',
     'legal', 'analysis', 'expert', 'compliance',
     'Review this contract for indemnification clauses and regulatory compliance issues'),

    ('tech::debugging::intermediate',
     'tech', 'troubleshooting', 'intermediate', 'debugging',
     'My Rust async function is deadlocking under concurrent load how do I fix it'),

    ('marketing::brand-building::beginner',
     'marketing', 'creation', 'beginner', 'brand-building',
     'How do I build my personal brand on LinkedIn from scratch with no following'),

    ('general::creation::intermediate',
     'general', 'creation', 'intermediate', 'general',
     'Help me plan and execute this project step by step');

-- ============================================================
-- SEED: Schema Templates (matching archetypes above)
-- ============================================================

INSERT OR IGNORE INTO schema_index
    (domain, task_type, expertise, intent_class, template_json)
VALUES
    ('real-estate', 'creation', 'intermediate', 'wealth-building', '{
        "role": {
            "profile": "Experienced Real Estate Agent specializing in licensing compliance, property marketing, client acquisition, and agency operations.",
            "guardrail": "accurate_and_thorough"
        },
        "context_fields": ["target_definition","current_state","wealth_gap","income_engine","capital_allocation","reality_check"],
        "constraints": { "tone": "professional-thorough-structured" },
        "execution_phases": [
            { "name": "Licensing & Pre-Registration", "duration": "1-3 months" },
            { "name": "Business Setup & GTM Strategy", "duration": "2-4 weeks"  },
            { "name": "Client Acquisition & Expansion", "duration": "Ongoing"  }
        ],
        "success_criteria_keys": ["target_defined","baseline_quantified","wealth_gap_calculated","income_engines_ranked","scenarios_modeled","reality_check"],
        "task_instruction_suffix": "Execute the comprehensive Goal-Decomposition and Wealth-Engineering plan with mathematical rigor."
    }'),

    ('legal', 'analysis', 'expert', 'compliance', '{
        "role": {
            "profile": "Senior Legal Counsel specializing in regulatory compliance, contract law, and risk mitigation.",
            "guardrail": "jurisdiction_aware"
        },
        "context_fields": ["jurisdiction","regulatory_framework","compliance_status","risk_matrix","remediation_roadmap"],
        "constraints": { "tone": "precise-exhaustive-structured" },
        "execution_phases": [
            { "name": "Regulatory Mapping",   "duration": "3-5 days"  },
            { "name": "Risk Classification",  "duration": "1 week"    },
            { "name": "Remediation Planning", "duration": "2-3 weeks" }
        ],
        "success_criteria_keys": ["statutes_identified","risk_matrix_complete","remediation_sequenced","escalation_triggers_defined"],
        "task_instruction_suffix": "Map applicable statutes, classify risks by severity, and sequence remediation steps."
    }'),

    ('tech', 'troubleshooting', 'intermediate', 'debugging', '{
        "role": {
            "profile": "Senior Software Engineer specializing in system diagnostics, root cause analysis, and performance optimization.",
            "guardrail": "reproducible_steps_only"
        },
        "context_fields": ["error_description","environment","reproduction_steps","root_cause_hypothesis","fix_options"],
        "constraints": { "tone": "technical-precise-stepwise" },
        "execution_phases": [
            { "name": "Symptom Isolation",   "duration": "30 mins"  },
            { "name": "Root Cause Analysis", "duration": "1-2 hrs"  },
            { "name": "Fix & Verification",  "duration": "Variable" }
        ],
        "success_criteria_keys": ["error_reproduced","root_cause_confirmed","fix_verified","regression_tested"],
        "task_instruction_suffix": "Provide reproducible diagnostic steps, root cause with evidence, and fix options ranked by risk."
    }'),

    ('general', 'creation', 'intermediate', 'general', '{
        "role": {
            "profile": "Expert Assistant. Adapt role to the domain stated in the user query.",
            "guardrail": "accurate_and_thorough"
        },
        "context_fields": ["goal","current_state","constraints","success_criteria"],
        "constraints": { "tone": "professional-structured" },
        "execution_phases": [
            { "name": "Discovery",  "duration": "Variable" },
            { "name": "Execution",  "duration": "Variable" },
            { "name": "Review",     "duration": "Variable" }
        ],
        "success_criteria_keys": ["goal_defined","constraints_listed","steps_sequenced"],
        "task_instruction_suffix": "Structure your response according to the goal, constraints, and success criteria above."
    }');
