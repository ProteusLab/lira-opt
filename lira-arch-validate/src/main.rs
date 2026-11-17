use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    input: PathBuf,
    /// Statement kinds that are accepted without schema checks.
    #[arg(long, value_delimiter = ',')]
    custom_kinds: Vec<String>,
    /// Known `divisor:dividend` multiplier pairs, e.g. `VL:VLxVL`.
    #[arg(long, value_delimiter = ',')]
    mult_divides: Vec<String>,
    #[arg(long, short = 'j')]
    threads: Option<usize>,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let arch = lira::Arch::read_yaml(&cli.input)
        .with_context(|| format!("failed to read Arch from {:?}", cli.input))?;
    if let Some(threads) = cli.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .unwrap();
    }

    let custom_kinds: Vec<&str> = cli.custom_kinds.iter().map(String::as_str).collect();
    let mult_divides = cli
        .mult_divides
        .iter()
        .map(|pair| {
            let (small, large) = pair
                .split_once(':')
                .with_context(|| format!("expected `divisor:dividend`, got {pair:?}"))?;
            Ok((small, large))
        })
        .collect::<Result<Vec<_>>>()?;
    lira_ext::validate_arch(&arch, &custom_kinds, &mult_divides)?;
    Ok(())
}
