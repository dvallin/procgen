use procgen::demo::crypt::{CryptIntentBuilder, build_crypt_situation};
use procgen::demo::tavern::{TavernIntentBuilder, build_tavern_situation};
use procgen::geometry::planner::GeometryPlanner;
use procgen::intent::builder::IntentBuilder;
use procgen::spatial::planner::SpatialPlanner;
use procgen::tile::rasterize::Rasterizer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = std::env::args().nth(1).unwrap_or_else(|| "crypt".into());

    match scenario.as_str() {
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
    let geometry = procgen::geometry::planner::SimpleGeometryPlanner.plan(&spatial_plan)?;
    let map = procgen::tile::rasterize::SimpleRasterizer.rasterize(&geometry)?;

    println!("=== Noble Crypt ===\n");
    println!("{}", procgen::tile::ascii::render_ascii(&map));
    Ok(())
}

fn run_tavern() -> Result<(), Box<dyn std::error::Error>> {
    let situation = build_tavern_situation();
    let intent = TavernIntentBuilder.build(&situation)?;

    let spatial_plan = procgen::spatial::planner::SimpleSpatialPlanner.plan(&intent)?;
    let geometry = procgen::geometry::planner::SimpleGeometryPlanner.plan(&spatial_plan)?;
    let map = procgen::tile::rasterize::SimpleRasterizer.rasterize(&geometry)?;

    println!("=== Tavern Cellar ===\n");
    println!("{}", procgen::tile::ascii::render_ascii(&map));
    Ok(())
}
