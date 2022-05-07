use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, multiple_occurrences = true, required = true)]
    sector: Vec<String>,

    /// Path to input matrix in numpy ndy format
    #[clap(short, long)]
    data_directory: Option<PathBuf>,

    /// Path to output distance and predecessor matrixes in numpy ndz format
    #[clap(short, long, default_value = "/var/tmp")]
    output_directory: PathBuf,

    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("args {:?}", args);

    Ok(())
}
