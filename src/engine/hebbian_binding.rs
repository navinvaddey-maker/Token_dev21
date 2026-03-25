use std::time::Instant;
use super::PrincipleResult;
use super::predictive_coding::get_role_frame;

/// CRISP output schema -> 5 Layers:
///   1. Role + Audience (from predictive coding)
///   2. Task verb
///   3. Format
///   4. Constraints
///   5. Scope (content)
pub fn run(text: &str, task: &str, use_case: &str) -> PrincipleResult {
    let start   = Instant::now();
    let frame   = get_role_frame(use_case);
    let (header, content) = if let Some((h, c)) = text.split_once("\n\n---\n\n") {
        (h, c)
    } else if let Some((h, c)) = text.split_once("\n---\n") {
        (h, c)
    } else if let Some((h, c)) = text.split_once(" --- ") {
        (h, c)
    } else {
        // Fallback: use default role and the whole text
        (frame.role, text)
    };

    let layer1 = if header.starts_with("Role:") { 
        header.to_string() 
    } else { 
        format!("Role: {}\nAudience: {}", frame.role, frame.audience) 
    };

    // CRISP assembly: 5 Layers
    let bound = format!(
        "{}\n\nTask: {}\n\nFormat:\n{}\n\nConstraints:\n{}\n\n---\n\n{}",
        layer1,
        task,
        frame.output_format,
        frame.constraints,
        content
    );

    PrincipleResult {
        text:          bound.clone(),
        chunks:        vec![bound],
        items_removed: 0,
        detail:        "CRISP reconstruction: 5 Layers (Role, Task, Format, Constraints, Scope)".to_string(),
        duration_ms:   start.elapsed().as_millis() as u64,
    }
}
