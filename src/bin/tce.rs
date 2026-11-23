//! Developer CLI & CI/CD Prompt Budget Linter (`src/bin/tce.rs` -> `tce`).
//!
//! Implements `GRW-04` (`docs/PRINCIPAL_AUDIT_TODO_2026_11.md`):
//! - `tce compress [--mode gentle|balanced|aggressive] [--harness] [--max-tokens N] [--json] [--stdin]`
//! - `tce lint --max-tokens N [--mode balanced] [--stdin]`
//! - `tce verify --facts <path> --answer <path> [--json]` or `tce verify --stdin [--json]`
//!
//! Exits with code `0` on pass and `1` when a `--max-tokens` budget or grounding verification fails.

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use dashmap::DashMap;
use serde_json::json;
use token_compress_engine::npae::aggressive::config::{ConfigHandle, ConfigLoader, UnifiedConfig};
use token_compress_engine::npae::grounding::verifier::{verify_grounding, VerifyGroundingRequest};
use token_compress_engine::npae::ory::OryEngine;
use token_compress_engine::npae::schema::types::{NpaeConfig, RenderStyle};
use token_compress_engine::pipeline::orchestrator::PipelineOrchestrator;
use token_compress_engine::session::SessionHistory;
use token_compress_engine::types::OrchestratorResponse;
use token_compress_engine::utils::tokens::estimate_tokens;
use tokio::sync::Mutex;

fn print_usage() {
    println!(
        "tce — Token Compress Engine CLI & CI Prompt Linter\n\n\
         USAGE:\n\
           tce compress [--mode gentle|balanced|aggressive] [--harness] [--max-tokens N] [--json] [--stdin | <text>]\n\
           tce lint     [--mode balanced] --max-tokens N [--json] [--stdin | <file>]\n\
           tce verify   (--stdin | --facts <path> --answer <path>) [--json]\n\
           tce --help\n"
    );
}

fn read_stdin_string() -> Result<String> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .context("failed to read from stdin")?;
    Ok(buf)
}

fn build_orchestrator(config_path: Option<&PathBuf>) -> PipelineOrchestrator {
    let cfg = config_path
        .and_then(|p| ConfigLoader::load(p).ok())
        .or_else(|| ConfigLoader::load("config/unified.json").ok())
        .unwrap_or_else(UnifiedConfig::default);
    let config_handle = Arc::new(ConfigHandle::new(cfg));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));
    let schema_priors = Arc::new(DashMap::new());
    PipelineOrchestrator::build(schema_priors, config_handle, ory_engine)
}

async fn run_compress(args: &[String], enforce_lint: bool) -> Result<ExitCode> {
    let mut mode = String::from("balanced");
    let mut harness = false;
    let mut max_tokens: Option<usize> = None;
    let mut json_out = false;
    let mut use_stdin = false;
    let mut config_path: Option<PathBuf> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                i += 1;
                mode = args.get(i).context("--mode requires a value")?.clone();
            }
            "--harness" => harness = true,
            "--max-tokens" => {
                i += 1;
                let raw = args.get(i).context("--max-tokens requires a number")?;
                max_tokens = Some(raw.parse::<usize>().context("invalid --max-tokens")?);
            }
            "--config" => {
                i += 1;
                config_path = Some(PathBuf::from(
                    args.get(i).context("--config requires a path")?,
                ));
            }
            "--json" => json_out = true,
            "--stdin" => use_stdin = true,
            "-h" | "--help" => {
                print_usage();
                return Ok(ExitCode::SUCCESS);
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let prompt_text = if use_stdin || positional.is_empty() {
        read_stdin_string()?
    } else if positional.len() == 1 && PathBuf::from(&positional[0]).is_file() {
        fs::read_to_string(&positional[0])
            .with_context(|| format!("failed to read prompt file '{}'", positional[0]))?
    } else {
        positional.join(" ")
    };

    if prompt_text.trim().is_empty() {
        bail!("input prompt must not be empty");
    }

    let orchestrator = build_orchestrator(config_path.as_ref());
    let mut session = SessionHistory::new(5);
    let npae_cfg = NpaeConfig {
        render_style: Some(if harness {
            RenderStyle::Harness
        } else {
            RenderStyle::Crisp
        }),
        ..Default::default()
    };

    let resp = orchestrator
        .process(
            &prompt_text,
            &mut session,
            None,
            Some(&mode),
            Some(&npae_cfg),
            None,
        )
        .await
        .map_err(|e| anyhow::anyhow!("compression failed: {e}"))?;

    let (optimized, tok_orig, tok_final, tok_saved, tes, sfs, scs) = match resp {
        OrchestratorResponse::Legacy(r) => {
            let orig = estimate_tokens(&prompt_text);
            let fin = estimate_tokens(&r.response);
            let saved = orig.saturating_sub(fin);
            (
                r.response,
                orig as usize,
                fin as usize,
                saved as usize,
                r.scoring_result.tes,
                r.scoring_result.sfs,
                r.scoring_result.scs,
            )
        }
        OrchestratorResponse::Aggressive(r) => (
            r.optimized_prompt,
            r.token_original as usize,
            r.token_final as usize,
            r.token_saved.max(0) as usize,
            r.scoring_result.tes,
            r.scoring_result.sfs,
            r.scoring_result.scs,
        ),
    };

    let budget_exceeded = max_tokens.is_some_and(|cap| tok_final > cap);
    let lint_failed = enforce_lint && scs < 6.0;

    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mode": mode,
                "token_original": tok_orig,
                "token_final": tok_final,
                "token_saved": tok_saved,
                "scores": { "tes": tes, "sfs": sfs, "scs": scs },
                "budget_exceeded": budget_exceeded,
                "max_tokens": max_tokens,
                "optimized_prompt": optimized,
            }))?
        );
    } else {
        println!("{optimized}");
    }

    if budget_exceeded {
        eprintln!(
            "tce: budget exceeded ({tok_final} > {} tokens)",
            max_tokens.unwrap_or(0)
        );
        return Ok(ExitCode::from(1));
    }
    if lint_failed {
        eprintln!("tce: constraint completeness below threshold (SCS={scs:.2} < 6.00)");
        return Ok(ExitCode::from(1));
    }

    Ok(ExitCode::SUCCESS)
}

fn run_verify(args: &[String]) -> Result<ExitCode> {
    let mut facts_path: Option<PathBuf> = None;
    let mut answer_path: Option<PathBuf> = None;
    let mut json_out = false;
    let mut use_stdin = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--facts" => {
                i += 1;
                facts_path = Some(PathBuf::from(
                    args.get(i).context("--facts requires a path")?,
                ));
            }
            "--answer" => {
                i += 1;
                answer_path = Some(PathBuf::from(
                    args.get(i).context("--answer requires a path")?,
                ));
            }
            "--json" => json_out = true,
            "--stdin" => use_stdin = true,
            "-h" | "--help" => {
                print_usage();
                return Ok(ExitCode::SUCCESS);
            }
            other => bail!("unknown flag for verify: {other}"),
        }
        i += 1;
    }

    let req: VerifyGroundingRequest = if use_stdin {
        let raw = read_stdin_string()?;
        serde_json::from_str(&raw).context("expected VerifyGroundingRequest JSON on stdin")?
    } else {
        let f_path = facts_path.context("--facts <path> is required unless --stdin is used")?;
        let a_path = answer_path.context("--answer <path> is required unless --stdin is used")?;
        let facts_raw = fs::read_to_string(&f_path)
            .with_context(|| format!("failed to read facts file '{}'", f_path.display()))?;
        let answer_raw = fs::read_to_string(&a_path)
            .with_context(|| format!("failed to read answer file '{}'", a_path.display()))?;
        VerifyGroundingRequest {
            facts: facts_raw
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(ToString::to_string)
                .collect(),
            llm_answer: answer_raw,
            ..Default::default()
        }
    };

    let report = verify_grounding(&req).context("grounding verification failed")?;

    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "passed={} grounding_score={:.2} unsupported={} missing={}",
            report.passed,
            report.grounding_score,
            report.unsupported_claims.len(),
            report.missing_items.len()
        );
    }

    Ok(if report.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        print_usage();
        return Ok(ExitCode::SUCCESS);
    }

    match args[0].as_str() {
        "compress" => run_compress(&args[1..], false).await,
        "lint" => run_compress(&args[1..], true).await,
        "verify" => run_verify(&args[1..]),
        other => {
            eprintln!("unknown subcommand: {other}");
            print_usage();
            Ok(ExitCode::from(2))
        }
    }
}
