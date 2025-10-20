use alltrailsgpx::{get_input_reader, get_output_writer, run, Args};
use clap::Parser;

fn main() -> Result<(), alltrailsgpx::Error> {
    let args = Args::parse();

    let reader = get_input_reader(&args.input)?;
    let writer = get_output_writer(&args.output)?;

    run(reader, writer)
}
