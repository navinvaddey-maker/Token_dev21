use crate::types::{
    AlgorithmOutput, FieldContentType, FieldValidationIssue, NormalizationResult, TextCorrection,
};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

lazy_static! {
    /// Domain vocabulary mapping common misspellings to correct terms
    static ref DOMAIN_VOCAB: HashMap<String, String> = {
        let mut m = HashMap::new();
        m.insert("taslk".to_string(), "task".to_string());
        m.insert("deliverrable".to_string(), "deliverable".to_string());
        m.insert("contex".to_string(), "context".to_string());
        m.insert("objecive".to_string(), "objective".to_string());
        m.insert("requriement".to_string(), "requirement".to_string());
        m.insert("specifcation".to_string(), "specification".to_string());
        m.insert("parametr".to_string(), "parameter".to_string());
        m.insert("configuratn".to_string(), "configuration".to_string());
        m.insert("initalize".to_string(), "initialize".to_string());
        m.insert("inital".to_string(), "initial".to_string());
        m
    };

    /// Field contracts defining expected content types for known fields
    static ref FIELD_CONTRACTS: HashMap<String, FieldContentType> = {
        let mut m = HashMap::new();
        m.insert("task".to_string(), FieldContentType::Text);
        m.insert("deliverable".to_string(), FieldContentType::Text);
        m.insert("context".to_string(), FieldContentType::Text);
        m.insert("objective".to_string(), FieldContentType::Text);
        m.insert("requirement".to_string(), FieldContentType::Text);
        m.insert("specification".to_string(), FieldContentType::Text);
        m.insert("parameter".to_string(), FieldContentType::Text);
        m.insert("configuration".to_string(), FieldContentType::Text);
        m.insert("priority".to_string(), FieldContentType::Number);
        m.insert("count".to_string(), FieldContentType::Number);
        m.insert("duration".to_string(), FieldContentType::Number);
        m.insert("deadline".to_string(), FieldContentType::Date);
        m.insert("date".to_string(), FieldContentType::Date);
        m.insert("id".to_string(), FieldContentType::Identifier);
        m.insert("identifier".to_string(), FieldContentType::Identifier);
        m.insert("code".to_string(), FieldContentType::Code);
        m.insert("script".to_string(), FieldContentType::Code);
        m
    };
}

pub struct NormalizationPrePass;

impl NormalizationPrePass {
    /// Run normalization pre-pass on raw prompt
    /// Returns normalized text and updates AlgorithmOutput with corrections and issues
    pub fn run(&self, raw_prompt: &str, out: &mut AlgorithmOutput) -> String {
        let mut normalized = raw_prompt.to_string();
        let mut applied_rules = Vec::new();
        let mut corrections = Vec::new();
        let mut field_issues = Vec::new();

        // 1. Typo correction using domain vocabulary
        for (misspelled, correct) in DOMAIN_VOCAB.iter() {
            if normalized.contains(misspelled) {
                normalized = normalized.replace(misspelled, correct);
                applied_rules.push(format!("typo_fix:{}:{}", misspelled, correct));
                corrections.push(TextCorrection {
                    original: misspelled.clone(),
                    corrected: correct.clone(),
                    confidence: 0.95,
                    correction_type: "typo".to_string(),
                });
            }
        }

        // 2. Field mismatch detection
        // Simple field extraction: look for "field: value" patterns
        let field_re = Regex::new(r#"(?m)^\s*(\w+)\s*:\s*(.+?)\s*$"#).unwrap();
        for cap in field_re.captures_iter(&normalized) {
            let field_name = cap.get(1).unwrap().as_str().to_lowercase();
            let value = cap.get(2).unwrap().as_str().trim();

            if let Some(expected_type) = FIELD_CONTRACTS.get(&field_name) {
                let actual_type = Self::infer_field_type(value);
                if actual_type != *expected_type && actual_type != FieldContentType::Unknown {
                    field_issues.push(FieldValidationIssue {
                        field_name: field_name.clone(),
                        issue_type: "type_mismatch".to_string(),
                        description: format!(
                            "Field '{}' expected {:?} but found {:?}",
                            field_name, expected_type, actual_type
                        ),
                        severity: "warning".to_string(),
                    });
                }
            }
        }

        // Update AlgorithmOutput
        out.normalization = Some(NormalizationResult {
            normalized_text: normalized.clone(),
            normalization_score: Self::calculate_normalization_score(&raw_prompt, &normalized),
            applied_rules,
        });
        out.field_issues.extend(field_issues);

        normalized
    }

    /// Infer semantic type of field value
    fn infer_field_type(value: &str) -> FieldContentType {
        // Check for number (integer)
        if value.parse::<i64>().is_ok() {
            return FieldContentType::Number;
        }
        // Check for number (float)
        if value.parse::<f64>().is_ok() {
            return FieldContentType::Number;
        }

        // Check for date (simple YYYY-MM-DD)
        let date_re = Regex::new(r#"^\d{4}-\d{2}-\d{2}$"#).unwrap();
        if date_re.is_match(value) {
            return FieldContentType::Date;
        }

        // Check for identifier (alphanumeric, underscores, hyphens)
        let id_re = Regex::new(r#"^[a-zA-Z0-9_-]+$"#).unwrap();
        if id_re.is_match(value) && !value.contains(' ') {
            return FieldContentType::Identifier;
        }

        // Check for code (contains typical code symbols)
        let code_indicators = vec!["(", ")", "{", "}", "[", "]", ";", "=", "=>", "->", "::"];
        if code_indicators.iter().any(|&ind| value.contains(ind)) {
            return FieldContentType::Code;
        }

        // Default to text
        FieldContentType::Text
    }

    /// Calculate normalization score based on changes made using Levenshtein distance
    fn calculate_normalization_score(original: &str, normalized: &str) -> f32 {
        if original == normalized {
            return 1.0;
        }

        let distance = Self::levenshtein_distance(original, normalized);
        let max_len = original.len().max(normalized.len());

        if max_len == 0 {
            return 1.0;
        }

        // Score is 1.0 minus normalized edit distance
        (1.0 - (distance as f32 / max_len as f32)).clamp(0.0, 1.0)
    }

    /// Calculate Levenshtein distance between two strings
    fn levenshtein_distance(s1: &str, s2: &str) -> usize {
        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();
        let len1 = chars1.len();
        let len2 = chars2.len();

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }

        matrix[len1][len2]
    }
}

impl Default for NormalizationPrePass {
    fn default() -> Self {
        Self::new()
    }
}

impl NormalizationPrePass {
    pub fn new() -> Self {
        Self
    }
}
