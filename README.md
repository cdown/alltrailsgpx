# alltrailsgpx | [![Tests](https://img.shields.io/github/actions/workflow/status/cdown/alltrailsgpx/ci.yml?branch=master)](https://github.com/cdown/alltrailsgpx/actions?query=branch%3Amaster)

alltrailsgpx creates a GPX file from AllTrails route data, even if you don't
have the region unlocked. It extracts the map drawing data directly from the
AllTrails API response and converts it into a GPX track.

## Installation

    cargo install alltrailsgpx

## Usage

First, get the input file.

1. Open the full screen map page for the route you want to convert (e.g., `https://www.alltrails.com/en-gb/trail/england/bristol/bristol-and-abbots-leigh-circular`)
2. Open your browser's developer tools and navigate to the network tab.
3. Find the network request to the AllTrails API, which will look similar (e.g., `https://www.alltrails.com/api/alltrails/v3/trails/{route_id}`)
4. Save the response to a file.

Now you can provide this response to alltrailsgpx. By default the GPX is read
from stdin and written to stdout, but you can also output it to a file. For
example:

    alltrailsgpx -i route.json -o route.gpx
