use derive_more::Deref;
use gpx::{Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use thiserror::Error;

const POLYLINE_PRECISION: u32 = 5;
const GPX_CREATOR: &str = "alltrailsgpx";

#[derive(Error, Debug)]
pub enum Error {
    #[error("Polyline data not found in JSON")]
    PolylineNotFound,

    #[error("Polyline data is not a string")]
    PolylineNotString,

    #[error("Route name not found in JSON")]
    RouteNameNotFound,

    #[error("Route name is not a string")]
    RouteNameNotString,

    #[error("Failed to decode polyline: {0}")]
    PolylineDecodeError(#[from] polyline::errors::PolylineError),

    #[error("Failed to parse JSON input: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("Failed to open file: {path}")]
    FileError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Error writing GPX data: {0}")]
    GpxWriteError(#[from] gpx::errors::GpxError),
}

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The input JSON file containing the polyline data. Defaults to stdin.
    #[arg(short, long)]
    pub input: Option<String>,

    /// The GPX file to create. Defaults to stdout.
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Copy, Deref)]
pub struct Polyline<'a>(&'a str);

#[derive(Debug, Clone, Copy, Deref)]
pub struct RouteName<'a>(&'a str);

pub fn find_in_json<'json>(json: &'json Value, paths: &[&str]) -> Option<&'json Value> {
    paths.iter().find_map(|path| json.pointer(path))
}

// - detail=offline: has "trails" array at root (e.g., /trails/0/defaultMap/routes/0/...)
// - detail=deep: has "maps" array at root (e.g., /maps/0/routes/0/...)
pub fn extract_polyline(json: &Value) -> Result<Polyline<'_>, Error> {
    let polyline_str = find_in_json(
        json,
        &[
            "/trails/0/defaultMap/routes/0/lineSegments/0/polyline/pointsData",
            "/maps/0/routes/0/lineSegments/0/polyline/pointsData",
        ],
    )
    .ok_or(Error::PolylineNotFound)?
    .as_str()
    .ok_or(Error::PolylineNotString)?;

    Ok(Polyline(polyline_str))
}

pub fn extract_route_name(json: &Value) -> Result<RouteName<'_>, Error> {
    let name_str = find_in_json(json, &["/trails/0/name", "/maps/0/name"])
        .ok_or(Error::RouteNameNotFound)?
        .as_str()
        .ok_or(Error::RouteNameNotString)?;

    Ok(RouteName(name_str))
}

pub fn create_gpx(line_string: geo_types::LineString<f64>, name: RouteName<'_>) -> Track {
    let waypoints: Vec<Waypoint> = line_string
        .into_inner()
        .into_iter()
        .map(|coord| Waypoint::new(coord.into()))
        .collect();

    let segment = TrackSegment { points: waypoints };

    Track {
        name: Some(name.to_string()),
        segments: vec![segment],
        ..Default::default()
    }
}

pub fn write_gpx(track: Track, writer: impl Write) -> Result<(), Error> {
    let gpx = Gpx {
        version: GpxVersion::Gpx11,
        creator: Some(GPX_CREATOR.to_string()),
        tracks: vec![track],
        ..Default::default()
    };

    Ok(gpx::write(&gpx, writer)?)
}

pub fn get_input_reader(input: &Option<String>) -> Result<Box<dyn Read>, Error> {
    match input.as_deref() {
        None | Some("-") => Ok(Box::new(std::io::stdin().lock())),
        Some(file_name) => {
            let file = File::open(file_name).map_err(|source| Error::FileError {
                path: file_name.to_string(),
                source,
            })?;
            Ok(Box::new(BufReader::new(file)))
        }
    }
}

pub fn get_output_writer(output: &Option<String>) -> Result<Box<dyn Write>, Error> {
    let writer: Box<dyn Write> = match output.as_deref() {
        None | Some("-") => Box::new(std::io::stdout()),
        Some(file_name) => {
            let file = File::create(file_name).map_err(|source| Error::FileError {
                path: file_name.to_string(),
                source,
            })?;
            Box::new(file)
        }
    };

    Ok(Box::new(BufWriter::new(writer)))
}

pub fn run(reader: impl Read, writer: impl Write) -> Result<(), Error> {
    let json: Value = serde_json::from_reader(reader)?;

    let polyline = extract_polyline(&json)?;
    let route_name = extract_route_name(&json)?;

    let line_string = polyline::decode_polyline(&polyline, POLYLINE_PRECISION)?;

    let track = create_gpx(line_string, route_name);

    write_gpx(track, writer)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo_types::Coord;
    use gpx::Gpx;
    use polyline::encode_coordinates;
    use serde_json::{json, Value};
    use std::io::BufReader;

    struct TestCase<'tc> {
        name: &'tc str,
        route_name: &'tc str,
        coords: Vec<Coord>,
        json_builder: Box<dyn Fn(&str) -> Value>,
    }

    fn run_conversion_test(case: TestCase<'_>) {
        let polyline_str = encode_coordinates(case.coords.clone(), POLYLINE_PRECISION)
            .expect("Failed to encode polyline");
        let json_value = (case.json_builder)(&polyline_str);
        let json_input = json_value.to_string();
        let parsed_gpx = run_and_parse_gpx(&json_input);

        assert_gpx_basics(&parsed_gpx, case.route_name, case.coords.len());

        let points = &parsed_gpx.tracks[0].segments[0].points;
        for (i, coord) in case.coords.iter().enumerate() {
            let point = points[i].point();
            const TOLERANCE: f64 = 1e-6;
            assert!(
                (point.y() - coord.y).abs() < TOLERANCE && (point.x() - coord.x).abs() < TOLERANCE,
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
        assert_eq!(gpx.creator.as_deref(), Some(GPX_CREATOR));
        assert_eq!(gpx.tracks.len(), 1, "Should contain exactly one track");

        let track = &gpx.tracks[0];
        assert_eq!(track.name.as_deref(), Some(expected_name));
        assert_eq!(track.segments.len(), 1, "Track should have one segment");

        let points = &track.segments[0].points;
        assert_eq!(points.len(), expected_point_count);
    }
}
