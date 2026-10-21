//! `photosift-eval`: the read-only evaluation harness (see `tests/eval/README.md`).
//!
//! ```text
//! photosift-eval run [--shoots <file>] [--helper] [--max-shoots N] [--label TEXT]
//! photosift-eval report [--label A --label B ...]
//! ```

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context};
use photosift::eval::{self, RunOptions};

const USAGE: &str = "usage:\n  \
    photosift-eval run [--shoots <file>] [--helper] [--max-shoots N] [--label TEXT]\n  \
    photosift-eval report [--label A --label B ...]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    let rest: Vec<String> = args.collect();
    match command.as_deref() {
        Some("run") => run(&rest).await,
        Some("report") => report(&rest),
        _ => bail!("{USAGE}"),
    }
}

/// The value after `flag`, or an error naming it.
fn value(it: &mut std::slice::Iter<'_, String>, flag: &str) -> anyhow::Result<String> {
    it.next()
        .cloned()
        .with_context(|| format!("{flag} needs a value\n{USAGE}"))
}

async fn run(args: &[String]) -> anyhow::Result<()> {
    let mut shoots_file = eval::default_shoots_file();
    let mut helper = false;
    let mut max_shoots = None;
    let mut label = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--shoots" => shoots_file = PathBuf::from(value(&mut it, a)?),
            "--helper" => helper = true,
            "--max-shoots" => {
                max_shoots = Some(value(&mut it, a)?.parse().context("--max-shoots")?)
            }
            "--label" => label = Some(value(&mut it, a)?),
            other => bail!("unknown argument {other:?}\n{USAGE}"),
        }
    }
    let shoots = eval::read_shoots_file(&shoots_file)?;
    if shoots.is_empty() {
        bail!("{} lists no shoots", shoots_file.display());
    }
    let opts = RunOptions {
        eval_dir: eval::default_eval_dir(),
        shoots,
        helper,
        max_shoots,
        label: label.unwrap_or_else(|| if helper { "helper" } else { "baseline" }.into()),
        overrides: Default::default(),
        helper_timeout: Duration::from_secs(45 * 60),
    };
    std::fs::create_dir_all(&opts.eval_dir)?;
    let rows = eval::run(&opts).await?;
    print!("{}", eval::format_table(&rows));
    println!(
        "{} shoot(s) scored; results appended to {}",
        rows.len(),
        opts.eval_dir.join("results.jsonl").display()
    );
    Ok(())
}

fn report(args: &[String]) -> anyhow::Result<()> {
    let mut labels = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--label" => labels.push(value(&mut it, a)?),
            other => bail!("unknown argument {other:?}\n{USAGE}"),
        }
    }
    print!("{}", eval::report(&eval::default_eval_dir(), &labels)?);
    Ok(())
}
