use std::io::{self, Write};
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use nova_ingest::dedup_benchmark::{run_synthetic_benchmark, SyntheticBenchmarkConfig};

fn main() -> Result<()> {
    let Some(config) = parse_config()? else {
        print_help();
        return Ok(());
    };
    let report = run_synthetic_benchmark(&config)?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, &report).context("serialize benchmark report")?;
    writeln!(output).context("write benchmark report")?;
    Ok(())
}

fn parse_config() -> Result<Option<SyntheticBenchmarkConfig>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        return Ok(None);
    }

    let mut config = SyntheticBenchmarkConfig::default();
    let mut index = 0_usize;
    while index < args.len() {
        let flag = &args[index];
        index += 1;
        let value = args
            .get(index)
            .with_context(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--books" => config.books = parse_value(flag, value)?,
            "--chapters-per-book" => config.chapters_per_book = parse_value(flag, value)?,
            "--related-pair-every" => config.related_pair_every = parse_value(flag, value)?,
            "--contained-added-chapters" => {
                config.contained_added_chapters = parse_value(flag, value)?;
            }
            "--max-hash-document-frequency" => {
                config.max_hash_document_frequency = parse_value(flag, value)?;
            }
            "--min-shared-chapters" => config.min_shared_chapters = parse_value(flag, value)?,
            "--seed" => config.seed = parse_value(flag, value)?,
            _ => bail!("unknown argument {flag}; use --help for usage"),
        }
        index += 1;
    }
    Ok(Some(config))
}

fn parse_value<T>(flag: &str, value: &str) -> Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value
        .parse()
        .with_context(|| format!("invalid value for {flag}: {value}"))
}

fn print_help() {
    println!(
        "Run a deterministic in-process novel deduplication capacity profile.\n\n\
Usage:\n  cargo run --release -p nova-ingest --example dedup_bench -- [options]\n\n\
Options:\n  --books <N>                         default 1000\n  --chapters-per-book <N>             default 12\n  --related-pair-every <N>             default 20\n  --contained-added-chapters <N>       default 5\n  --max-hash-document-frequency <N>    default 50\n  --min-shared-chapters <N>            default 2\n  --seed <N>                           default 42\n\n\
The JSON report includes phase timings, candidate counts, relation counts,\n\
candidate recall against synthetic ground truth and process RSS samples."
    );
}
