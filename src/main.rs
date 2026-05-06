use clap::{Parser, ValueEnum};

use procgen::demo::cave::build_cave_situation;
use procgen::demo::cellar::build_cellar_situation;
use procgen::demo::crypt::build_crypt_situation;
use procgen::demo::market::build_market_situation;
use procgen::demo::mine::build_mine_situation;
use procgen::demo::tavern::build_tavern_situation;
use procgen::entity::registry::EntityRegistry;
use procgen::feature::registry::FeatureRegistry;
use procgen::pipeline::{GeometryStrategy, Pipeline, PipelineConfig};
use procgen::tile::ascii::{render_ascii, render_ascii_annotated};
use procgen::tile::color::{render_ascii_annotated_colored, render_ascii_colored};
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

    /// Print annotated map with room letters and legend.
    #[arg(long)]
    annotate: bool,

    /// Geometry layout strategy.
    #[arg(long, default_value = "force")]
    layout: Layout,

    /// Enable colored output (ANSI). Default: auto-detect terminal.
    #[arg(long, default_value = "auto")]
    color: ColorMode,
}

#[derive(Clone, ValueEnum)]
enum Scenario {
    Crypt,
    Tavern,
    Cave,
    Cellar,
    Mine,
    Market,
}

#[derive(Clone, ValueEnum)]
enum TraceLevel {
    Info,
    Debug,
    All,
}

#[derive(Clone, ValueEnum)]
enum Layout {
    /// BFS column layout (original, good for small maps).
    Column,
    /// Force-directed simulation (better for larger maps).
    Force,
    /// Street-skeleton-first layout for urban environments.
    Street,
}

#[derive(Clone, ValueEnum)]
enum ColorMode {
    /// Auto-detect: color if stdout is a terminal.
    Auto,
    /// Always use color.
    On,
    /// Never use color.
    Off,
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
        geometry_strategy: match cli.layout {
            Layout::Column => GeometryStrategy::Column,
            Layout::Force => GeometryStrategy::ForceDirected,
            Layout::Street => GeometryStrategy::StreetSkeleton,
        },
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
        Scenario::Cellar => {
            let situation = build_cellar_situation();
            println!("=== Rat-Infested Port Cellar ===\n");
            pipeline.run(&situation)
        }
        Scenario::Mine => {
            let situation = build_mine_situation();
            println!("=== Abandoned Mine ===\n");
            pipeline.run(&situation)
        }
        Scenario::Market => {
            let situation = build_market_situation();
            println!("=== Market District ===\n");
            pipeline.run(&situation)
        }
    };

    match result {
        Ok(r) => {
            println!("(seed: {})\n", r.seed);
            let tile_registry = TileRegistry::default_registry();
            let feature_registry = FeatureRegistry::default_registry();
            let entity_registry = EntityRegistry::default_registry();

            let use_color = match cli.color {
                ColorMode::On => true,
                ColorMode::Off => false,
                ColorMode::Auto => std::io::IsTerminal::is_terminal(&std::io::stdout()),
            };

            if cli.annotate {
                if use_color {
                    println!(
                        "{}",
                        render_ascii_annotated_colored(
                            &r.tiles,
                            &r.features,
                            &r.entities,
                            &tile_registry,
                            &feature_registry,
                            &entity_registry,
                            &r.annotations,
                        )
                    );
                } else {
                    println!(
                        "{}",
                        render_ascii_annotated(
                            &r.tiles,
                            &r.features,
                            &r.entities,
                            &tile_registry,
                            &feature_registry,
                            &entity_registry,
                            &r.annotations,
                        )
                    );
                }
            } else if use_color {
                println!(
                    "{}",
                    render_ascii_colored(
                        &r.tiles,
                        &r.features,
                        &r.entities,
                        &tile_registry,
                        &feature_registry,
                        &entity_registry,
                    )
                );
            } else {
                println!(
                    "{}",
                    render_ascii(
                        &r.tiles,
                        &r.features,
                        &r.entities,
                        &tile_registry,
                        &feature_registry,
                        &entity_registry,
                    )
                );
            }

            // Print pacing warnings if any.
            if !r.pacing_warnings.is_empty() {
                println!();
                for w in &r.pacing_warnings {
                    println!("\u{26a0} {}", w);
                }
            }
        }
        Err(e) => {
            eprintln!("Pipeline error: {e}");
            std::process::exit(1);
        }
    }
}
