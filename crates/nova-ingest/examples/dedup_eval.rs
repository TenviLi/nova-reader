use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use nova_ingest::dedup_evaluation::{
    evaluate_relations, read_labels_jsonl, read_predictions_jsonl,
};

fn main() -> Result<()> {
    let Some((labels_path, predictions_path)) = parse_paths()? else {
        print_help();
        return Ok(());
    };

    let labels = read_labels_jsonl(BufReader::new(
        File::open(&labels_path)
            .with_context(|| format!("open labels file {}", labels_path.display()))?,
    ))
    .with_context(|| format!("parse labels file {}", labels_path.display()))?;
    let predictions = read_predictions_jsonl(BufReader::new(
        File::open(&predictions_path)
            .with_context(|| format!("open predictions file {}", predictions_path.display()))?,
    ))
    .with_context(|| format!("parse predictions file {}", predictions_path.display()))?;
    let report = evaluate_relations(&labels, &predictions)?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, &report).context("serialize evaluation report")?;
    writeln!(output).context("write evaluation report")?;
    Ok(())
}

fn parse_paths() -> Result<Option<(PathBuf, PathBuf)>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        return Ok(None);
    }

    let mut labels = None;
    let mut predictions = None;
    let mut index = 0_usize;
    while index < args.len() {
        let flag = &args[index];
        index += 1;
        let value = args
            .get(index)
            .with_context(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--labels" => labels = Some(PathBuf::from(value)),
            "--predictions" => predictions = Some(PathBuf::from(value)),
            _ => bail!("unknown argument {flag}; use --help for usage"),
        }
        index += 1;
    }

    let labels = labels.context("--labels is required")?;
    let predictions = predictions.context("--predictions is required")?;
    Ok(Some((labels, predictions)))
}

fn print_help() {
    println!(
        r#"Evaluate pair-level novel deduplication predictions.

Usage:
  cargo run -p nova-ingest --example dedup_eval -- \
    --labels <labels.jsonl> --predictions <predictions.jsonl>

The command requires exactly one prediction per labeled pair and writes JSON
with one-vs-rest precision, recall and F1 for every dedup relation."#
    );
}
