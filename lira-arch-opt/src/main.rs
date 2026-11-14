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

const AFTER_HELP: &str = "\
TIME: the effective per-case deadline is min(soft + remaining_pool, hard).
Overshoot above `soft` is charged to the shared pool; when the pool is
exhausted the run exits 1 and writes no output. A case that reaches its
deadline is skipped: its statements stay unchanged.

MEMORY: each case builds its own egraph and SMT encoding. Two caps bound it:
`Z3_MEMORY_MB` (default 512, 0 disables) limits the z3 child process,
`LIRA_MAX_ROWS` (default 200000) limits the summed rows of all
tables and skips the case when exceeded. Budget ~1 GB per worker with both
caps, ~2.5 GB without them.";

#[derive(Parser)]
#[command(about = "Optimize an arch file with the DFG egraph pass", after_help = AFTER_HELP)]
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

    /// Base max time per case in s; time above it is charged to the shared pool
    #[arg(long, default_value_t = 1)]
    timeout_per_case_soft: usize,
    /// Absolute max time per case in s
    #[arg(long, default_value_t = 60)]
    timeout_per_case_hard: usize,
    /// Shared pool for overshoot above the soft budget, in s
    /// (default: `timeout_per_case_soft * <number of cases>`)
    #[arg(long)]
    timeout_global_extra: Option<usize>,
}

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
    let max_rows = std::env::var("LIRA_MAX_ROWS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(200_000);
    if cli.opt.opt {
        arch.generalize_operations()?;
    }
    let case_count = arch.0.snippets.len() + arch.0.instructions.len();
    let pool = cli
        .timeout_global_extra
        .unwrap_or(cli.timeout_per_case_soft * case_count);
    let timeout_pool = AtomicUsize::new(pool * 1000);
    [].into_par_iter()
        .chain(
            arch.0
                .snippets
                .par_iter_mut()
                .with_min_len(1)
                .map(|s| (&s.name, &mut s.seq)),
        )
        .chain(
            arch.0
                .instructions
                .par_iter_mut()
                .with_min_len(1)
                .map(|s| (&s.name, &mut s.semantic)),
        )
        .filter(|(name, _)| filter(name))
        .for_each(|(name, stmts)| {
            let timeout = Duration::from_millis(
                cli.timeout_per_case_soft as u64 * 1000 + timeout_pool.load(SeqCst) as u64,
            )
            .min(Duration::from_secs(cli.timeout_per_case_hard as u64));
            let start = Instant::now();
            if let Err(error) = process(name, stmts, &config, &cli.opt, start + timeout, max_rows) {
                log::error!("{name}: {error:#}");
            }
            let took = start.elapsed().as_millis() as usize;
            let overshoot = took.saturating_sub(cli.timeout_per_case_soft * 1000);
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
    max_rows: usize,
) -> Result<()> {
    let before = stmts.len();
    let implicitness = |kind: &str| {
        let fail = || panic!("unknown kind {kind} in {name}");
        *config.implicitness.get(kind).unwrap_or_else(fail)
    };

    let start = Instant::now();
    let mut dfg = lira_trs::dfg::State::from_lira(stmts, implicitness);
    if opt.opt {
        let skipped = process_opt(name, &mut dfg, opt.mgc, deadline, max_rows, start)?;
        *stmts = dfg.to_lira();
        log::trace!(
            "{}",
            serde_json::json!({
                "name": name,
                "ms": start.elapsed().as_millis(),
                "before": before,
                "after": stmts.len(),
                "skipped": skipped,
            })
        );
    } else {
        *stmts = dfg.to_lira();
    }
    Ok(())
}

fn process_opt(
    name: &str,
    dfg: &mut lira_trs::dfg::State,
    mgc: usize,
    deadline: Instant,
    max_rows: usize,
    start: Instant,
) -> Result<bool> {
    let mut theory = lira_opt::Theory::default();
    theory.add_transformation_rulesets(mgc);
    let state = dfg.to_egraph(&mut theory);
    theory.run_ruleset_set_transformation_costs_to_zero();

    if let Err(error) = (|| -> Result<()> {
        theory.run_ruleset_transformations_operations(deadline, max_rows)?;
        theory.run_ruleset_transformations_vector(deadline, max_rows)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        *dfg = lira_trs::dfg::State::from_egraph_with_timeout(&theory, state, remaining)?;
        Ok(())
    })() {
        if Instant::now() >= deadline {
            let took = start.elapsed();
            log::warn!("{name}: timeout after {took:?} ({error:#})");
            return Ok(true);
        } else if theory.total_rows() > max_rows {
            log::warn!("{name}: too many rows: {error:#}");
            return Ok(true);
        } else {
            Err(error)?
        }
    }
    Ok(false)
}
