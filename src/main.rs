use alltrailsgpx::{get_input_reader, get_output_writer, run, Args};
use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = Args::parse();

    let reader = get_input_reader(&args.input)?;
    let writer = get_output_writer(&args.output)?;

    run(reader, writer)
}
