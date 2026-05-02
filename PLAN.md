# PLAN.md — Roadmap

## Vision

A story-driven procedural generator where narrative signals steer every decision. Given a `SituationContext` ("sealed crypt, noble family, locked vault"), the system infers structure, materializes themed rooms, places narratively appropriate features and entities, and produces a map that feels *designed* — with pacing, tension, and coherence — without any hand-authored graph.

---

## Where We Are

The pipeline skeleton is complete and validated. Two demo scenarios flow through all layers and produce ASCII output. 213 tests (property-based + integration) confirm layer invariants hold for arbitrary inputs. Seeded RNG ensures reproducibility. **Phase 1 (Data-Driven Rules & Asset Loading) is complete.**

```
SituationContext → IntentBuilder → MapIntent → SpatialPlan → GeometryPlan → TileMap → FeaturePlan → EntityPlan → ASCII
```

**What works:**
- Full pipeline runner with retry semantics and seeded RNG
- Geometry placement with constraint-aware layout and corridor routing
- Feature placement (furniture, traps, decorations) driven by role/tag rules
- Entity placement (monsters, NPCs) with patrol zones and safe entry rooms
- Validators at geometry, feature, and entity levels
- Property-based stress tests with random spatial plans
- Two regression scenarios: Noble Crypt (lock-and-key), Tavern Cellar (branching + secret)
- **Data-driven rules**: Feature/entity rules loaded from JSON assets (`assets/rules/`)
- **Asset loader**: Generic `load_rules<T>()` with file + embedded fallback
- **Pipeline rule overrides**: `PipelineConfig.feature_rules` / `.entity_rules` let callers inject custom rule sets
- **Proptest coverage**: Random rule sets (arbitrary roles, tags, counts) never cause panics

**What's missing:**
- The `IntentBuilder` is hard-coded per scenario — no generative inference from situation
- Theme/variety is baked in — no vocabulary system, no atmosphere-driven selection
- Tile types are a fixed enum — can't express game-specific terrain
- Maps are structurally correct but narratively flat — no pacing, no tension curve
- No world layer — rule set selection is manual (will be driven by world context in the future)

---

## Design Boundary: Types vs. Data

A core tension in this system: what stays as Rust enums/traits (compile-time, exhaustive) vs. what becomes data-driven (runtime, open-ended)?

**Stays typed (structural contracts):**
- `NodeRole` — the finite set of *structural functions* a room can serve (Entry, Hub, Gate, Goal, Reward, Branch, Transition). These are architectural concepts, not content.
- `EdgeRole` — traversal semantics determine corridor generation logic.
- Pipeline traits (`IntentBuilder`, `SpatialPlanner`, etc.) — the layer boundaries.
- `Severity` — validators need exhaustive match arms.
- `Tile` — *for now*. See Phase 3 for the path to data-driven tiles.

**Becomes data (content/variety):**
- Feature/entity rules — which features spawn in which rooms, driven by tags
- Theme vocabularies — labels, tags, atmosphere, archetype selection per theme
- Narrative patterns — reusable graph structures (lock-and-key, hub-and-spoke)
- Room templates — pre-designed interior layouts for specific archetype+theme combos
- Tile palettes — *eventually*. A "tile registry" maps tile IDs to properties (walkable, opaque, ASCII char). But the MVP can keep the enum.

**The heuristic:** If adding a new *kind* of X requires changing match arms in multiple files, X should probably be data. If X determines which *code path* executes, it should stay typed.

---

## Phased Roadmap

### Phase 1: Data-Driven Rules & Asset Loading ✅

**Goal:** Extract hard-coded feature/entity rules into JSON assets. The rule engine becomes generic — rules are data, matching logic is code.

| # | Task | Status | Description |
|---|---|---|---|
| 1.1 | **FeatureRule JSON schema** | ✅ | Serde derives on `FeatureRule`, `FeatureKind`, `PlacementStrategy`, `NodeRole`, `SpaceArchetype`, `Tag`. Round-trip tests. |
| 1.2 | **EntityRule JSON schema** | ✅ | Serde derives on `EntityRule`, `EntityPlacementStrategy`, `EntityArchetypeId`. Round-trip tests. |
| 1.3 | **Asset loader** | ✅ | `load_rules<T>()`, `load_rules_from_str<T>()`, `load_rules_with_fallback<T>()`. `AssetLoadError` with Io/Parse variants. Embedded defaults via `include_str!`. |
| 1.4 | **Default rule assets** | ✅ | `assets/rules/features.json` (10 rules) + `assets/rules/entities.json` (7 rules). `default_rules()` functions are thin loaders. |
| 1.5 | **Pipeline accepts rule sets** | ✅ | `PipelineConfig.feature_rules` / `.entity_rules` overrides. Planner traits accept `rules: &[Rule]` parameter. |
| 1.6 | **Proptest: random rules** | ✅ | 4 property-based tests generate arbitrary rule sets (0–10 rules, random roles/tags/counts) and confirm no panics + valid output. |

**Exit criterion met:** `cargo run` produces identical output with rules loaded from JSON. Users can modify JSON and see different feature/entity behavior without recompiling. Random rules never panic.

**Future integration:** Rule sets are currently loaded as a pipeline-level concern (`PipelineConfig`). In the future, a **world layer** above `SituationContext` will select which rule files to load based on regional themes (e.g., undead region → `undead_crypts.json`, port city → `urban_cellars.json`). The seam is already clean — `PipelineConfig` is where a world layer injects its choices.

---

### Phase 2: Narrative Patterns & Intent Generation

**Goal:** Replace hard-coded `IntentBuilder` fixtures with a generative system that infers a `ScenarioGraph` from situation tags.

| # | Task | Description |
|---|---|---|
| 2.1 | **NarrativePattern data model** | A pattern is a parameterized graph template: named slots with role constraints, edges with traversal types, metadata (what tags vote for this pattern). JSON-serializable. |
| 2.2 | **Pattern library** | 4–5 starter patterns: "lock-and-key" (Entry→Hub→Gate→Goal + optional branches), "branching-exploration" (Hub with N spokes), "linear-descent" (chain of increasing danger), "hub-and-spoke" (central hub, radiating paths), "gauntlet" (linear + gates). |
| 2.3 | **Pattern Selector** | Situation tags have weighted votes for patterns. `locked_vault` → lock-and-key (+3). `open_cavern` → hub-and-spoke (+2). Highest-scoring pattern wins (with RNG tiebreaking). |
| 2.4 | **Theme Vocabulary** | Data model: maps `(role, theme_binding)` → list of `{label, tags, archetype}` tuples. E.g. theme `undead_nobility`: Hub → ("Great Hall", [noble, sealed], Hall). JSON asset. |
| 2.5 | **Slot Filler** | Given a pattern + vocabulary, instantiate each slot: pick a vocabulary entry matching the slot's role, assign label/tags/archetype. RNG for variety across runs. |
| 2.6 | **Constraint Inference** | Pattern metadata declares constraint shapes. Lock-and-key → emit `MustGate`. Hub-and-spoke with required spoke → emit `MustConnect`. |
| 2.7 | **GenericIntentBuilder** | Implement `IntentBuilder` using selector + filler + constraint inference. One builder for all situations. |
| 2.8 | **Regression tests** | Given crypt/tavern situations, `GenericIntentBuilder` produces graphs with the same structural shape (same role counts, same edge types) as the fixtures. |

**Design decisions:**
- Patterns are *topological*, not geometric. They define what connects to what, not where.
- Theme vocabularies are the primary variety knob. Adding a scenario = adding vocabulary entries + situation tags.
- Existing demo fixtures (`CryptIntentBuilder`, `TavernIntentBuilder`) remain as regression baselines — they're never deleted.

**Exit criterion:** `cargo run` with a `GenericIntentBuilder` + crypt situation produces a valid, thematically coherent dungeon indistinguishable in quality from the fixture version.

---

### Phase 3: Tile Registry & Data-Driven Rasterization

**Goal:** Break the hard-coded `Tile` enum into a registry-based system so game-specific terrain can be defined in data.

| # | Task | Description |
|---|---|---|
| 3.1 | **TileId newtype** | `TileId(u16)` — a lightweight handle into the tile registry. `TileMap` stores `Vec<TileId>` instead of `Vec<Tile>`. |
| 3.2 | **TileRegistry** | Maps `TileId` → `TileProperties { name, walkable, opaque, ascii_char, tags }`. Loaded from JSON. Built-in defaults cover the current enum variants. |
| 3.3 | **Migrate Tile enum → TileId** | `Tile::Floor` becomes `TileId(1)` etc. All code that pattern-matches on `Tile` now queries registry properties (`registry.is_walkable(id)`). |
| 3.4 | **Rasterizer uses registry** | `SimpleRasterizer` receives `&TileRegistry` (via pipeline config or trait method). Places `TileId`s instead of enum variants. |
| 3.5 | **Extended tile palette** | Add tiles for Water, Pit, Stairs, Rubble, Grass — each with properties. Demonstrate in a new "cave" scenario context. |
| 3.6 | **ASCII renderer uses registry** | `render_ascii` queries `TileRegistry` for the display character instead of matching an enum. |
| 3.7 | **Backward compat** | Keep a convenience `Tile` enum as a named constant set (like `Tile::FLOOR -> TileId(1)`) so existing tests don't break catastrophically. |

**Design decisions:**
- `NodeRole` and `EdgeRole` do NOT get this treatment — they determine which *code path* runs (corridor routing, entry room entity skip, etc.). They stay as enums.
- `TileId` is tiny (u16) and Copy — no performance regression vs. the enum.
- The registry is a game-level concern: different games using this generator can define different tile sets.
- Feature/entity placement strategies query `registry.is_walkable(tile)` instead of matching enum variants.

**Exit criterion:** Current demos produce identical ASCII output. A new tile type can be added with zero Rust code changes — just a JSON entry.

---

### Phase 4: Narrative Room Character & Atmosphere

**Goal:** Rooms feel different based on narrative context, not just structural role.

| # | Task | Description |
|---|---|---|
| 4.1 | **Atmosphere tags on SpaceSpec** | Add `atmosphere: Vec<Tag>` to `SpaceSpec`. Populated by the slot filler from theme vocabulary (e.g. theme "damp_cellar" → atmosphere: [musty, dripping]). |
| 4.2 | **Feature rules conditioned on atmosphere** | `FeatureRule` gains `match_atmosphere: Option<Tag>`. "damp" rooms get puddle decorations, "arcane" rooms get glowing runes. |
| 4.3 | **Entity rules conditioned on atmosphere/motif** | `EntityRule` gains `match_motif: Option<MotifId>`. Motif "timber" → rats more likely; motif "gothic" → undead more likely. |
| 4.4 | **Room size by narrative importance** | Rooms tagged `main_goal` or `climax` get larger `SizeHint`. Optional/branch rooms get smaller. Spatial planner reads these tags. |
| 4.5 | **Room templates (prefab interiors)** | For specific `(archetype, theme)` combos, provide a pre-designed feature layout as a JSON template. E.g. "library" archetype + "arcane" theme → shelves along walls, desk at center. Template interiors override rule-based placement for that room. |
| 4.6 | **New Scenario: Wizard's Tower** | 3–4 vertical levels connected by shafts. Tests `VerticalTraversal`, `Shaft` archetype, arcane theme vocabulary. Proves the system handles non-dungeon structures. |

**Exit criterion:** Three scenarios (crypt, tavern, tower) with visibly different room character. Room templates work for at least one archetype.

---

### Phase 5: Pacing, Composition & Scale

**Goal:** Generate larger maps with narrative pacing — buildup, climax, release.

| # | Task | Description |
|---|---|---|
| 5.1 | **Pattern Composition** | A slot in one pattern can expand into a sub-pattern. `MapScale` controls expansion budget: Tiny=1 pattern, Small=1+1 expansion, Medium=1+2, Large=recursive. |
| 5.2 | **Pacing Curve** | Assign tension scores to rooms by graph distance from Entry. Tag rooms with `tension: low/medium/high/climax`. |
| 5.3 | **Tension-aware entity density** | Entity rules gain `match_tension: Option<TensionLevel>`. High-tension rooms get more/harder entities. Low-tension rooms stay sparse. |
| 5.4 | **Rest points** | If critical path length > N, insert a `Reward`-role room mid-path (safe, contains supplies). Pattern-level insertion. |
| 5.5 | **Pacing validator** | Warns if the tension curve is flat (boring), immediately maxed (unfair), or has no climax before the goal. |
| 5.6 | **New Scenario: Abandoned Mine** | Medium-scale (10–15 rooms). Main shaft (linear descent) + branching galleries (hub-and-spoke at depth). Tests pattern composition + pacing over a longer path. |

**Exit criterion:** Medium-scale maps have measurable pacing curves visible in `--trace` output. Validator catches degenerate structures.

---

### Phase 6: Algorithmic Depth (scenario-driven)

**Goal:** Improve generation quality at individual layers, motivated by concrete scenario needs.

| Scenario Need | Layer | Approach |
|---|---|---|
| Caves feel rectangular | Geometry | Cellular automata / irregular polygons for `Cave` LocationKind |
| Corridors are monotonous | Routing | A* with cost maps; wider corridors for Hall connections |
| Furniture feels random | Features | Template interiors (Phase 4.5) for more room types |
| No encounter design | Entities | Encounter budgets per room (difficulty ≤ tension × budget) |
| Can't detect boring maps | Validation | Branching factor check, dead-end density, reachability analysis |
| Tower needs vertical movement | Geometry | Multi-level support: `PlacedSpace` gains `z_level`, stairwell routing |

**Priority is driven by which new scenario needs it.** No algorithm work happens without a scenario that exercises it.

---

### Scenario Roadmap

| Scenario | Key Tests | Unlocked by |
|---|---|---|
| Noble Crypt | lock-and-key, undead theme | Foundation ✅ |
| Tavern Cellar | branching, urban theme, secrets | Foundation ✅ |
| Wizard's Tower | vertical traversal, arcane theme, multi-level | Phase 4 |
| Abandoned Mine | pattern composition, medium scale, natural | Phase 5 |
| Thieves' Guild | complex hub-and-spoke, traps, NPC dialogue | Phase 5 |
| Dragon's Lair | large scale, encounter budgets, boss room | Phase 6 |

---

## Architecture Reference

### Pipeline Data Flow

```
SituationContext            (narrative tags + bindings)
    │
    ▼
MapIntent                   (structural graph, scale, constraints, motifs)
    │
    ▼
SpatialPlan                 (abstract spaces + links + size hints)
    │
    ▼
GeometryPlan                (placed rects + corridor polylines)
    │
    ▼
TileMap                     (2D grid of TileIds)
    │
    ▼
FeaturePlan                 (placed furniture/traps/decorations)
    │
    ▼
EntityPlan                  (placed monsters/NPCs with behavior)
    │
    ▼
Validation → retry loop if errors
    │
    ▼
ASCII / Export
```

### Key Types (structural — stay as enums)

```rust
enum NodeRole    { Entry, Hub, Gate, Goal, Reward, Branch, Transition }
enum EdgeRole    { Traversal, OptionalTraversal, RestrictedTraversal, SecretTraversal, VerticalTraversal }
enum Severity    { Error, Warning, Info }
```

### Key Types (content — becoming data-driven)

```rust
// Currently enums, migrating to data:
enum FeatureKind { Furniture, Container, Trap, Decoration, Interactable }  // → JSON rules
enum Tile        { Void, Floor, Wall, Door, LockedDoor }                   // → TileRegistry (Phase 3)

// Already data-driven (string newtypes):
struct Tag(String)
struct EntityArchetypeId(String)
struct MotifId(String)
```

### Data Flow Rules

- **Down is default.** Each layer's output is the next layer's input.
- **Cross-references are explicit.** FeaturePlan receives `SpatialPlan + GeometryPlan + TileMap`.
- **Validators trigger reruns, not mutations.** On failure, constraints relax and the layer re-executes.

---

## Design Decisions

### Why types vs. data?

Types (enums) are for things that change the *control flow* — `NodeRole::Entry` triggers the "skip entities" logic, `EdgeRole::RestrictedTraversal` triggers locked-door placement. These need exhaustive matching.

Data (JSON) is for things that change the *content* — which label a room gets, which features spawn, what entities appear. These should be open-ended without recompilation.

### Why patterns + vocabularies (not LLM prompts)?

Deterministic, testable, seed-reproducible. An LLM could *produce* patterns and vocabularies as content authoring, but the runtime generation must be fully deterministic given a seed.

### Why separate tiles from features?

Tiles define *terrain* (affects pathfinding, line-of-sight). Features define *objects on terrain* (can block movement but are semantically different). A chest is not a tile — it's a feature placed on a floor tile.

### Why property-based testing matters here?

The system is becoming highly compositional: random patterns × random vocabularies × random seeds × random rules = exponential combinations. You can't hand-write enough integration tests. Proptest generates arbitrary valid inputs and asserts invariants (connectivity, no overlaps, safe entry rooms) hold for *all* of them.

---

## Success Criteria

The system is "done" when:

1. `Pipeline::run(situation)` produces a valid map for *any* well-formed situation context — no per-scenario code needed.
2. Adding a new scenario = authoring data files (vocabulary JSON + pattern selection tags). Zero Rust changes.
3. The same situation + different seeds → different maps that all pass validation and feel thematically coherent.
4. Property-based tests confirm invariants hold across thousands of random combinations.
5. At least 4 distinct scenarios demonstrate meaningfully different structures and themes.
6. `cargo run` always works, `cargo test` is always green.
