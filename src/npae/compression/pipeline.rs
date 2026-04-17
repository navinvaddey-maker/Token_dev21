use rayon;
use crate::npae::compression::types::{CompressedRepr, StageMetrics};
use crate::npae::compression::merge::{merge_stage1, merge_stage2, merge_stage3};
use crate::npae::compression::semantic;
use std::time::Instant;

pub fn run_parallel_pipeline(raw: &str) -> Result<CompressedRepr, String> {
    let tokens: Vec<String> = raw.split_whitespace().map(String::from).collect();

    let t0 = Instant::now();
    let (s1_lex, s1_spr) = rayon::join(
        || {
            // Mocking lexical compress
            tokens.clone()
        },
        || {
            // Mocking sparse prune
            vec![true; tokens.len()]
        },
    );
    let s1 = merge_stage1(s1_lex, s1_spr)?;
    let stage1_ms = t0.elapsed().as_millis() as u64;

    let t1 = Instant::now();
    let (s2_sem, s2_prd) = rayon::join(
        || semantic::compress(&s1),
        || {
            // Mock predictive encode returns intent probability dist
            Ok::<Vec<f32>, String>(vec![0.2; 5])
        },
    );
    let s2 = merge_stage2(s2_sem?, s2_prd?)?;
    let stage2_ms = t1.elapsed().as_millis() as u64;

    let t2 = Instant::now();
    let (s3_heb, s3_cmp) = rayon::join(
        || {
            // Mock hebbian
            Ok::<Vec<String>, String>(vec!["concept_a".to_string(), "concept_b".to_string()])
        },
        || {
            // Mock competitive
            Ok::<Vec<i32>, String>(vec![1, 2, 3])
        },
    );
    let stage3_ms = t2.elapsed().as_millis() as u64;

    merge_stage3(&s2, s3_heb?, s3_cmp?, StageMetrics { stage1_ms, stage2_ms, stage3_ms })
}
