use procgen::demo::crypt::{CryptIntentBuilder, build_crypt_situation};
use procgen::demo::tavern::{TavernIntentBuilder, build_tavern_situation};
use procgen::intent::builder::IntentBuilder;
use procgen::spatial::planner::SpatialPlanner;
use procgen::tile::rasterize::Rasterizer;
use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Parse --trace flag: --trace (INFO), --trace=debug (DEBUG), --trace=all (TRACE)
    let trace_level = args.iter().find_map(|a| {
        if a == "--trace" {
            Some("info")
        } else if let Some(level) = a.strip_prefix("--trace=") {
            Some(match level {
                "debug" => "debug",
                "all" | "trace" => "trace",
                _ => "info",
            })
        } else {
            None
        }
    });

    if let Some(level) = trace_level {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("procgen={level}")));
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
    }

    // Find the scenario name (first arg that doesn't start with --)
    let scenario = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .unwrap_or("crypt");

    match scenario {
        "crypt" => run_crypt()?,
        "tavern" => run_tavern()?,
        other => {
            eprintln!("Unknown scenario: {other}");
            eprintln!("Available: crypt, tavern");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn run_crypt() -> Result<(), Box<dyn std::error::Error>> {
    let situation = build_crypt_situation();
    let intent = CryptIntentBuilder.build(&situation)?;

    let spatial_plan = procgen::spatial::planner::SimpleSpatialPlanner.plan(&intent)?;
    let geometry = procgen::geometry::planner::SimpleGeometryPlanner::default()
        .plan_with_validation(&spatial_plan)?;
    let map = procgen::tile::rasterize::SimpleRasterizer.rasterize(&geometry)?;

    println!("=== Noble Crypt ===\n");
    println!("{}", procgen::tile::ascii::render_ascii(&map));
    Ok(())
}

fn run_tavern() -> Result<(), Box<dyn std::error::Error>> {
    let situation = build_tavern_situation();
    let intent = TavernIntentBuilder.build(&situation)?;

    let spatial_plan = procgen::spatial::planner::SimpleSpatialPlanner.plan(&intent)?;
    let geometry = procgen::geometry::planner::SimpleGeometryPlanner::default()
        .plan_with_validation(&spatial_plan)?;
    let map = procgen::tile::rasterize::SimpleRasterizer.rasterize(&geometry)?;

    println!("=== Tavern Cellar ===\n");
    println!("{}", procgen::tile::ascii::render_ascii(&map));
    Ok(())
}
