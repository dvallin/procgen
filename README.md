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
cargo test                    # Run all 309 tests
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
TileMap (+scatter)
    │  "What is the ground made of?"
    ▼
FeaturePlan
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

**5. TileMap (+scatter)** — The actual ground. Rooms are carved as floor tiles, walls are inferred from floor adjacency, doors placed at corridor endpoints. Then **tile scatter rules** replace some floor tiles with terrain variants (grass in natural rooms, rubble in collapsed rooms, water in flooded rooms).

**6. FeaturePlan** — Objects placed on tiles. Chests, altars, barrels, torches, moss, stalagmites — driven by rules that match on room role, archetype, and tags.

**7. EntityPlan** — Creatures placed in rooms. Skeletons, rats, guardians — driven by rules matching on roles and tags. Entry rooms are kept safe.

---

## Asset System

All content is defined in JSON files under `assets/`. The binary embeds these as fallback defaults — if a file exists on disk it's used instead, allowing runtime customization without recompilation.

### Narrative Patterns (`assets/patterns/narrative.json`)

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

### Theme Vocabularies (`assets/vocabularies/themes.json`)

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

The tags assigned here (`"vast"`, `"overgrown"`, `"flooded"`) drive both tile scatter and feature placement downstream.

### Tile Registry (`assets/rules/tiles.json`)

Defines the tile types available in the world. Each tile has properties that determine rendering and pathfinding:

```json
{ "name": "water", "walkable": false, "opaque": false, "ascii_char": "~", "tags": ["liquid", "hazard"] }
{ "name": "grass", "walkable": true,  "opaque": false, "ascii_char": ",", "tags": ["natural"] }
{ "name": "rubble", "walkable": true, "opaque": false, "ascii_char": ":", "tags": ["rough"] }
```

Adding a new tile type (e.g. "lava") requires only a JSON entry here — no Rust code changes.

### Tile Scatter Rules (`assets/rules/tile_scatter.json`)

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

### Feature Rules (`assets/rules/features.json`)

Place objects on tiles based on room properties:

```json
{ "kind": "Sarcophagus", "strategy": "Center", "required": true, "max_count": 1,
  "match_archetype": "Vault", "match_tag": "main_goal" }
{ "kind": {"Decoration": "moss"}, "strategy": "RandomFloor", "required": false, "max_count": 3,
  "match_tag": "overgrown" }
```

- `strategy` — where to place: `Center`, `WallAdjacent`, `Corner`, `RandomFloor`
- `required` — if true, pipeline errors when placement fails
- `match_role` / `match_archetype` / `match_tag` — all specified criteria are ANDed

### Entity Rules (`assets/rules/entities.json`)

Place creatures and NPCs:

```json
{ "archetype": "skeleton", "strategy": "RandomFloor", "max_count": 2,
  "match_role": "Hub", "match_tag": null }
{ "archetype": "gate_guardian", "strategy": "NearDoor", "max_count": 2,
  "match_role": "Gate", "match_tag": "locked" }
```

---

## How Assets Stack Together

Here's how a single room flows through the asset layers:

```text
Theme vocabulary says:
    Hub → "Grand Cavern", tags: ["vast", "echoing"], archetype: Hall

Tile scatter rules see tag "vast":
    (no scatter rule matches "vast" — floor stays as-is)

Feature rules see tag "vast" + role Hub:
    → place stalagmites (max 2, RandomFloor)
    → place table (max 1, Center)
    → place barrels (max 2, WallAdjacent)

Entity rules see role Hub:
    → place skeletons (max 2, RandomFloor)
```

Another example with terrain:

```text
Theme vocabulary says:
    Goal → "Underground Lake", tags: ["main_goal", "flooded"], archetype: Vault

Tile scatter rules see tag "flooded":
    → replace 15% of floor tiles with Water (non-walkable, connectivity-safe)

Feature rules see tag "main_goal" + archetype Vault:
    → place sarcophagus (required, Center)
    → place torches (max 2, WallAdjacent)

Entity rules see role Goal:
    → place guardians (max 2, NearDoor)
```

---

## Adding a New Scenario

To add a new scenario you need:

1. **A situation factory** (`src/demo/your_scenario.rs`) — returns a `SituationContext` with tags
2. **A theme vocabulary** (entry in `assets/vocabularies/themes.json`) — maps roles to themed rooms
3. Optionally: new entries in feature/entity rules or tile scatter for scenario-specific tags

The narrative pattern is selected automatically from your situation tags via weighted voting — you don't need a new pattern unless your dungeon has a fundamentally different structure.

---

## Development

```sh
cargo run                          # Default scenario
cargo run -- cave --seed 42        # Fixed seed for reproducibility
cargo run -- --trace debug         # Pipeline trace with placement details
cargo test                         # 309 tests (unit + property-based + integration)
```

### Tracing Levels

| Level | Shows |
|-------|-------|
| `--trace` (info) | Pipeline stage transitions, tile counts |
| `--trace debug` | Room placement decisions, scatter counts, feature/entity details |
| `--trace all` | Everything including corridor routing steps |

### Architecture

The codebase follows these principles:
- **Data flows down through typed layers** — each stage's output struct is the next stage's input
- **Traits at boundaries** — every transform is swappable (`SpatialPlanner`, `Rasterizer`, etc.)
- **Tags for soft intent** — layers communicate context via `Vec<Tag>` strings
- **No panics in library code** — all trait methods return `Result`
- **Property-based tests** — layer invariants verified for arbitrary inputs via proptest

See [PLAN.md](PLAN.md) for the full roadmap and [AGENTS.md](AGENTS.md) for contributor/AI-agent guidance.
