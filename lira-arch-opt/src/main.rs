use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
    time::{Duration, Instant},
};

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

    #[command(flatten)]
    opt: CliOpt,

    /// Base timeout per case in s
    #[arg(long, default_value_t = 10)]
    timeout_per_case: usize,
    /// Global time pull to use for cases exceeding `timeout_per_case` in s
    #[arg(long, default_value_t = 60)]
    timeout_global_extra: usize,
}

// todo: soft_timeout, skip optimization (fallback on dfg-only) on soft_timeout
#[derive(Parser)]
struct CliOpt {
    /// Run actual optimization (overwise `lira->dfg->lira`-only)
    #[arg(long)]
    opt: bool,

    // todo: description
    #[arg(long, default_value_t = 1)]
    mgc: usize,
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
    let arch = lira::Arch::read_yaml(&cli.input)
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
    let mut arch = lira_opt::Arch(arch);
    if cli.opt.opt {
        arch.generalize_operations()?;
    }
    let arch_is = &mut arch.0.instructions;
    let timeout_pool = AtomicUsize::new(cli.timeout_global_extra * 1000);
    [].into_par_iter()
        .chain(arch.0.snippets.par_iter_mut().map(|s| (&s.name, &mut s.seq)))
        .chain(arch_is.par_iter_mut().map(|s| (&s.name, &mut s.semantic)))
        .filter(|(name, _)| filter(name))
        .for_each(|(name, stmts)| {
            let timeout = Duration::from_millis(
                cli.timeout_per_case as u64 * 1000 + timeout_pool.load(SeqCst) as u64,
            );
            let start = Instant::now();
            if let Err(error) = process(name, stmts, &config, &cli.opt, start + timeout) {
                log::error!("{name}: {error:#}");
            }
            let took = start.elapsed().as_millis() as usize;
            let overshoot = took.saturating_sub(cli.timeout_per_case * 1000);
            if overshoot > 0 {
                let previous = timeout_pool
                    .fetch_update(SeqCst, SeqCst, |pool| Some(pool.saturating_sub(overshoot)))
                    .unwrap_or(0);
                if previous <= overshoot {
                    log::error!("{name}: global time pool exhausted, exiting");
                    std::process::exit(1);
                }
            }
        });

    if cli.opt.opt {
        arch.specialize_operations()?;
    }
    std::fs::write(&cli.output, serde_yaml::to_string(&arch.0).unwrap())
        .with_context(|| format!("failed to write Arch to {:?}", cli.output))?;
    Ok(())
}

fn process(
    name: &str,
    stmts: &mut lira::StatementSeq,
    config: &Config,
    opt: &CliOpt,
    deadline: Instant,
) -> Result<()> {
    let before = stmts.len();
    let implicitness = |kind: &str| {
        let fail = || panic!("unknown kind {kind} in {name}");
        *config.implicitness.get(kind).unwrap_or_else(fail)
    };

    let start = Instant::now();
    let mut dfg = lira_trs::dfg::State::from_lira(stmts, implicitness);
    if opt.opt {
        let mut theory = lira_opt::Theory::default();
        theory.add_transformation_rulesets(opt.mgc);
        let state = dfg.to_egraph(&mut theory);
        theory.run_ruleset_set_transformation_costs_to_zero();

        theory.run_ruleset_transformations_operations(deadline)?;
        theory.run_ruleset_transformations_vector(deadline)?;
        dfg = lira_trs::dfg::State::from_egraph(&theory, state)?;
    }
    *stmts = dfg.to_lira();
    let time = start.elapsed();

    let log = serde_json::json!({
        "name": name,
        "ms": time.as_millis(),
        "before": before,
        "after": stmts.len(),
    });
    log::trace!("{log}");
    Ok(())
}
