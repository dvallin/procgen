# Procgen — Narrative Procedural Dungeon Generator

> **⚠️ Highly experimental.** This project is under active development. APIs, asset formats, and output quality will change without notice. Use at your own risk — but have fun exploring.

A story-driven procedural map generator written in Rust. Given a high-level narrative context ("sealed crypt with a locked vault"), the system infers structure, materializes themed rooms, scatters terrain, places features and entities, and produces a map that feels *designed* — with coherence, variety, and no hand-authored graph.

## Quick Start

```sh
cargo run                     # Generate a Noble Crypt (default)
cargo run -- tavern           # Generate a Tavern Cellar
cargo run -- cave             # Generate a Natural Cave
cargo run -- cave --seed 42   # Deterministic output with fixed seed
cargo run -- --trace          # Show pipeline stages (to stderr)
cargo test                    # Run all 349 tests
```

### Example Output (cave, seed 3)

```text
=== Natural Cave ===

               #########
               #!..,,,,#
  ##############...,.,.#      #########    #######
  #o....s...+..+...,.,,#      #......!#    #.....#
  #........¤####.,.....#      #.~.~...######.....#
  #.........####......!#      #..GSG..+....+.....#
  #.s..T....+....,.#####      #.......############
  #.........######.#          #......!#
  #.........#    #.#          #+#######
  #o...s...¤#    #.#          #.#
  #####+#####    #.#          #.#
      #.#        #.#          #.#
      #.#        #.#          #.#
      #.#        #+###        #.#####
      #.#        #...##########.....#
      #.#        #...+..............#
      #.#        #...##########..$..#
      #.#        #####        #..M..#
      #.#                     #.....#
      #.#                     #+#####
      #.#                     #.#
      #.#                     #.#
      #.##############        #.#
      #..........+...##########.#
      ############...+..........#
                 #...############
                 #####
```

**Legend:**
| Char | Meaning |
|------|---------|
| `#` | Wall |
| `.` | Floor |
| `+` | Door |
| `*` | Locked door |
| `,` | Grass (tile) / Moss (feature) |
| `:` | Rubble (tile) |
| `~` | Water (tile) |
| `>` | Stairs (tile) |
| `!` | Torch |
| `¤` | Stalagmite |
| `†` | Altar |
| `S` | Sarcophagus |
| `$` | Chest |
| `o` | Barrel |
| `=` | Shelf |
| `T` | Table |
| `^` | Trap |
| `s` | Skeleton |
| `r` | Rat |
| `G` | Guardian |
| `@` | Smuggler |
| `M` | Mimic |

---

## How It Works

The generator is a layered pipeline where each stage transforms its input into a richer representation:

```text
SituationContext
    │  "What kind of place is this?"
    ▼
MapIntent
    │  "What rooms exist and how do they connect?"
    ▼
SpatialPlan
    │  "How big is each room and what are its constraints?"
    ▼
GeometryPlan
    │  "Where exactly does each room sit in 2D space?"
    ▼
TileMap + Atmosphere Scatter
    │  "What is the ground made of?"
    ▼
InteriorPlan
    │  "What spatial zones exist in each room?"
    ▼
FeaturePlan (templates + rules + atmosphere)
    │  "What objects sit on the ground?"
    ▼
EntityPlan
    │  "What creatures inhabit these rooms?"
    ▼
ASCII Output
```

### Layer Details

**1. SituationContext** — The narrative seed. A set of tags describing the scenario:
- `["crypt", "noble", "sealed_crypt", "locked_vault"]` → selects undead nobility theme
- `["cave", "natural", "underground", "exploration"]` → selects natural cave theme

**2. MapIntent** — A structural graph of rooms and connections. A *narrative pattern* (selected from JSON based on situation tags) defines the skeleton: which rooms have which roles (Entry, Hub, Gate, Goal, Reward, Branch) and how they connect (Traversal, RestrictedTraversal, OptionalTraversal, SecretTraversal).

**3. SpatialPlan** — Abstract room specs with sizes, archetypes, and constraints. Each node in the graph becomes a `SpaceSpec` with dimensions, archetype (Hall, Vault, Chamber, Vestibule, Corridor), and tags inherited from the theme vocabulary.

**4. GeometryPlan** — Concrete 2D placement. Rooms get pixel-level positions via BFS layout. Corridors are routed between rooms using Z-shape routing with door placement.

**5. TileMap + Atmosphere Scatter** — The actual ground. Rooms are carved as floor tiles, walls are inferred from floor adjacency, doors placed at corridor endpoints. Then tile scatter (from data-driven rules AND atmosphere profiles) replaces floor tiles with terrain variants — grass in overgrown rooms, rubble in collapsed rooms, water in flooded rooms. Non-walkable scatter is connectivity-safe.

**6. InteriorPlan** — Each room is spatially analyzed into zones (Center, WallBand, Corner, DoorPath, Open) and reserved door-to-door paths are computed. This structural analysis is consumed by downstream planners.

**7. FeaturePlan** — Objects placed on tiles. Three systems feed into feature placement, in priority order:
1. **Interior templates** — zone-aware structural placement (altar at center, torches on walls)
2. **Feature rules** — role/archetype/tag matching (chest in Reward rooms)
3. **Atmosphere contributions** — sampled from matched profiles (moss, vines, crystals)

**8. EntityPlan** — Creatures placed in rooms. Skeletons, rats, guardians — driven by rules matching on roles and tags, plus atmosphere contributions. Entry rooms are kept safe.

---

## Asset System

All content is defined in JSON files under `assets/`. The binary embeds these as fallback defaults — if a file exists on disk it's used instead, allowing runtime customization without recompilation.

The assets are organized into **four distinct layers**, each with a different job:

| Layer | Files | Purpose |
|-------|-------|---------|
| **Structure** | `patterns/`, `vocabularies/` | *What rooms exist* and what they're called |
| **Ground** | `rules/tiles.json`, `rules/tile_scatter.json` | *What the floor IS* (the tile itself) |
| **Atmosphere** | `rules/atmospheres.json` | *Mood-driven influences* sampled from room tags |
| **Furnishing** | `rules/features.json`, `rules/entities.json`, `rules/interior_templates.json` | *What sits ON the ground* |

Understanding which layer something belongs to is critical:
- **Tiles ARE the ground.** Water, rubble, grass — these are the floor itself. They affect pathfinding.
- **Features are objects ON tiles.** Moss, chests, altars — they occupy a floor cell but don't change the tile.
- **Atmosphere profiles don't place anything directly.** They *sample* influences (scatter rules, feature rules, entity rules) that feed into the existing planners.
- **Interior templates don't replace feature rules.** They provide *zone-aware structural guidance* for the feature planner — "put the altar in the center zone" rather than "put it on a random floor tile."

---

### Structure Layer

#### Narrative Patterns (`assets/patterns/narrative.json`)

Define the structural skeleton of a dungeon. Each pattern specifies:
- **Slots** — rooms with roles (Entry, Hub, Gate, Goal, Reward, Branch)
- **Edges** — connections between slots with traversal types
- **Votes** — which situation tags prefer this pattern (weighted scoring)

```json
{
  "id": "lock_and_key",
  "name": "Lock and Key",
  "slots": [
    { "key": "entry", "role": "Entry", "optional": false },
    { "key": "hub", "role": "Hub", "optional": false },
    { "key": "gate", "role": "Gate", "optional": false },
    { "key": "goal", "role": "Goal", "optional": false }
  ],
  "edges": [
    { "from": "entry", "to": "hub", "role": "Traversal" },
    { "from": "hub", "to": "gate", "role": "Traversal" },
    { "from": "gate", "to": "goal", "role": "RestrictedTraversal" }
  ],
  "votes": [
    { "tag": "locked_vault", "weight": 3 },
    { "tag": "sealed_crypt", "weight": 2 }
  ]
}
```

#### Theme Vocabularies (`assets/vocabularies/themes.json`)

Map structural roles to themed room labels, tags, and archetypes. Each vocabulary provides entries for every `NodeRole`:

```json
{
  "id": "natural_cave",
  "name": "Natural Cave",
  "location_kind": "Dungeon",
  "motifs": ["stone", "damp", "natural"],
  "roles": [
    {
      "role": "Hub",
      "entries": [
        { "label": "Grand Cavern", "tags": ["vast", "echoing"], "archetype": "Hall" },
        { "label": "Fungal Garden", "tags": ["overgrown", "luminous"], "archetype": "Hall" }
      ]
    }
  ]
}
```

The tags assigned here (`"vast"`, `"overgrown"`, `"flooded"`) are classified into **structural tags** (gameplay triggers like `"main_goal"`, `"locked"`) and **atmosphere tags** (flavor like `"damp"`, `"overgrown"`). This classification drives all downstream systems.

---

### Ground Layer

#### Tile Registry (`assets/rules/tiles.json`)

Defines the tile types available in the world. Each tile has properties that determine rendering and pathfinding:

```json
{ "name": "water", "walkable": false, "opaque": false, "ascii_char": "~", "tags": ["liquid", "hazard"] }
{ "name": "grass", "walkable": true,  "opaque": false, "ascii_char": ",", "tags": ["natural"] }
{ "name": "rubble", "walkable": true, "opaque": false, "ascii_char": ":", "tags": ["rough"] }
```

Adding a new tile type (e.g. "lava") requires only a JSON entry here — no Rust code changes.

#### Tile Scatter Rules (`assets/rules/tile_scatter.json`)

Replace floor tiles with terrain variants based on room tags. Applied during rasterization after rooms are carved:

```json
{ "target_tile": "water",  "density": 0.15, "match_tag": "flooded" }
{ "target_tile": "grass",  "density": 0.25, "match_tag": "overgrown" }
{ "target_tile": "rubble", "density": 0.20, "match_tag": "collapsed" }
```

- `target_tile` — name of a tile in the registry
- `density` — fraction of floor tiles to replace (0.0–1.0)
- `match_tag` — room must have this tag for the rule to fire

Non-walkable scatter (water, pit) is **connectivity-safe** — tiles are only placed where all cardinal neighbors remain walkable, preventing rooms from becoming disconnected.

---

### Atmosphere Layer

#### Atmosphere Profiles (`assets/rules/atmospheres.json`)

Atmosphere profiles provide **mood-driven influences** that are *sampled* based on a room's atmosphere tags. Unlike feature/entity rules (which fire deterministically when criteria match), atmosphere profiles are probabilistic — the system draws from a weighted palette.

Each profile bundles scatter, feature, and entity influences:

```json
{
  "name": "damp",
  "match_tags": ["damp", "flooded"],
  "priority": 0,
  "scatter": [
    { "target_tile": "water", "density": 0.15, "weight": 3.0 }
  ],
  "features": [
    { "feature_type": "moss", "strategy": "RandomFloor", "max_count": 3, "weight": 5.0 },
    { "feature_type": "fungus", "strategy": "Corner", "max_count": 2, "weight": 1.5 }
  ],
  "entities": [
    { "archetype": "rat", "placement": "RandomFloor", "max_count": 2, "weight": 2.0 }
  ]
}
```

**How it works:** For each room, all profiles whose `match_tags` overlap the room's atmosphere tags are merged into a palette. Scatter influences are applied deterministically (they ARE the ground). Feature and entity influences are sampled via weighted random draws (controlled by `atmosphere_sample_budget` in config, default 3). The sampled results are converted into standard `FeatureRule`/`EntityRule` entries and merged with the other rule sources.

**Key insight:** Atmosphere scatter changes the *tile itself* (ground layer), while atmosphere features/entities produce rules for the *furnishing layer*. A "damp" profile can simultaneously make the floor watery AND sprinkle moss on remaining floor tiles.

---

### Furnishing Layer

#### Interior Templates (`assets/rules/interior_templates.json`)

Interior templates provide **zone-aware structural placement** for rooms that match specific criteria. They sit at the highest priority in feature placement — when a template matches a room, its directives guide where features go using the room's spatial zones.

```json
{
  "name": "crypt_vault",
  "match_role": "Goal",
  "match_archetype": "Vault",
  "match_tag": "main_goal",
  "directives": [
    { "zone": "Center", "action": { "Place": { "feature_type": "sarcophagus", "max_count": 1 } } },
    { "zone": "WallBand", "action": { "Place": { "feature_type": "torch", "max_count": 2 } } },
    { "zone": "DoorPath", "action": "Clear" }
  ],
  "priority": 10
}
```

**Zones** are computed from the room's geometry:
- `Center` — the geometric center area
- `WallBand` — cells adjacent to walls
- `Corner` — cells near two perpendicular walls
- `DoorPath` — reserved door-to-door traversal paths (must stay unblocked)
- `Open` — remaining floor cells

**Actions:**
- `Place { feature_type, max_count }` — place this feature using zone cells as candidates
- `Clear` — no blocking features may be placed in this zone (atmosphere rules respect this too)

**Templates vs. feature rules:** Templates say *where in the room* something goes (spatial constraint). Feature rules say *what goes in which rooms* (matching constraint). A room can have a template AND still receive atmosphere-contributed features — but atmosphere features respect the template's `Clear` directives.

#### Feature Rules (`assets/rules/features.json`)

Place objects on tiles based on room properties. These are the baseline rules — they fire for rooms that don't match any template (or supplement template rooms for roles/tags not covered by the template):

```json
{ "feature_type": "sarcophagus", "strategy": "Center", "required": true, "max_count": 1,
  "match_archetype": "Vault", "match_tag": "main_goal" }
{ "feature_type": "table", "strategy": "Center", "required": false, "max_count": 1,
  "match_role": "Hub" }
{ "feature_type": "chest", "strategy": "Center", "required": true, "max_count": 1,
  "match_role": "Reward" }
```

- `strategy` — where to place: `Center`, `WallAdjacent`, `Corner`, `RandomFloor`
- `required` — if true, pipeline errors when placement fails
- `match_role` / `match_archetype` / `match_tag` — all specified criteria are ANDed

#### Feature Type Registry (`assets/rules/feature_types.json`)

Defines properties for every feature type name referenced by rules and templates:

```json
{ "name": "altar", "category": "Interactable", "ascii_char": "†", "blocking": true, "tags": ["religious"] }
{ "name": "torch", "category": "Decoration", "ascii_char": "!", "blocking": false, "tags": ["light"] }
{ "name": "chest", "category": "Container", "ascii_char": "$", "blocking": true, "tags": ["loot"] }
```

New feature types can be added with zero Rust code changes — just a JSON entry here plus rules/templates that reference it.

#### Entity Rules (`assets/rules/entities.json`)

Place creatures and NPCs:

```json
{ "archetype": "skeleton", "placement": "RandomFloor", "max_count": 2,
  "role_match": "Hub", "tag_match": null }
{ "archetype": "skeleton_guardian", "placement": { "NearFeature": "sarcophagus" }, "max_count": 2,
  "role_match": "Gate", "tag_match": "locked" }
```

---

## How the Layers Compose

Here's how a single room flows through all asset layers, showing the separation of concerns:

```text
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
STRUCTURE LAYER: "What is this room?"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Theme vocabulary assigns:
    Role=Goal, Archetype=Vault, Label="Sealed Tomb"
    structural_tags: ["main_goal", "locked"]
    atmosphere_tags: ["noble", "sealed"]

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
GROUND LAYER: "What is the floor made of?"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Tile scatter rules: no scatter rules match "noble" or "sealed"
    → floor stays as plain Floor tiles

  Atmosphere profile "noble_sealed" scatter: (none defined)
    → no scatter from atmosphere either

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
INTERIOR ANALYSIS: "What are the spatial zones?"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Zone classification produces:
    Center: 4 cells around room midpoint
    WallBand: 12 cells adjacent to walls
    Corner: 8 cells near wall intersections
    DoorPath: 5 cells on the door-to-door route
    Open: 6 remaining cells

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
FURNISHING LAYER: "What objects go where?"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  1. Interior template "crypt_vault" matches (Goal + Vault + main_goal):
     → sarcophagus in Center zone (1)
     → torches in WallBand zone (2)
     → DoorPath zone marked Clear (no blocking features)

  2. Atmosphere profile "noble_sealed" samples features:
     → banner on WallAdjacent (weight 3.0, sampled)
     → tapestry on WallAdjacent (weight 2.0, not sampled this run)
     (respects template's Clear directive on DoorPath)

  3. Atmosphere profile "noble_sealed" samples entities:
     → skeleton_guardian at Center (weight 2.0, sampled)
```

Another example showing the ground/furnishing distinction:

```text
  Room: "Fungal Garden", Role=Hub, tags: ["overgrown", "luminous"]

  GROUND: Scatter rule fires for "overgrown"
    → 25% of floor tiles BECOME Grass (tile ID changes, affects pathfinding)

  ATMOSPHERE: Profile "overgrown" fires
    → scatter: Grass at 25% (already handled above, deduped)
    → features sampled: vines (WallAdjacent), fungus (Corner)
    → entities: (none in this profile)

  FURNISHING: Template "hub_hall" matches (Hub + Hall)
    → table in Center zone
    → torches in WallBand zone
    → atmosphere features (vines, fungus) placed around template features
```

**The critical distinction:** Grass tiles in the Fungal Garden are *the ground itself* — the tile changed from Floor to Grass. The vines feature sits *on top of* a remaining Floor tile. Both came from the "overgrown" tag, but through different mechanisms targeting different layers.

---

## Adding a New Scenario

To add a new scenario you need:

1. **A situation factory** (`src/demo/your_scenario.rs`) — returns a `SituationContext` with tags
2. **A theme vocabulary** (entry in `assets/vocabularies/themes.json`) — maps roles to themed rooms
3. Optionally: new entries in feature/entity rules, atmosphere profiles, or interior templates for scenario-specific content

The narrative pattern is selected automatically from your situation tags via weighted voting — you don't need a new pattern unless your dungeon has a fundamentally different structure.

### Extending the Asset Layers

| I want to... | Edit this file |
|--------------|----------------|
| Add a new tile type (lava, ice) | `rules/tiles.json` |
| Make a tag scatter terrain | `rules/tile_scatter.json` |
| Add a new feature type (bookshelf, brazier) | `rules/feature_types.json` |
| Place a feature in specific rooms | `rules/features.json` |
| Create a mood that influences rooms | `rules/atmospheres.json` |
| Control feature layout in a room type | `rules/interior_templates.json` |
| Add a new creature type | `rules/entities.json` |
| Create a new dungeon topology | `patterns/narrative.json` |
| Theme rooms with labels and tags | `vocabularies/themes.json` |

---

## Development

```sh
cargo run                          # Default scenario
cargo run -- cave --seed 42        # Fixed seed for reproducibility
cargo run -- --trace debug         # Pipeline trace with placement details
cargo test                         # 349 tests (unit + property-based + integration)
```

### Tracing Levels

| Level | Shows |
|-------|-------|
| `--trace` (info) | Pipeline stage transitions, tile counts, atmosphere matching summary |
| `--trace debug` | Room placement, scatter counts, template matching, zone details |
| `--trace all` | Everything including corridor routing steps |

### Architecture

The codebase follows these principles:
- **Data flows down through typed layers** — each stage's output struct is the next stage's input
- **Traits at boundaries** — every transform is swappable (`SpatialPlanner`, `Rasterizer`, etc.)
- **Tags for soft intent** — layers communicate context via `Vec<Tag>` strings
- **No panics in library code** — all trait methods return `Result`
- **Property-based tests** — layer invariants verified for arbitrary inputs via proptest

See [PLAN.md](PLAN.md) for the full roadmap and [AGENTS.md](AGENTS.md) for contributor/AI-agent guidance.
