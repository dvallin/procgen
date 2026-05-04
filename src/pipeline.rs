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

use crate::atmosphere::apply::apply_atmosphere_influences;
use crate::atmosphere::profile::AtmosphereProfile;
use crate::entity::plan::{EntityPlan, EntityPlanError};
use crate::entity::planner::{EntityPlanner, SimpleEntityPlanner};
use crate::entity::rules::EntityRule;
use crate::feature::plan::{FeaturePlan, FeaturePlanError};
use crate::feature::planner::{FeaturePlanner, SimpleFeaturePlanner};
use crate::feature::registry::FeatureRegistry;
use crate::feature::rules::FeatureRule;
use crate::geometry::force_directed::{ForceDirectedConfig, ForceDirectedGeometryPlanner};
use crate::geometry::geom::GeometryPlan;
use crate::geometry::planner::{
    ColumnGeometryPlanner, GeometryPlanError, GeometryPlanner, PlacementConfig,
};
use crate::intent::builder::{IntentBuildError, IntentBuilder};
use crate::intent::generic_builder::GenericIntentBuilder;
use crate::intent::map_intent::MapIntent;
use crate::interior::builder::build_interior_plans;
use crate::interior::paths::{compute_reserved_paths, find_room_doors};
use crate::interior::plan::InteriorPlan;
use crate::interior::template::InteriorTemplate;
use crate::situation::SituationContext;
use crate::spatial::plan::SpatialPlan;
use crate::spatial::planner::{SimpleSpatialPlanner, SpatialPlanError, SpatialPlanner};
use crate::tile::map::TileMap;
use crate::tile::rasterize::{RasterizeError, Rasterizer, SimpleRasterizer};
use crate::tile::registry::TileRegistry;
use crate::tile::scatter::scatter_room;
use crate::validate::entity::{EntityValidationInput, EntityValidator};
use crate::validate::feature::{FeatureValidationInput, FeatureValidator};
use crate::validate::geometry::GeometryValidator;
use crate::validate::{Severity, Validator};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// PipelineConfig + RelaxationStrategy
// ---------------------------------------------------------------------------

/// Which geometry planner implementation to use.
#[derive(Debug, Clone)]
pub enum GeometryStrategy {
    /// BFS column layout (the original `SimpleGeometryPlanner`).
    /// Good for small maps (3–6 rooms) with linear or tree-like graphs.
    Column,
    /// Force-directed simulation layout.
    /// Better for medium-to-large maps (7–15+ rooms) with complex topology.
    ForceDirected,
}

impl Default for GeometryStrategy {
    fn default() -> Self {
        Self::ForceDirected
    }
}

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
    /// Optional atmosphere profiles override. If `None`, loads from the default asset path.
    pub atmosphere_profiles: Option<Vec<AtmosphereProfile>>,
    /// Number of weighted samples to draw from each atmosphere palette per room.
    /// Higher values produce denser, more varied atmosphere-driven content.
    /// Default: 3.
    pub atmosphere_sample_budget: usize,
    /// Optional interior templates override. If `None`, loads from the default asset path.
    pub interior_templates: Option<Vec<InteriorTemplate>>,

    // ── Additive rule injection (merged with defaults, never replacing) ──
    /// Additional feature rules merged *after* the base rules (default or override).
    /// Use this to layer quest/event-specific features without replacing the entire ruleset.
    pub additional_feature_rules: Vec<FeatureRule>,
    /// Additional entity rules merged *after* the base rules (default or override).
    /// Use this to layer quest/event-specific entities without replacing the entire ruleset.
    pub additional_entity_rules: Vec<EntityRule>,
    /// Additional atmosphere profiles merged *after* the base profiles (default or override).
    /// Use this to inject temporary quest/event effects that activate on tag matches.
    pub additional_atmospheres: Vec<AtmosphereProfile>,
    /// Which geometry planner to use. Default: `ForceDirected`.
    pub geometry_strategy: GeometryStrategy,
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
            atmosphere_profiles: None,
            atmosphere_sample_budget: 3,
            interior_templates: None,
            additional_feature_rules: Vec::new(),
            additional_entity_rules: Vec::new(),
            additional_atmospheres: Vec::new(),
            geometry_strategy: GeometryStrategy::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// PipelineResult
// ---------------------------------------------------------------------------

/// A named entity placed in a specific room (from situation directives).
#[derive(Debug, Clone)]
pub struct NamedEntityAnnotation {
    /// The entity archetype ID.
    pub archetype: crate::entity::plan::EntityArchetypeId,
    /// The unique display name.
    pub name: String,
    /// The tile position where the entity was placed.
    pub position: crate::geometry::geom::Point,
}

/// Annotation for a single room in the generated map.
///
/// Carries metadata that a world engine can use to correlate map output
/// with narrative graph nodes — room labels, roles, positions, and doors.
#[derive(Debug, Clone)]
pub struct RoomAnnotation {
    /// Internal space ID (matches [`SpaceId`] in [`SpatialPlan`]).
    pub space_id: crate::spatial::plan::SpaceId,
    /// Human-readable label (from vocabulary entry, e.g. "Ossuary").
    pub label: Option<String>,
    /// Narrative role of this room.
    pub role: crate::intent::graph::NodeRole,
    /// Structural tags on this room (e.g. "main_goal", "locked").
    pub structural_tags: Vec<crate::tag::Tag>,
    /// Bounding rectangle of the room in tile coordinates.
    pub rect: crate::geometry::geom::Rect,
    /// Door positions on the room boundary.
    pub doors: Vec<crate::geometry::geom::Point>,
    /// True if this room is the dungeon entry point.
    pub is_entry: bool,
    /// True if this room is the primary goal (has "main_goal" structural tag).
    pub is_goal: bool,
    /// Assigned letter for map overlay rendering (A, B, C, …).
    pub letter: char,
    /// Center point of the room (for letter overlay positioning).
    pub center: crate::geometry::geom::Point,
    /// Named entities placed in this room (from situation directives).
    pub named_entities: Vec<NamedEntityAnnotation>,
}

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
    /// Interior plans per room (zones, doors, reserved paths).
    pub interiors: Vec<InteriorPlan>,
    /// Room annotations for world-engine correlation.
    pub annotations: Vec<RoomAnnotation>,
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

        // ── 3. Geometry planning (retry loop) ──────────────────────
        let geometry = self.plan_geometry_with_retries(&spatial, &mut rng)?;

        // ── 4. Rasterisation ───────────────────────────────────────
        info!("rasterising tiles");
        let registry = match &self.config.tile_registry {
            Some(reg) => reg.clone(),
            None => TileRegistry::default_registry(),
        };
        let mut tiles = SimpleRasterizer.rasterize(&geometry, &registry)?;
        debug!(width = tiles.width, height = tiles.height, "tile map ready");

        // ── 4b. Pre-compute reserved paths for scatter safety ──────────
        let reserved_per_room: HashMap<
            crate::spatial::plan::SpaceId,
            HashSet<crate::geometry::geom::Point>,
        > = geometry
            .spaces
            .iter()
            .map(|placed| {
                let doors = find_room_doors(&tiles, placed.rect);
                let (_, reserved) = compute_reserved_paths(&tiles, placed.rect, &doors);
                (placed.space_id, reserved)
            })
            .collect();

        // ── 5. Atmosphere (scatter + per-room rules) ───────────────
        let (per_room_feature_rules, per_room_entity_rules) = self.apply_atmosphere(
            &spatial,
            &geometry,
            &mut tiles,
            &registry,
            &mut rng,
            &reserved_per_room,
        );

        // ── 5b. Interior planning ──────────────────────────────────────
        info!("computing interior plans");
        let interiors = build_interior_plans(&geometry, &tiles);
        debug!(rooms = interiors.len(), "interior plans ready");

        // ── 6. Feature planning + validation ───────────────────────
        info!("planning features");
        let feature_rules = self.resolve_feature_rules();
        let interior_templates = self.resolve_interior_templates();
        let features = SimpleFeaturePlanner.plan(
            &spatial,
            &geometry,
            &tiles,
            &feature_rules,
            &per_room_feature_rules,
            &interiors,
            &interior_templates,
            &mut rng,
        )?;
        self.validate_features(&features, &tiles, &geometry, &spatial)?;

        // ── 7. Entity planning + validation ────────────────────────
        info!("planning entities");
        let entity_rules = self.resolve_entity_rules();
        let entities = SimpleEntityPlanner.plan(
            &spatial,
            &geometry,
            &tiles,
            &features,
            &entity_rules,
            &per_room_entity_rules,
            &situation.directives,
            &mut rng,
        )?;
        self.validate_entities(&entities, &features, &tiles, &geometry, &spatial)?;

        // ── 8. Build annotations ─────────────────────────────────────────────
        info!("building annotations");
        let annotations = Self::build_annotations(&spatial, &geometry, &tiles, &entities);
        debug!(count = annotations.len(), "annotations ready");

        info!("pipeline complete");
        Ok(PipelineResult {
            intent,
            spatial,
            geometry,
            tiles,
            features,
            entities,
            interiors,
            annotations,
        })
    }

    // ── internal helpers ─────────────────────────────────────────────────────

    /// Build room annotations from the spatial plan and geometry.
    ///
    /// Each room gets a letter (A, B, C, …), its center point, label, role,
    /// structural tags, bounding rect, door positions, and Entry/Goal flags.
    fn build_annotations(
        spatial: &SpatialPlan,
        geometry: &GeometryPlan,
        tiles: &TileMap,
        entities: &EntityPlan,
    ) -> Vec<RoomAnnotation> {
        use crate::intent::graph::NodeRole;
        use crate::interior::paths::find_room_doors;
        use crate::tag::Tag;

        let spec_map: std::collections::HashMap<
            crate::spatial::plan::SpaceId,
            &crate::spatial::plan::SpaceSpec,
        > = spatial.spaces.iter().map(|s| (s.id, s)).collect();

        let mut annotations = Vec::new();
        let mut letter = b'A';

        for placed in &geometry.spaces {
            let Some(spec) = spec_map.get(&placed.space_id) else {
                continue;
            };

            let doors = find_room_doors(tiles, placed.rect);
            let center = crate::geometry::geom::Point {
                x: placed.rect.x + placed.rect.w / 2,
                y: placed.rect.y + placed.rect.h / 2,
            };

            let is_entry = spec.role == NodeRole::Entry;
            let is_goal = spec.structural_tags.contains(&Tag::from("main_goal"));

            let assigned_letter = letter as char;
            if letter < b'Z' {
                letter += 1;
            }

            // Collect named entities for this room.
            let named_entities: Vec<NamedEntityAnnotation> = entities
                .entities
                .iter()
                .filter(|e| e.space_id == placed.space_id && e.name.is_some())
                .map(|e| NamedEntityAnnotation {
                    archetype: e.archetype.clone(),
                    name: e.name.clone().unwrap(),
                    position: e.position,
                })
                .collect();

            annotations.push(RoomAnnotation {
                space_id: placed.space_id,
                label: spec.label.clone(),
                role: spec.role,
                structural_tags: spec.structural_tags.clone(),
                rect: placed.rect,
                doors,
                is_entry,
                is_goal,
                letter: assigned_letter,
                center,
                named_entities,
            });
        }

        annotations
    }

    /// Run geometry planning with a retry loop governed by [`PipelineConfig`].
    ///
    /// On each retry the [`PlacementConfig`] is adjusted according to the
    /// configured [`RelaxationStrategy`].
    fn plan_geometry_with_retries(
        &self,
        spatial: &SpatialPlan,
        rng: &mut StdRng,
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
                strategy = ?self.config.geometry_strategy,
                "geometry attempt"
            );

            let plan = match &self.config.geometry_strategy {
                GeometryStrategy::Column => {
                    let planner = ColumnGeometryPlanner { config };
                    planner.plan(spatial, rng)
                }
                GeometryStrategy::ForceDirected => {
                    let planner = ForceDirectedGeometryPlanner {
                        config: ForceDirectedConfig {
                            placement: config,
                            ..ForceDirectedConfig::default()
                        },
                    };
                    planner.plan(spatial, rng)
                }
            };

            let plan = match plan {
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

    /// Resolve feature rules from config or defaults, then append any additional rules.
    fn resolve_feature_rules(&self) -> Vec<FeatureRule> {
        let mut rules = match &self.config.feature_rules {
            Some(rules) => rules.clone(),
            None => crate::asset::load::load_default_feature_rules()
                .expect("embedded feature rules are valid JSON"),
        };
        rules.extend(self.config.additional_feature_rules.iter().cloned());
        rules
    }

    /// Resolve entity rules from config or defaults, then append any additional rules.
    fn resolve_entity_rules(&self) -> Vec<EntityRule> {
        let mut rules = match &self.config.entity_rules {
            Some(rules) => rules.clone(),
            None => crate::asset::load::load_default_entity_rules()
                .expect("embedded entity rules are valid JSON"),
        };
        rules.extend(self.config.additional_entity_rules.iter().cloned());
        rules
    }

    /// Resolve interior templates from config or defaults.
    fn resolve_interior_templates(&self) -> Vec<InteriorTemplate> {
        match &self.config.interior_templates {
            Some(templates) => templates.clone(),
            None => crate::asset::load::load_default_interior_templates()
                .expect("embedded interior templates are valid JSON"),
        }
    }

    /// Apply atmosphere profiles: sample contributions per room, apply scatter
    /// to tiles, and return per-room feature/entity rules for downstream planners.
    fn apply_atmosphere(
        &self,
        spatial: &SpatialPlan,
        geometry: &GeometryPlan,
        tiles: &mut TileMap,
        registry: &TileRegistry,
        rng: &mut StdRng,
        reserved_per_room: &HashMap<
            crate::spatial::plan::SpaceId,
            HashSet<crate::geometry::geom::Point>,
        >,
    ) -> (
        HashMap<crate::spatial::plan::SpaceId, Vec<FeatureRule>>,
        HashMap<crate::spatial::plan::SpaceId, Vec<EntityRule>>,
    ) {
        let mut atmosphere_profiles = match &self.config.atmosphere_profiles {
            Some(profiles) => profiles.clone(),
            None => crate::asset::load::load_default_atmosphere_profiles()
                .expect("embedded atmosphere profiles are valid JSON"),
        };
        atmosphere_profiles.extend(self.config.additional_atmospheres.iter().cloned());

        let mut per_room_feature_rules: HashMap<crate::spatial::plan::SpaceId, Vec<FeatureRule>> =
            HashMap::new();
        let mut per_room_entity_rules: HashMap<crate::spatial::plan::SpaceId, Vec<EntityRule>> =
            HashMap::new();

        if atmosphere_profiles.is_empty() {
            return (per_room_feature_rules, per_room_entity_rules);
        }

        let _span = info_span!(
            "atmosphere_planning",
            profiles = atmosphere_profiles.len(),
            rooms = spatial.spaces.len(),
        )
        .entered();

        info!(
            profiles = atmosphere_profiles.len(),
            "applying atmosphere profiles"
        );

        // Pass 1: sample atmosphere contributions for every room.
        let mut per_room_scatter: HashMap<
            crate::spatial::plan::SpaceId,
            Vec<crate::tile::scatter::TileScatterRule>,
        > = HashMap::new();

        for spec in &spatial.spaces {
            let contrib = apply_atmosphere_influences(
                &atmosphere_profiles,
                spec,
                self.config.atmosphere_sample_budget,
                rng,
            );

            if !contrib.scatter_rules.is_empty() {
                per_room_scatter.insert(spec.id, contrib.scatter_rules);
            }
            if !contrib.feature_rules.is_empty() {
                per_room_feature_rules.insert(spec.id, contrib.feature_rules);
            }
            if !contrib.entity_rules.is_empty() {
                per_room_entity_rules.insert(spec.id, contrib.entity_rules);
            }
        }

        // Pass 2: apply scatter rules per room (iterating geometry spaces
        // to preserve the original RNG consumption order).
        for placed in &geometry.spaces {
            if let Some(rules) = per_room_scatter.get(&placed.space_id) {
                let reserved = reserved_per_room
                    .get(&placed.space_id)
                    .cloned()
                    .unwrap_or_default();
                scatter_room(tiles, placed.rect, rules, registry, rng, &reserved);
            }
        }

        let matched_rooms = per_room_scatter
            .len()
            .max(per_room_feature_rules.len())
            .max(per_room_entity_rules.len());
        info!(
            matched_rooms = matched_rooms,
            total_rooms = spatial.spaces.len(),
            "atmosphere matching complete"
        );

        (per_room_feature_rules, per_room_entity_rules)
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
        assert!(cfg.interior_templates.is_none());
        assert!(cfg.additional_feature_rules.is_empty());
        assert!(cfg.additional_entity_rules.is_empty());
        assert!(cfg.additional_atmospheres.is_empty());
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
                ..PipelineConfig::default()
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
                ..PipelineConfig::default()
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
