use plain_rs::image_inference::{Engine, ImageEncoder, manifest::Manifest};
use serde::Serialize;
use std::{path::PathBuf, time::Instant};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultRow {
    kind: String,
    input: String,
    elapsed_ms: f64,
    vector: Option<Vec<f32>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    initialized_ms: f64,
    rows: Vec<ResultRow>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let directory = PathBuf::from(
        arguments
            .next()
            .ok_or("Usage: image_search PACKAGE OUTPUT_JSON [image:PATH|text:QUERY ...]")?,
    );
    let output = PathBuf::from(arguments.next().ok_or("Missing output file")?);
    let manifest = Manifest::parse(&std::fs::read(directory.join("manifest.json"))?)?;
    let start = Instant::now();
    let engine = Engine::open(directory, manifest)?;
    let initialized_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut rows = Vec::new();
    for argument in arguments {
        let (kind, input) = argument
            .split_once(':')
            .ok_or("Expected image:PATH or text:QUERY")?;
        let start = Instant::now();
        let vector = match kind {
            "image" => engine.image(std::path::Path::new(input))?,
            "text" => Some(engine.text(input)?),
            _ => return Err("Unknown input kind".into()),
        };
        rows.push(ResultRow {
            kind: kind.into(),
            input: input.into(),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            vector,
        });
    }
    std::fs::write(
        output,
        serde_json::to_vec(&Report {
            initialized_ms,
            rows,
        })?,
    )?;
    Ok(())
}
