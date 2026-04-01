use std::collections::HashMap;

/// Semantic Clustering — groups similar tokens into clusters.
///
/// This is a placeholder implementation that groups tokens by their first character.
/// In a real system, this would use embeddings or a more sophisticated similarity measure.
#[derive(Debug, Default)]
pub struct SemanticClustering {
    // No state needed for this simple implementation
}

impl SemanticClustering {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cluster tokens into groups based on similarity.
    /// Returns groups of tokens and a label for each group.
    pub fn cluster(&self, tokens: &[String]) -> ClusterResult {
        if tokens.is_empty() {
            return ClusterResult {
                groups: Vec::new(),
                labels: Vec::new(),
            };
        }

        // Group by first character (lowercased)
        let mut map: HashMap<char, Vec<String>> = HashMap::new();
        for token in tokens {
            if let Some(first_char) = token.chars().next() {
                let key = first_char.to_lowercase().next().unwrap_or(first_char);
                map.entry(key).or_default().push(token.clone());
            }
        }

        // Convert to sorted vectors for deterministic output
        let mut groups: Vec<Vec<String>> = map.clone().into_values().collect();
        let mut labels: Vec<String> = map.keys().cloned().map(|c| c.to_string()).collect();

        // Sort groups and labels by label for deterministic output
        let mut paired: Vec<(String, Vec<String>)> = labels
            .iter()
            .zip(groups.iter())
            .map(|(label, group)| (label.clone(), group.clone()))
            .collect();
        paired.sort_by(|a, b| a.0.cmp(&b.0));

        labels = paired.iter().map(|(label, _)| label.clone()).collect();
        groups = paired.iter().map(|(_, group)| group.clone()).collect();

        ClusterResult { groups, labels }
    }
}

#[derive(Debug, Clone)]
pub struct ClusterResult {
    pub groups: Vec<Vec<String>>, // Each inner vector is a cluster of tokens
    pub labels: Vec<String>,      // Label for each cluster (e.g., first character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tokens() {
        let sc = SemanticClustering::new();
        let result = sc.cluster(&[]);
        assert!(result.groups.is_empty());
        assert!(result.labels.is_empty());
    }

    #[test]
    fn clusters_by_first_letter() {
        let sc = SemanticClustering::new();
        let tokens = vec![
            "apple".to_string(),
            "ant".to_string(),
            "banana".to_string(),
            "berry".to_string(),
            "cat".to_string(),
        ];
        let result = sc.cluster(&tokens);
        assert_eq!(result.labels, vec!["a", "b", "c"]);
        assert_eq!(result.groups.len(), 3);
        // Check that 'a' group has apple and ant
        let a_group = &result.groups[0];
        assert!(a_group.contains(&"apple".to_string()));
        assert!(a_group.contains(&"ant".to_string()));
        assert_eq!(a_group.len(), 2);
        // Check that 'b' group has banana and berry
        let b_group = &result.groups[1];
        assert!(b_group.contains(&"banana".to_string()));
        assert!(b_group.contains(&"berry".to_string()));
        assert_eq!(b_group.len(), 2);
        // Check that 'c' group has cat
        let c_group = &result.groups[2];
        assert!(c_group.contains(&"cat".to_string()));
        assert_eq!(c_group.len(), 1);
    }

    #[test]
    fn case_insensitive() {
        let sc = SemanticClustering::new();
        let tokens = vec![
            "Apple".to_string(),
            "ant".to_string(),
            "Banana".to_string(),
            "berry".to_string(),
        ];
        let result = sc.cluster(&tokens);
        assert_eq!(result.labels, vec!["a", "b"]);
        assert_eq!(result.groups.len(), 2);
        // 'a' group: Apple, ant
        let a_group = &result.groups[0];
        assert!(a_group.contains(&"Apple".to_string()));
        assert!(a_group.contains(&"ant".to_string()));
        // 'b' group: Banana, berry
        let b_group = &result.groups[1];
        assert!(b_group.contains(&"Banana".to_string()));
        assert!(b_group.contains(&"berry".to_string()));
    }
}
