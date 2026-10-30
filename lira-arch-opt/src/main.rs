use std::{collections::BTreeMap, path::PathBuf, time::Instant};

use anyhow::{Context, Result};
use clap::Parser;
use rayon::prelude::*;
use serde::Deserialize;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long, short = 'j')]
    threads: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Config {
    include: Vec<String>,
    exclude: Vec<String>,
    implicitness: BTreeMap<String, lira_trs::dfg::Implicitness>,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let mut arch = lira::Arch::read_yaml(&cli.input)
        .with_context(|| format!("failed to read Arch from {:?}", cli.input))?;
    let mut config = Config::default();
    if let Some(path) = &cli.config {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read Config from {path:?}"))?;
        config = toml::from_str(&text)
            .with_context(|| format!("failed to parse Config from {path:?}"))?;
    }
    if let Some(threads) = cli.threads {
        let builder = rayon::ThreadPoolBuilder::new().num_threads(threads);
        builder.build_global().unwrap();
    }

    for (kind, value) in lira_trs::IMPLICITNESS_STD.iter() {
        let kind = kind.to_string();
        config.implicitness.entry(kind).or_insert(*value);
    }
    let filter = |name: &str| {
        (config.include.is_empty() || config.include.iter().any(|pattern| name.contains(pattern)))
            && !config.exclude.iter().any(|pattern| name.contains(pattern))
    };
    let arch_is = &mut arch.instructions;
    [].into_par_iter()
        .chain(arch.snippets.par_iter_mut().map(|s| (&s.name, &mut s.seq)))
        .chain(arch_is.par_iter_mut().map(|s| (&s.name, &mut s.semantic)))
        .filter(|(name, _)| filter(name))
        .for_each(|(name, stmts)| process(name, stmts, &config));

    std::fs::write(&cli.output, serde_yaml::to_string(&arch).unwrap())
        .with_context(|| format!("failed to write Arch to {:?}", cli.output))?;
    Ok(())
}

fn process(name: &str, stmts: &mut lira::StatementSeq, config: &Config) {
    let before = stmts.len();
    let implicitness = |kind: &str| {
        let fail = || panic!("unknown kind {kind} in {name}");
        *config.implicitness.get(kind).unwrap_or_else(fail)
    };

    let start = Instant::now();
    let dfg = lira_trs::dfg::State::from_lira(stmts, implicitness);
    *stmts = dfg.to_lira();
    let time = start.elapsed();

    let log = serde_json::json!({
        "name": name,
        "ms": time.as_millis(),
        "before": before,
        "after": stmts.len(),
    });
    log::trace!("{log}");
}
