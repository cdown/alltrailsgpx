use anyhow::{Context, Result};
use clap::Parser;
use gpx::{Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use std::fs::File;
use std::io::{BufWriter, Read, Write};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// The input JSON file containing the polyline data. Defaults to stdin.
    #[clap(short, long)]
    input: Option<String>,

    /// The GPX file to create. Defaults to stdout.
    #[clap(short, long)]
    output: Option<String>,
}

fn read_input(input: &Option<String>) -> Result<String> {
    let mut contents = String::new();
    match input.as_deref() {
        None | Some("-") => {
            std::io::stdin()
                .read_to_string(&mut contents)
                .context("Error reading from stdin")?;
        }
        Some(file_name) => {
            let mut file = File::open(file_name)
                .with_context(|| format!("Failed to open input file: {file_name}"))?;
            file.read_to_string(&mut contents)
                .context("Error reading input file")?;
        }
    }
    Ok(contents)
}

fn extract_polyline(json_str: &str) -> Result<String> {
    let json: serde_json::Value =
        serde_json::from_str(json_str).context("Error parsing JSON input")?;

    let polyline_str = json
        .get("trails")
        .and_then(|trails| trails.get(0))
        .and_then(|trail| trail.get("defaultMap"))
        .and_then(|default_map| default_map.get("routes"))
        .and_then(|routes| routes.get(0))
        .and_then(|route| route.get("lineSegments"))
        .and_then(|segments| segments.get(0))
        .and_then(|segment| segment.get("polyline"))
        .and_then(|polyline| polyline.get("pointsData"))
        .and_then(|points| points.as_str())
        .ok_or_else(|| anyhow::anyhow!("Polyline data not found in JSON"))?;

    Ok(polyline_str.to_string())
}

fn create_gpx(line_string: geo_types::LineString<f64>) -> Track {
    let waypoints = line_string
        .0
        .into_iter()
        .map(|coord| {
            let point = geo_types::Point::new(coord.x, coord.y);
            Waypoint::new(point)
        })
        .collect::<Vec<_>>();

    let segment = TrackSegment { points: waypoints };

    Track {
        name: Some("Decoded Track".into()),
        segments: vec![segment],
        ..Default::default()
    }
}

fn write_gpx<W: Write>(track: Track, writer: W) -> Result<()> {
    let gpx = Gpx {
        version: GpxVersion::Gpx11,
        creator: Some("polyline2gpx".to_string()),
        tracks: vec![track],
        ..Default::default()
    };

    gpx::write(&gpx, writer).context("Error writing GPX data")
}

fn main() -> Result<()> {
    let args = Args::parse();

    let json_str = read_input(&args.input).context("Failed to read input")?;

    let polyline_str =
        extract_polyline(&json_str).context("Failed to extract polyline from JSON")?;

    let line_string =
        polyline::decode_polyline(&polyline_str, 5).context("Failed to decode polyline")?;

    let track = create_gpx(line_string);

    match args.output.as_deref() {
        None | Some("-") => {
            write_gpx(track, BufWriter::new(std::io::stdout()))
                .context("Failed to write GPX to stdout")?;
        }
        Some(file_name) => {
            let file = File::create(file_name)
                .with_context(|| format!("Failed to create file: {file_name}"))?;
            write_gpx(track, BufWriter::new(file)).context("Failed to write GPX file")?;
        }
    };

    Ok(())
}
