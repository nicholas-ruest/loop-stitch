#![forbid(unsafe_code)]

use clap::{Args, Parser, Subcommand};
use loop_stitch::{SearchConfig, find_loop_candidates, read_wav, write_loop};
use std::error::Error;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version, about = "Find and render low-click WAV loops")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Score and print the best loop regions as JSON.
    Analyze {
        /// Input mono or stereo 16-bit PCM WAV.
        input: PathBuf,
        #[command(flatten)]
        search: SearchArgs,
    },
    /// Render the best loop region to a new WAV.
    Render {
        /// Input mono or stereo 16-bit PCM WAV.
        input: PathBuf,
        /// Output 16-bit PCM WAV.
        output: PathBuf,
        #[command(flatten)]
        search: SearchArgs,
        /// Replace an existing output file.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// Desired loop length in milliseconds.
    #[arg(long)]
    target_ms: u64,
    /// Allowed deviation from the target length.
    #[arg(long, default_value_t = 100)]
    tolerance_ms: u64,
    /// Search for loop starts within this initial span.
    #[arg(long, default_value_t = 150)]
    start_search_ms: u64,
    /// Waveform window compared across the seam.
    #[arg(long, default_value_t = 12)]
    compare_ms: u64,
    /// Search grid spacing in milliseconds.
    #[arg(long, default_value_t = 2)]
    step_ms: u64,
    /// Number of ranked candidates to report.
    #[arg(long, default_value_t = 5)]
    candidates: usize,
}

impl From<SearchArgs> for SearchConfig {
    fn from(args: SearchArgs) -> Self {
        Self {
            target_ms: args.target_ms,
            tolerance_ms: args.tolerance_ms,
            start_search_ms: args.start_search_ms,
            compare_ms: args.compare_ms,
            step_ms: args.step_ms,
            candidates: args.candidates,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("loop-stitch: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Analyze { input, search } => {
            let audio = read_wav(input)?;
            let plans = find_loop_candidates(&audio, search.into())?;
            println!("{}", serde_json::to_string_pretty(&plans)?);
        }
        Command::Render {
            input,
            output,
            search,
            force,
        } => {
            if output.exists() && !force {
                return Err(format!(
                    "output already exists: {} (pass --force to replace it)",
                    output.display()
                )
                .into());
            }
            let audio = read_wav(input)?;
            let plan = find_loop_candidates(&audio, search.into())?
                .into_iter()
                .next()
                .ok_or("candidate search returned no plans")?;
            write_loop(output, &audio, &plan)?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
    }
    Ok(())
}
