use crate::types::FieldValidationIssue;

/// Validates field types and constraints for task, deliverable, and context fields.
pub struct FieldTypeValidator;

impl FieldTypeValidator {
    /// Creates a new FieldTypeValidator.
    pub fn new() -> Self {
        FieldTypeValidator
    }

    /// Validates the resolved fields and returns any validation issues found.
    ///
    /// # Arguments
    /// * `task` - The resolved task field
    /// * `deliverable` - The resolved deliverable field
    /// * `constraints` - The resolved constraints field
    /// * `context` - The resolved context fields
    ///
    /// # Returns
    /// A vector of field validation issues found during validation
    pub fn validate(
        &self,
        schema: &crate::types::CompressionSchema,
    ) -> Vec<FieldValidationIssue> {
        let mut issues = Vec::new();

        issues.extend(self.check_task(&schema.task));

        if let Some(ctx) = &schema.context {
            let context_issues = self.check_context(ctx, 0);
            issues.extend(context_issues);
        }

        issues
    }

    /// Validates the task field for type and content issues.
    fn check_task(&self, task: &Option<String>) -> Vec<FieldValidationIssue> {
        let mut issues = Vec::new();

        match task {
            Some(t) => {
                if t.trim().is_empty() {
                    issues.push(FieldValidationIssue {
                        field_name: "task".to_string(),
                        issue_type: "empty".to_string(),
                        description: "Task field is empty or contains only whitespace".to_string(),
                        severity: "error".to_string(),
                    });
                } else if !Self::is_valid_text_content(t) {
                    issues.push(FieldValidationIssue {
                        field_name: "task".to_string(),
                        issue_type: "invalid_content".to_string(),
                        description: "Task field contains invalid characters or format".to_string(),
                        severity: "warning".to_string(),
                    });
                }
            }
            None => {
                issues.push(FieldValidationIssue {
                    field_name: "task".to_string(),
                    issue_type: "missing".to_string(),
                    description: "Task field is missing".to_string(),
                    severity: "error".to_string(),
                });
            }
        }

        issues
    }


    /// Validates a single context field for type and content issues.
    fn check_context(&self, context: &String, index: usize) -> Vec<FieldValidationIssue> {
        let mut issues = Vec::new();

        if context.trim().is_empty() {
            issues.push(FieldValidationIssue {
                field_name: format!("context[{}]", index),
                issue_type: "empty".to_string(),
                description: format!(
                    "Context field at index {} is empty or contains only whitespace",
                    index
                ),
                severity: "error".to_string(),
            });
        } else if !Self::is_valid_text_content(context) {
            issues.push(FieldValidationIssue {
                field_name: format!("context[{}]", index),
                issue_type: "invalid_content".to_string(),
                description: format!(
                    "Context field at index {} contains invalid characters or format",
                    index
                ),
                severity: "warning".to_string(),
            });
        }

        issues
    }

    /// Helper function to check if text content is valid (basic validation).
    /// In a real implementation, this might check for specific allowed characters, length, etc.
    fn is_valid_text_content(text: &str) -> bool {
        // Basic validation: non-empty after trim and reasonable length
        let trimmed = text.trim();
        !trimmed.is_empty() && trimmed.chars().count() <= 500
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_new() {
        let validator = FieldTypeValidator::new();
        assert_eq!(std::mem::size_of_val(&validator), 0);
    }

    #[test]
    fn test_validate_empty_task() {
        let validator = FieldTypeValidator::new();
        let schema = crate::types::CompressionSchema {
            task: Some("   ".to_string()),
            context: Some("valid context".to_string()),
            role: None,
        };
        let issues = validator.validate(&schema);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field_name, "task");
        assert_eq!(issues[0].issue_type, "empty");
        assert_eq!(issues[0].severity, "error");
    }


    #[test]
    fn test_validate_context_empty() {
        let validator = FieldTypeValidator::new();
        let schema = crate::types::CompressionSchema {
            task: Some("valid task".to_string()),
            context: Some("   ".to_string()),
            role: None,
        };
        let issues = validator.validate(&schema);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field_name, "context[0]");
        assert_eq!(issues[0].issue_type, "empty");
    }

    #[test]
    fn test_validate_valid_fields() {
        let validator = FieldTypeValidator::new();
        let schema = crate::types::CompressionSchema {
            task: Some("valid task".to_string()),
            context: Some("valid context".to_string()),
            role: None,
        };
        let issues = validator.validate(&schema);

        assert_eq!(issues.len(), 0);
    }

    #[test]
    fn test_is_valid_text_content() {
        assert!(!FieldTypeValidator::is_valid_text_content(""));
        assert!(!FieldTypeValidator::is_valid_text_content("   "));
        assert!(FieldTypeValidator::is_valid_text_content("hello"));
        assert!(FieldTypeValidator::is_valid_text_content("hello world"));
        // Assuming 500 char limit
        let long_string = "a".repeat(500);
        assert!(FieldTypeValidator::is_valid_text_content(&long_string));
        let too_long = "a".repeat(501);
        assert!(!FieldTypeValidator::is_valid_text_content(&too_long));
    }
}
