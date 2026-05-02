use clap::{Parser, ValueEnum};

use procgen::demo::cave::build_cave_situation;
use procgen::demo::crypt::build_crypt_situation;
use procgen::demo::tavern::build_tavern_situation;
use procgen::pipeline::{Pipeline, PipelineConfig};
use procgen::tile::ascii::render_ascii_full;
use procgen::tile::registry::TileRegistry;
use tracing_subscriber::EnvFilter;

/// Procedural dungeon generator.
#[derive(Parser)]
#[command(name = "procgen", version, about)]
struct Cli {
    /// Scenario to generate.
    #[arg(default_value = "crypt")]
    scenario: Scenario,

    /// Seed for deterministic generation.
    #[arg(long)]
    seed: Option<u64>,

    /// Enable tracing output (to stderr).
    #[arg(long, default_missing_value = "info", num_args = 0..=1)]
    trace: Option<TraceLevel>,
}

#[derive(Clone, ValueEnum)]
enum Scenario {
    Crypt,
    Tavern,
    Cave,
}

#[derive(Clone, ValueEnum)]
enum TraceLevel {
    Info,
    Debug,
    All,
}

fn main() {
    let cli = Cli::parse();

    // Initialise tracing if requested.
    if let Some(level) = &cli.trace {
        let level_str = match level {
            TraceLevel::Info => "info",
            TraceLevel::Debug => "debug",
            TraceLevel::All => "trace",
        };
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("procgen={level_str}")));
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
    }

    let config = PipelineConfig {
        seed: cli.seed,
        ..PipelineConfig::default()
    };
    let pipeline = Pipeline { config };

    let result = match cli.scenario {
        Scenario::Crypt => {
            let situation = build_crypt_situation();
            println!("=== Noble Crypt ===\n");
            pipeline.run(&situation)
        }
        Scenario::Tavern => {
            let situation = build_tavern_situation();
            println!("=== Tavern Cellar ===\n");
            pipeline.run(&situation)
        }
        Scenario::Cave => {
            let situation = build_cave_situation();
            println!("=== Natural Cave ===\n");
            pipeline.run(&situation)
        }
    };

    match result {
        Ok(r) => {
            let registry = TileRegistry::default_registry();
            println!(
                "{}",
                render_ascii_full(&r.tiles, &r.features, &r.entities, &registry)
            );
        }
        Err(e) => {
            eprintln!("Pipeline error: {e}");
            std::process::exit(1);
        }
    }
}
