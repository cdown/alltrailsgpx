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

fn write_gpx(track: Track, writer: impl Write) -> Result<()> {
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

fn run(reader: impl Read, writer: impl Write) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use geo_types::Coord;
    use gpx::Gpx;
    use polyline::encode_coordinates;
    use serde_json::{json, Value};
    use std::io::BufReader;

    struct TestCase<'a> {
        name: &'a str,
        route_name: &'a str,
        coords: Vec<Coord>,
        json_builder: Box<dyn Fn(&str) -> Value>,
    }

    fn run_conversion_test(case: TestCase) {
        let polyline_str =
            encode_coordinates(case.coords.clone(), 5).expect("Failed to encode polyline");
        let json_value = (case.json_builder)(&polyline_str);
        let json_input = json_value.to_string();
        let parsed_gpx = run_and_parse_gpx(&json_input);

        assert_gpx_basics(&parsed_gpx, case.route_name, case.coords.len());

        let points = &parsed_gpx.tracks[0].segments[0].points;
        for (i, coord) in case.coords.iter().enumerate() {
            let point = points[i].point();
            assert!(
                (point.y() - coord.y).abs() < 1e-6 && (point.x() - coord.x).abs() < 1e-6,
                "Test case '{}' failed: point {} mismatch.\n Expected: ({:?})\n  Got: ({:?})",
                case.name,
                i,
                coord,
                point
            );
        }
    }

    #[test]
    fn test_offline_format_conversion() {
        let case = TestCase {
            name: "offline_format",
            route_name: "My Test Trail",
            coords: vec![
                Coord { x: -120.2, y: 38.5 },
                Coord {
                    x: -120.95,
                    y: 40.7,
                },
                Coord {
                    x: -121.2116,
                    y: 40.9416,
                },
            ],
            json_builder: Box::new(|polyline| {
                json!({
                    "trails": [
                        {
                            "name": "My Test Trail",
                            "defaultMap": {
                                "routes": [
                                    {
                                        "lineSegments": [
                                            {
                                                "polyline": {
                                                    "pointsData": polyline
                                                }
                                            }
                                        ]
                                    }
                                ]
                            }
                        }
                    ]
                })
            }),
        };
        run_conversion_test(case);
    }

    #[test]
    fn test_deep_format_conversion() {
        let case = TestCase {
            name: "deep_format",
            route_name: "My Other Trail",
            coords: vec![Coord { x: -121.0, y: 38.8 }],
            json_builder: Box::new(|polyline| {
                json!({
                    "maps": [
                        {
                            "name": "My Other Trail",
                            "routes": [
                                {
                                    "lineSegments": [
                                        {
                                            "polyline": {
                                                "pointsData": polyline
                                            }
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                })
            }),
        };
        run_conversion_test(case);
    }

    fn run_and_parse_gpx(json_input: &str) -> Gpx {
        let mut output_buffer: Vec<u8> = Vec::new();
        run(json_input.as_bytes(), &mut output_buffer).unwrap_or_else(|e| {
            panic!(
                "Test run failed: {e:?}\nOutput: {}",
                String::from_utf8_lossy(&output_buffer)
            )
        });

        gpx::read(BufReader::new(output_buffer.as_slice())).expect("Failed to parse output GPX")
    }

    fn assert_gpx_basics(gpx: &Gpx, expected_name: &str, expected_point_count: usize) {
        assert_eq!(gpx.creator.as_deref(), Some("alltrailsgpx"));
        assert_eq!(gpx.tracks.len(), 1, "Should contain exactly one track");

        let track = &gpx.tracks[0];
        assert_eq!(track.name.as_deref(), Some(expected_name));
        assert_eq!(track.segments.len(), 1, "Track should have one segment");

        let points = &track.segments[0].points;
        assert_eq!(points.len(), expected_point_count);
    }
}
