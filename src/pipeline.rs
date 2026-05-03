//! Reusable pipeline runner that orchestrates the full generation flow.
//!
//! ```text
//! SituationContext → MapIntent → SpatialPlan → GeometryPlan → TileMap → FeaturePlan → EntityPlan
//! ```
//!
//! The [`Pipeline`] struct drives every stage, validates intermediate results,
//! and supports configurable retry + constraint relaxation via [`PipelineConfig`].

use rand::SeedableRng;
use rand::rngs::StdRng;
use tracing::{debug, info, info_span, warn};

use crate::entity::plan::{EntityPlan, EntityPlanError};
use crate::entity::planner::{EntityPlanner, SimpleEntityPlanner};
use crate::entity::rules::EntityRule;
use crate::feature::plan::{FeaturePlan, FeaturePlanError};
use crate::feature::planner::{FeaturePlanner, SimpleFeaturePlanner};
use crate::feature::registry::FeatureRegistry;
use crate::feature::rules::FeatureRule;
use crate::geometry::geom::GeometryPlan;
use crate::geometry::planner::{
    GeometryPlanError, GeometryPlanner, PlacementConfig, SimpleGeometryPlanner,
};
use crate::intent::builder::{IntentBuildError, IntentBuilder};
use crate::intent::generic_builder::GenericIntentBuilder;
use crate::intent::map_intent::MapIntent;
use crate::situation::SituationContext;
use crate::spatial::plan::SpatialPlan;
use crate::spatial::planner::{SimpleSpatialPlanner, SpatialPlanError, SpatialPlanner};
use crate::tile::map::TileMap;
use crate::tile::rasterize::{RasterizeError, Rasterizer, SimpleRasterizer};
use crate::tile::registry::TileRegistry;
use crate::tile::scatter::{TileScatterRule, apply_scatter};
use crate::validate::entity::{EntityValidationInput, EntityValidator};
use crate::validate::feature::{FeatureValidationInput, FeatureValidator};
use crate::validate::geometry::GeometryValidator;
use crate::validate::{Severity, Validator};

// ---------------------------------------------------------------------------
// PipelineConfig + RelaxationStrategy
// ---------------------------------------------------------------------------

/// Strategy for relaxing constraints when a geometry retry is triggered.
#[derive(Debug, Clone)]
pub enum RelaxationStrategy {
    /// Increase geometry spacing by `step` tiles each retry.
    IncreaseSpacing {
        /// Number of tiles to add to both `min_gap` and `separation_gap` per retry.
        step: i32,
    },
    /// No relaxation — just retry (useful if RNG gives different results).
    None,
}

/// Configuration knobs for the [`Pipeline`] runner.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Maximum number of retry attempts when geometry validation fails.
    pub max_retries: u32,
    /// Strategy for relaxing constraints on retry.
    pub relaxation: RelaxationStrategy,
    /// Optional seed for deterministic generation. If `None`, a random seed is used.
    pub seed: Option<u64>,
    /// Optional feature rules override. If `None`, loads from the default asset path.
    pub feature_rules: Option<Vec<FeatureRule>>,
    /// Optional entity rules override. If `None`, loads from the default asset path.
    pub entity_rules: Option<Vec<EntityRule>>,
    /// Optional tile registry override. If `None`, uses the default built-in registry.
    pub tile_registry: Option<TileRegistry>,
    /// Optional feature registry override. If `None`, uses the default built-in registry.
    pub feature_registry: Option<FeatureRegistry>,
    /// Optional tile scatter rules override. If `None`, loads from the default asset path.
    pub scatter_rules: Option<Vec<TileScatterRule>>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            relaxation: RelaxationStrategy::IncreaseSpacing { step: 2 },
            seed: None,
            feature_rules: None,
            entity_rules: None,
            tile_registry: None,
            feature_registry: None,
            scatter_rules: None,
        }
    }
}

// ---------------------------------------------------------------------------
// PipelineResult
// ---------------------------------------------------------------------------

/// Holds every intermediate and final output produced by a pipeline run.
#[derive(Debug)]
pub struct PipelineResult {
    /// The structural intent derived from the situation.
    pub intent: MapIntent,
    /// The abstract spatial layout.
    pub spatial: SpatialPlan,
    /// The concrete geometry (room rects + corridor links).
    pub geometry: GeometryPlan,
    /// The rasterised tile map.
    pub tiles: TileMap,
    /// Placed features (chests, altars, traps, …).
    pub features: FeaturePlan,
    /// Placed entities (guards, rats, …).
    pub entities: EntityPlan,
}

// ---------------------------------------------------------------------------
// PipelineError
// ---------------------------------------------------------------------------

/// Unified error type covering every stage of the pipeline.
#[derive(Debug)]
pub enum PipelineError {
    /// Intent building failed.
    Intent(IntentBuildError),
    /// Spatial planning failed.
    Spatial(SpatialPlanError),
    /// Geometry planning failed.
    Geometry(GeometryPlanError),
    /// Tile rasterization failed.
    Rasterize(RasterizeError),
    /// Feature planning failed.
    Feature(FeaturePlanError),
    /// Entity planning failed.
    Entity(EntityPlanError),
    /// A stage's validation failed after exhausting all retries.
    ValidationFailed {
        /// Human-readable name of the stage that failed.
        stage: &'static str,
        /// Error-severity messages collected from the last attempt.
        issues: Vec<String>,
    },
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Intent(e) => write!(f, "intent: {e}"),
            Self::Spatial(e) => write!(f, "spatial: {e}"),
            Self::Geometry(e) => write!(f, "geometry: {e}"),
            Self::Rasterize(e) => write!(f, "rasterize: {e}"),
            Self::Feature(e) => write!(f, "feature: {e}"),
            Self::Entity(e) => write!(f, "entity: {e}"),
            Self::ValidationFailed { stage, issues } => {
                write!(f, "{stage} validation failed: {}", issues.join("; "))
            }
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Intent(e) => Some(e),
            Self::Spatial(e) => Some(e),
            Self::Geometry(e) => Some(e),
            Self::Rasterize(e) => Some(e),
            Self::Feature(e) => Some(e),
            Self::Entity(e) => Some(e),
            Self::ValidationFailed { .. } => None,
        }
    }
}

impl From<IntentBuildError> for PipelineError {
    fn from(e: IntentBuildError) -> Self {
        Self::Intent(e)
    }
}

impl From<SpatialPlanError> for PipelineError {
    fn from(e: SpatialPlanError) -> Self {
        Self::Spatial(e)
    }
}

impl From<GeometryPlanError> for PipelineError {
    fn from(e: GeometryPlanError) -> Self {
        Self::Geometry(e)
    }
}

impl From<RasterizeError> for PipelineError {
    fn from(e: RasterizeError) -> Self {
        Self::Rasterize(e)
    }
}

impl From<FeaturePlanError> for PipelineError {
    fn from(e: FeaturePlanError) -> Self {
        Self::Feature(e)
    }
}

impl From<EntityPlanError> for PipelineError {
    fn from(e: EntityPlanError) -> Self {
        Self::Entity(e)
    }
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// Orchestrates the full generation pipeline with configurable retry and
/// constraint relaxation.
///
/// By default, [`Pipeline::run`] constructs a [`GenericIntentBuilder`] from the
/// embedded asset data. Use [`Pipeline::run_with`] to supply a custom
/// [`IntentBuilder`] (e.g. in tests).
///
/// # Example
///
/// ```
/// use procgen::pipeline::{Pipeline, PipelineConfig};
/// use procgen::demo::crypt::build_crypt_situation;
///
/// let pipeline = Pipeline { config: PipelineConfig { seed: Some(42), ..PipelineConfig::default() } };
/// let situation = build_crypt_situation();
/// let result = pipeline.run(&situation).unwrap();
/// assert!(result.tiles.width > 0);
/// ```
pub struct Pipeline {
    /// Runtime configuration (retries, relaxation strategy).
    pub config: PipelineConfig,
}

impl Pipeline {
    /// Execute the full pipeline using the default [`GenericIntentBuilder`].
    ///
    /// This is the primary entry point. It constructs a
    /// [`GenericIntentBuilder::from_defaults`] internally, then delegates to
    /// [`Pipeline::run_with`].
    pub fn run(&self, situation: &SituationContext) -> Result<PipelineResult, PipelineError> {
        let builder = GenericIntentBuilder::from_defaults()?;
        self.run_with(situation, &builder)
    }

    /// Execute the full pipeline with a caller-supplied [`IntentBuilder`].
    ///
    /// 1. Build intent from [`SituationContext`].
    /// 2. Plan abstract spatial layout.
    /// 3. Plan concrete geometry (with retry + relaxation on validation failure).
    /// 4. Rasterise tiles.
    /// 5. Place features and validate.
    /// 6. Place entities and validate.
    pub fn run_with(
        &self,
        situation: &SituationContext,
        intent_builder: &dyn IntentBuilder,
    ) -> Result<PipelineResult, PipelineError> {
        let _span = info_span!("pipeline").entered();

        // ── 0. Seeded RNG ──────────────────────────────────────────────
        let mut rng: StdRng = match self.config.seed {
            Some(seed) => {
                info!(seed = seed, "using fixed seed");
                StdRng::seed_from_u64(seed)
            }
            None => {
                info!("using entropy-based seed");
                StdRng::from_entropy()
            }
        };

        // ── 1. Intent ──────────────────────────────────────────────────
        info!("building intent");
        let intent = intent_builder.build(situation, &mut rng)?;
        debug!(nodes = intent.structural_graph.nodes.len(), "intent built");

        // ── 2. Spatial planning ────────────────────────────────────────
        info!("spatial planning");
        let spatial = SimpleSpatialPlanner.plan(&intent)?;
        debug!(
            spaces = spatial.spaces.len(),
            links = spatial.links.len(),
            "spatial plan ready"
        );

        // ── 3. Geometry planning (retry loop) ──────────────────────────
        let geometry = self.plan_geometry_with_retries(&spatial)?;

        // ── 4. Rasterisation ───────────────────────────────────────
        info!("rasterising tiles");
        let registry = match &self.config.tile_registry {
            Some(reg) => reg.clone(),
            None => TileRegistry::default_registry(),
        };
        let mut tiles = SimpleRasterizer.rasterize(&geometry, &registry)?;
        debug!(width = tiles.width, height = tiles.height, "tile map ready");

        // ── 4b. Tile scatter ───────────────────────────────────────────
        let scatter_rules = match &self.config.scatter_rules {
            Some(rules) => rules.clone(),
            None => crate::asset::load::load_default_tile_scatter_rules()
                .expect("embedded tile scatter rules are valid JSON"),
        };
        if !scatter_rules.is_empty() {
            info!(rules = scatter_rules.len(), "applying tile scatter");
            apply_scatter(
                &mut tiles,
                &geometry,
                &spatial,
                &scatter_rules,
                &registry,
                &mut rng,
            );
        }

        // ── Resolve rule sets ──────────────────────────────────────────
        let feature_rules = match &self.config.feature_rules {
            Some(rules) => rules.clone(),
            None => crate::asset::load::load_default_feature_rules()
                .expect("embedded feature rules are valid JSON"),
        };
        let entity_rules = match &self.config.entity_rules {
            Some(rules) => rules.clone(),
            None => crate::asset::load::load_default_entity_rules()
                .expect("embedded entity rules are valid JSON"),
        };

        // ── 5. Feature planning + validation ───────────────────────────
        info!("planning features");
        let features =
            SimpleFeaturePlanner.plan(&spatial, &geometry, &tiles, &feature_rules, &mut rng)?;
        self.validate_features(&features, &tiles, &geometry, &spatial)?;

        // ── 6. Entity planning + validation ───────────────────────────
        info!("planning entities");
        let entities = SimpleEntityPlanner.plan(
            &spatial,
            &geometry,
            &tiles,
            &features,
            &entity_rules,
            &mut rng,
        )?;
        self.validate_entities(&entities, &features, &tiles, &geometry, &spatial)?;

        info!("pipeline complete");
        Ok(PipelineResult {
            intent,
            spatial,
            geometry,
            tiles,
            features,
            entities,
        })
    }

    // ── internal helpers ───────────────────────────────────────────────

    /// Run geometry planning with a retry loop governed by [`PipelineConfig`].
    ///
    /// On each retry the [`PlacementConfig`] is adjusted according to the
    /// configured [`RelaxationStrategy`].
    fn plan_geometry_with_retries(
        &self,
        spatial: &SpatialPlan,
    ) -> Result<GeometryPlan, PipelineError> {
        let _span = info_span!("geometry_retry_loop").entered();
        let base_config = PlacementConfig::default();
        let mut last_errors: Vec<String> = Vec::new();

        for attempt in 0..=self.config.max_retries {
            let config = self.relaxed_config(&base_config, attempt);

            info!(
                attempt = attempt,
                min_gap = config.min_gap,
                separation_gap = config.separation_gap,
                "geometry attempt"
            );

            let planner = SimpleGeometryPlanner { config };
            let plan = match planner.plan(spatial) {
                Ok(p) => p,
                Err(e) => {
                    warn!(attempt = attempt, error = %e, "geometry planning error");
                    // A hard planning error (e.g. EmptyPlan) is not retryable.
                    return Err(PipelineError::Geometry(e));
                }
            };

            let result = GeometryValidator::default().validate(&plan);
            log_validation_issues(&result.issues);

            if result.is_ok() {
                info!(attempt = attempt, "geometry validation passed");
                return Ok(plan);
            }

            last_errors = result
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| i.message.clone())
                .collect();

            warn!(
                attempt = attempt,
                errors = last_errors.len(),
                "geometry validation failed, will retry"
            );
        }

        Err(PipelineError::ValidationFailed {
            stage: "geometry",
            issues: last_errors,
        })
    }

    /// Produce a [`PlacementConfig`] adjusted by the configured relaxation
    /// strategy for the given `attempt` number (0-based).
    fn relaxed_config(&self, base: &PlacementConfig, attempt: u32) -> PlacementConfig {
        match &self.config.relaxation {
            RelaxationStrategy::IncreaseSpacing { step } => PlacementConfig {
                min_gap: base.min_gap + (attempt as i32) * step,
                separation_gap: base.separation_gap + (attempt as i32) * step,
            },
            RelaxationStrategy::None => base.clone(),
        }
    }

    /// Validate the [`FeaturePlan`]; returns `PipelineError::ValidationFailed`
    /// on error-severity issues.
    fn validate_features(
        &self,
        features: &FeaturePlan,
        tiles: &TileMap,
        geometry: &GeometryPlan,
        spatial: &SpatialPlan,
    ) -> Result<(), PipelineError> {
        let result = FeatureValidator.validate(&FeatureValidationInput {
            features,
            tiles,
            geometry,
            spatial,
        });
        log_validation_issues(&result.issues);

        if result.is_ok() {
            info!("feature validation passed");
            Ok(())
        } else {
            let issues: Vec<String> = result
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| i.message.clone())
                .collect();
            Err(PipelineError::ValidationFailed {
                stage: "feature",
                issues,
            })
        }
    }

    /// Validate the [`EntityPlan`]; returns `PipelineError::ValidationFailed`
    /// on error-severity issues.
    fn validate_entities(
        &self,
        entities: &EntityPlan,
        features: &FeaturePlan,
        tiles: &TileMap,
        geometry: &GeometryPlan,
        spatial: &SpatialPlan,
    ) -> Result<(), PipelineError> {
        let result = EntityValidator.validate(&EntityValidationInput {
            entities,
            features,
            tiles,
            geometry,
            spatial,
        });
        log_validation_issues(&result.issues);

        if result.is_ok() {
            info!("entity validation passed");
            Ok(())
        } else {
            let issues: Vec<String> = result
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| i.message.clone())
                .collect();
            Err(PipelineError::ValidationFailed {
                stage: "entity",
                issues,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Log each validation issue at the appropriate tracing level.
fn log_validation_issues(issues: &[crate::validate::ValidationIssue]) {
    for issue in issues {
        match issue.severity {
            Severity::Error => warn!(msg = %issue.message, "validation error"),
            Severity::Warning => warn!(msg = %issue.message, "validation warning"),
            Severity::Info => debug!(msg = %issue.message, "validation info"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = PipelineConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert!(cfg.seed.is_none());
        assert!(cfg.feature_rules.is_none());
        assert!(cfg.entity_rules.is_none());
        match &cfg.relaxation {
            RelaxationStrategy::IncreaseSpacing { step } => assert_eq!(*step, 2),
            RelaxationStrategy::None => panic!("expected IncreaseSpacing"),
        }
    }

    #[test]
    fn relaxed_config_no_relaxation() {
        let pipeline = Pipeline {
            config: PipelineConfig {
                max_retries: 1,
                relaxation: RelaxationStrategy::None,
                seed: None,
                feature_rules: None,
                entity_rules: None,
                tile_registry: None,
                feature_registry: None,
                scatter_rules: None,
            },
        };
        let base = PlacementConfig::default();
        let adjusted = pipeline.relaxed_config(&base, 3);
        assert_eq!(adjusted.min_gap, base.min_gap);
        assert_eq!(adjusted.separation_gap, base.separation_gap);
    }

    #[test]
    fn relaxed_config_increase_spacing() {
        let pipeline = Pipeline {
            config: PipelineConfig {
                max_retries: 5,
                relaxation: RelaxationStrategy::IncreaseSpacing { step: 3 },
                seed: None,
                feature_rules: None,
                entity_rules: None,
                tile_registry: None,
                feature_registry: None,
                scatter_rules: None,
            },
        };
        let base = PlacementConfig {
            min_gap: 2,
            separation_gap: 6,
        };
        let adjusted = pipeline.relaxed_config(&base, 2);
        assert_eq!(adjusted.min_gap, 2 + 2 * 3);
        assert_eq!(adjusted.separation_gap, 6 + 2 * 3);
    }

    #[test]
    fn pipeline_error_display() {
        let err = PipelineError::ValidationFailed {
            stage: "geometry",
            issues: vec!["overlap A-B".to_string(), "gap too small".to_string()],
        };
        let msg = format!("{err}");
        assert!(msg.contains("geometry"));
        assert!(msg.contains("overlap A-B"));
        assert!(msg.contains("gap too small"));
    }

    #[test]
    fn pipeline_error_from_conversions() {
        // Verify From impls compile and produce the right variant.
        let _: PipelineError = IntentBuildError::InvalidSituation("bad".into()).into();
        let _: PipelineError = RasterizeError::EmptyLayout.into();
    }
}
