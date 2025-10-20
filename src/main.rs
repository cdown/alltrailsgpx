use anyhow::{Context, Result};
use clap::Parser;
use gpx::{Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// The input JSON file containing the polyline data. Defaults to stdin.
    #[arg(short, long)]
    input: Option<String>,

    /// The GPX file to create. Defaults to stdout.
    #[arg(short, long)]
    output: Option<String>,
}

fn find_in_json<'a>(json: &'a Value, paths: &[&str]) -> Option<&'a Value> {
    paths.iter().find_map(|path| json.pointer(path))
}

// - detail=offline: has "trails" array at root (e.g., /trails/0/defaultMap/routes/0/...)
// - detail=deep: has "maps" array at root (e.g., /maps/0/routes/0/...)
fn extract_polyline(json: &Value) -> Result<&str> {
    find_in_json(
        json,
        &[
            "/trails/0/defaultMap/routes/0/lineSegments/0/polyline/pointsData",
            "/maps/0/routes/0/lineSegments/0/polyline/pointsData",
        ],
    )
    .context("Polyline data not found in JSON")?
    .as_str()
    .context("Polyline data is not a string")
}

fn extract_route_name(json: &Value) -> Result<&str> {
    find_in_json(json, &["/trails/0/name", "/maps/0/name"])
        .context("Route name not found in JSON")?
        .as_str()
        .context("Route name data is not a string")
}

fn create_gpx(line_string: geo_types::LineString<f64>, name: &str) -> Track {
    let waypoints = line_string
        .0
        .into_iter()
        .map(|coord| Waypoint::new(coord.into()))
        .collect::<Vec<_>>();

    let segment = TrackSegment { points: waypoints };

    Track {
        name: Some(name.to_string()),
        segments: vec![segment],
        ..Default::default()
    }
}

fn write_gpx<W: Write>(track: Track, writer: W) -> Result<()> {
    let gpx = Gpx {
        version: GpxVersion::Gpx11,
        creator: Some("alltrailsgpx".to_string()),
        tracks: vec![track],
        ..Default::default()
    };

    gpx::write(&gpx, writer).context("Error writing GPX data")
}

fn get_input_reader(input: &Option<String>) -> Result<Box<dyn Read>> {
    match input.as_deref() {
        None | Some("-") => Ok(Box::new(std::io::stdin().lock())),
        Some(file_name) => {
            let file = File::open(file_name)
                .with_context(|| format!("Failed to open input file: {file_name}"))?;
            Ok(Box::new(BufReader::new(file)))
        }
    }
}

fn get_output_writer(output: &Option<String>) -> Result<Box<dyn Write>> {
    match output.as_deref() {
        None | Some("-") => Ok(Box::new(std::io::stdout())),
        Some(file_name) => {
            let file = File::create(file_name)
                .with_context(|| format!("Failed to create file: {file_name}"))?;
            Ok(Box::new(file))
        }
    }
}

fn run<R: Read, W: Write>(reader: R, writer: W) -> Result<()> {
    let json: Value = serde_json::from_reader(reader).context("Failed to parse JSON input")?;

    let polyline_str = extract_polyline(&json)?;
    let route_name = extract_route_name(&json)?;

    let line_string =
        polyline::decode_polyline(polyline_str, 5).context("Failed to decode polyline")?;

    let track = create_gpx(line_string, route_name);

    write_gpx(track, BufWriter::new(writer)).context("Failed to write GPX data")?;

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let reader = get_input_reader(&args.input)?;
    let writer = get_output_writer(&args.output)?;

    run(reader, writer)
}
