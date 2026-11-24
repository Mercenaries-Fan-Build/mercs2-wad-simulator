# Driving — behaviour contract (Chassis · Powertrain · Steering · Coupling)

The design is the "Driving" row of the Workshop Polish Pass canvas: `DrivingChassis`, `DrivingPowertrain`,
`DrivingSteering` and `Driving` (the Coupling board). Driving stops being Inspect with a vehicle filter
and becomes a page about one vehicle's driving systems: a systems bar, a 3D viewport with overlays and
preview-state pickers, a fixed bench under it, and system cards on the right.

Every row is an element: what the user does, what happens, where the data comes from, what it writes,
what is missing, and what proves it. Nothing is drawn without its row being true. The page reads the
game and writes nothing; a contribution chip starts a Depot item, as on Inspect.

Grades: PROVEN (read from bytes or code this pass, or run in the game), INFERRED, UNPROVEN. Game values
come from the retail `vz.wad` worldentity (`mercs2_probe vehicle-tuning --out`, `we-template`, `hier`) and
executable reads of pc-v10 (`mercs2_unpacked.exe`, sha256 `8c179a99…c7ba`).

## Facts the design stands on

| Fact | Evidence | Grade |
|---|---|---|
| A car's physics record is `_CarPhysicsV2`: 97 fields in 99 words (one is three floats), each read by name. 454 templates carry one | loader `FUN_00658f60` (`mov edx,<name string>` per field); `vehicle-tuning --out` | PROVEN |
| The worldentity schema stores a field's name hash only; the Workshop's `names.bin` holds asset and node names, not field names | `mercs2_formats::schema::SchemaField`; `main.rs` `referenced_hashes_one` | PROVEN |
| `MaxSpeed`, `ReverseMaxSpeed`, `LowSpeedLimit` are km/h | `fmul [0xBA8734]` (3.6) at `0x44DD42`; `FUN_0044f680` compares with the same speed | PROVEN |
| Engine rpm = `GearRatioPrimary × ratio[gear] × Σ share × 9.5493 × wheel speed / WheelRadius`; shares sum to 2 | `FUN_0044d430`, `FUN_0044fe20`; constants `0xBEB430` (9.5493), `0xB92874` (2.0) | PROVEN (constants read by bytes; formula by agent disassembly) |
| A gear shifts up above `GearUpshiftRPM`, down below `GearDownshiftRPM`, five forward gears; each shift gives `ClutchDelayTime` seconds of no drive | `FUN_0044fda0` | PROVEN (agent disassembly) |
| Torque below `GearOptRPM` is `E(1 + (TFmin − 1)x²)`, above it `E(1 + (TF − 1)y²)`; friction `E(0.1 − 0.05x)` then `E(0.1 + 0.2y)`; at `GearMaxRPM` torque 0 and friction 0.3E | `FUN_0044fe20`; constants `0xB92B58`, `0xBEAD68`, `0xBEAD64`, `0xB9B688` | PROVEN (constants by bytes) |
| The max-rpm torque factor slot is filled from the name `TorqueFactorAtMinRPM` | `mov edx,0xBC8780` at `0x6594E0` and `0x6594F8` | PROVEN |
| No code reads `GearTorqueMultiReverse`, `GearTorqueMulti1..5`, `torqueFactorAtMaxRPM` (by name), `UseIkarugaShape`. `SteerMaxVisualAngle` is copied into the car (`+0x1a8`, at `0x4495a9`) and nothing reads it there | offset search over the car actor functions and the wheel pose controller | PROVEN for the functions searched |
| Drive fades over the last 5 % below the speed cap | `0xB98748` (0.05), `FUN_0044a970` | PROVEN |
| `RearToFrontTorqueDistrib` is the rear share; a rear wheel gets `RTF × 2 / n_rear`, a front wheel `(1 − RTF) × 2 / n_front` | `FUN_0044fc10` | PROVEN (agent disassembly) |
| Steering lock = `SteerMaxAngle × (1 − L × clamp01(v / MaxSpeed))` radians; `L` is `HighSpeedSteerLoss`, times the brake or no-throttle multiplier | `FUN_0044f680` | PROVEN (agent disassembly) |
| With `SteeringInertia` ≥ 0 the angle keeps a fraction `k` of its error every 1/60 s; below 0 the rate fields `SteerSpeed` and `LowSpeedSteerSpeed` apply instead | `FUN_0044f680`, `FUN_00714d90` | PROVEN (agent disassembly) |
| Only front wheels steer. A `_CarWheel` record is `SuspBone`, `VisYawBone`, `VisRotBone`, `VisPosBone`, `Axle`; `Axle` 0 is front | loader at `0x6599c0`; `FUN_00449dc0` | PROVEN (agent disassembly) |
| A wheel's `VisYawBone`, `VisRotBone` and `VisPosBone` each fall back to `SuspBone` when 0. Every wheel of the Austin has the three at 0, so `bone_wheel_fl` / `_fr` take the yaw | `FUN_0044df60`: `mov ecx,[eax+4]; test ecx,ecx; jne; mov ecx,[eax]` at `0x44e0a5`, the same for `+8` and `+0xc`; `we-template "Austin (CIV)"` | PROVEN |
| The game shows steering on a front wheel by turning its yaw node about the node's own up axis at the node's own place, ahead of its rest orientation; the parent hub is not written | wheel pose controller (vtable `0xbcd490`) `FUN_006814e0`: `D3DXMatrixRotationY` at `0x6819bf`, multiplied at `0x6819dc`; yaw only where the front flag is set (`0x68178f`) | PROVEN (agent disassembly) |
| The angle shown is the share of the lock in use times the whole `SteerMaxAngle`, smoothed, the same on every front wheel; the value goes to `D3DXMatrixRotationY` as stored, so it is radians. Which side a positive steer input turns to is not read | `FUN_0044f680` at `0x44fb8d`; `FUN_00449dc0` | PROVEN (agent disassembly); the side UNPROVEN |
| The Austin's front wheel nodes stand upright at rest, so the node's up axis is the model's | `hier` | PROVEN |
| Suspension uses only the sum `SuspExpLength + SuspCmpLength + ExtraInvisibleSpringLength` | `FUN_0044df60` | PROVEN (agent disassembly) |
| The seven templates with `EngineTorque` 0 are the trailers: Fuel, Box, Flatbed, Opentop, Trailer, Civ Fueltrailer, Civ Fueltrailer_PmcCon018 | `vehicle-tuning --out`, filter on the field | PROVEN |
| The model-name rule files `oc_veh_semi_fueltrailer` under SEMI | `index.rs` `classify_vehicle_token` checks `semi` before `trailer` | PROVEN |
| Six tractor models have a node `0x661F1DF1` at (0, 1.20, −3.10). `hp_trailerhitch` hashes to it; no shipped record or script names the node | `hier`; hash scan of every public build | PROVEN (node), INFERRED (the name) |
| `Object.Attach(truck, "hp_trailerhitch", trailer)` puts the trailer's origin on that node, rigidly | live, retail PC, 2026-11-23 | PROVEN |
| An exhaust is a `VehiclePart` of type 6: `PartName` is a node, `PartData` the name hash of a template. The Austin has two (`hp_fx_exhaust_a`, `_b`), both `0x06A262B1` = `global_particle_veh_exhaust_car`; that hash is no effect of the archive | `we-template "Austin (CIV)"`; `pandemic_hash_m2`; `game_world::load_effect` returns none for it | PROVEN |
| The effect an exhaust plays is the one its template's `RedEffectComponent` names, started with that record's duration (0: it runs on) and per-metre rate | `vfxview::starters`, as `ObjectState.StartEmitter` passes a template name hash (`effect_container_format.md` §1.1) | PROVEN (the records), INFERRED (that the engine starts the template at the node, turned as the node is) |
| The Austin's exhaust nodes are the tops of its stacks at height 4.30, each tipped 50° back from upright | `hier` | PROVEN |
| What a light part (`VehiclePart` types 1 to 3) changes on screen is not known | nothing in the Workshop or engine draws one; `Vehicle.SetParts` → `0x513C00` is unread | UNPROVEN |
| How a rider is posed in a seat is not known | `layerview.rs:14-15` | UNPROVEN |

## Decisions

Defaults taken where the boards and the code disagree. D5 to D9 are the user's to confirm.

- **D1 — Its own page module.** `drivingpage.rs` in the `audiopage.rs` shape: `Panel`, `Act`, `navigator`,
  `center`, `inspector`, `verbs`, `apply`, `crumb`, `apply_state`. Driving keeps the shared viewport and
  `Preview`.
- **D2 — Panel widths stay the user's.** The boards draw 240 and 370; the page uses the shared held
  panels (220–360 and 300–480) at whatever width they were left.
- **D3 — Field names ship in the Workshop.** A table of the 97 `_CarPhysicsV2` and 5 `_CarWheel` names,
  hashed at load with `pandemic_hash_m2`. A schema hash with no name in the table stops the page with a
  message naming the hash.
- **D4 — Units are shown only where proven:** km/h, rad, rad/s, rpm, s, m for node positions. Every other
  field shows a bare number.
- **D5 — The systems bar lists four systems.** Occupants, Lights & FX, Damage and Cameras have no boards,
  so they have no tabs yet.
- **D6 — Car physics only.** The four systems read `_CarPhysicsV2`. A vehicle with tank, boat, helicopter
  or jet physics opens with no systems bar and the existing model sections.
- **D7 — Pickers in this pass: Trailer and Exhaust.** Exhaust is Off or On, since nothing in the data
  separates idle from running. Lights and Riders are not drawn until the two UNPROVEN rows above are read.
- **D8 — Contribution chips stay `Domain::Driving.kinds()`:** New model, Replace texture, New texture,
  Destruction states. The boards' "Template edit" and "Lua patch" are not kind titles and come off.
- **D9 — Classes in the vehicle list stay as they are.** The fuel trailer stays under SEMI and the towed
  fuel tank under TOWED. Only the Trailer picker and the Trailers card use the `EngineTorque` 0 rule.

## Page and navigation

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Rail | Unchanged: Driving is the third page | `app.rs` `Workbench::ALL` | — | — | existing rail tests |
| Header crumb | `DRIVING` then class › template › system, e.g. Semi › Austin (CIV) › Chassis | `drivingpage::crumb` | — | crumb branch in the cmdbar | test: crumb for a selection |
| Vehicle list | Models with a vehicle class, grouped by class with a count per group; a group opens and closes; the open group lists its models; click loads the model | `index.models()` filtered by `Domain::Driving.model_lens`, `index::category_order` | — | collapsible groups and counts (today a flat list) | test: group counts equal the lens count |
| Filter box | Typing narrows the list to models whose name contains the text, across groups | same rows | — | filter | test: filter keeps matching rows only |
| Toolbar | Export F10 and View textures F3 as on Inspect | `Act::OpenExport`, `Act::TexOfPreview` | as Inspect | — | existing |
| Template picker | Lists the templates that name the model, by name; shows the position (1 of 4); choosing one re-reads every card, chart and overlay | `AssetIndex.world_templates` filtered by model | — | picker; per-template records | test: Austin model lists its templates; values change with the choice |
| Systems bar | One tab per system; click swaps the bench, the right-hand cards and the overlay toggles | `drivingpage::System` | — | tab strip helper in `gui.rs` | test: `apply(Act::System)`; capture per system |
| Status bar | Template name and the system's headline values | computed, per system rows below | — | — | capture |
| `--ui-state` | Keys `system`, `template`, `trailer`, `exhaust`, `overlays` on `page: "Driving"`; `load` works on the page; `orbit` aims the model viewport's camera on any page | `ui_state.rs`, `drivingpage::apply_state` | — | keys; `load` on a domain page | captures `ui_states/driving_*.json` |
| Type and panels | Sizes from `theme::type_scale()`; the bench is 270 px, not resizable; a card row never draws wider than the inspector: a value with no range bar is cut short at the row's end | `gui.rs` | — | bottom bench panel | `no_hard_coded_font_sizes_in_the_workshop_sources`; fit test of every system's cards in a 300 and a 372 px panel |

## Data the page loads

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Template records | On first show the page parses the worldentity once and keeps it | `WadStack::worldentity`, `WorldEntity::parse` | — | a held copy (boot keeps derived tables only) | test: second show does not re-parse |
| Named fields | A template's `_CarPhysicsV2` record as name → value | `Component::decode`, the D3 table | — | `drivingdata::car_fields` | test: Austin `MaxSpeed` 90, `GearRatio1` 3.7, `SteerMaxAngle` 0.8 |
| Fleet range | Lowest and highest value of each field over every `_CarPhysicsV2` record, and the template count | one pass over `we.groups_of("_CarPhysicsV2")` | — | `drivingdata::Fleet` | test: 454 templates in retail `vz.wad` alone (the page reads the open stack, overlays included); `Mass` 200 to 28,500; `EngineTorque` 0 to 1,800 |
| Wheels | The template's `_CarWheel` records: node, axle | `we.groups_of("_CarWheel")`, `WorldEntity.enums` | — | decoder | test: Austin 6 wheels, 2 front and 4 rear |
| Node positions | Each HIER node's model-space position by name hash | `Preview.hier_nodes`, parent chain as `layerview::node_worlds` | — | `node_worlds` made shared | test: Austin front axle z 4.07, hitch (0, 1.20, −3.10) |
| Trailers | Templates with `EngineTorque` 0, with their models | `Fleet` | — | list | test: seven names |

## Boards

### Chassis (`DrivingChassis`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Blueprint, side | Wheels as circles of `WheelRadius` at their node positions, the ground line under them, wheelbase dimensions between axles; hitch, winch and exhaust nodes at their positions with their heights | wheels, node positions, `ConnectPoint`, `VehiclePart` | — | scaled drawing helper | capture: Austin dimensions 6.44 and 1.38 |
| Blueprint, top | Wheels, axles, track per axle, hitch, winch points, seats and exhausts on the centre line view | same, plus `SeatLink` nodes | — | — | capture: track 2.28 front, 1.92 rear |
| Blueprint scale | One scale for both views, chosen so the longest vehicle axis fits the bench | node extents | — | — | test: a 2 m and a 14 m model both fit |
| Axles card | Front and rear columns: axle position, wheel radius, suspension strength, damp, expansion and compression length, forward and side friction, brake, emergency brake | named fields | — | — | test: Austin values |
| Range bar | Beside a value, a tick at its place between the fleet's lowest and highest; no bar where lowest equals highest | `Fleet` | — | `theme::range_tick` | test: tick fraction for `Mass` |
| Body card | Mass, gravity factor, linear dampening, angular dampening X·Y·Z, inertia tensor X·Y·Z, centre of mass offset | named fields | — | — | test |
| SHOW WHEELS | Toggle: a marker on each wheel node | `scene.set_glow_cards`, wheels | — | the Mods-page marker block shared | capture |
| SHOW SUSPENSION TRAVEL | Toggle: at each wheel a line the length of the three-field sum, below the node | `layerview::project`, named fields | — | 2D overlay | capture |
| Existing sections | Info, LOD, Destruction, Segments, Animation, Game scripts, Skeleton follow the cards, as on Inspect | `app.rs` inspector | as Inspect | — | existing |

### Powertrain (`DrivingPowertrain`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Gearing chart | Rpm against km/h: one line per forward gear, the full-throttle path with its shift drops, lines at the upshift and downshift rpm, the speed cap and its 5 % fade band; the end point labelled with its rpm | `drivingmath::gearing` from named fields, wheels | — | chart helper; the math | test: Austin upshifts 42.45, 47.60, 62.83, 71.40 km/h; cap at 4,297 rpm; capture |
| Chart axes | X to the next 20 km/h above `MaxSpeed`; Y to `GearMaxRPM` | named fields | — | — | test: a 165 km/h car and a 75 km/h truck |
| Torque chart | Torque, friction and their difference at full throttle against rpm, with ticks at min, opt and max rpm | `drivingmath::torque` | — | — | test: Austin 1,520 at 1,000 rpm, 1,440 at 4,500, 1,120 at 8,000 |
| Engine card | Engine torque, max speed, reverse max speed, rpm min · opt · max, torque factor at min, rear torque share | named fields, `Fleet` | — | — | test |
| Gear table | Reverse and gears 1 to 5: ratio, and the km/h at which each of 1 to 4 shifts up | `drivingmath::upshift_kmh` | — | — | test |
| Gearbox rows | Final drive, shift rpm up and down, clutch delay | named fields | — | — | test |
| Not read by the PC exe | Lists this class's fields that no code reads | a constant list beside the D3 table, each with the read that showed it | — | — | test: every listed name is in the schema |
| SHOW TORQUE SPLIT | Toggle: each wheel labelled with its share of drive torque | `FUN_0044fc10` rule, wheels | — | 2D overlay | test: Austin 0.25 rear, 0.5 front; capture |

### Steering (`DrivingSteering`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Lock chart | Radians against km/h for throttle, coasting and braking, flat past `MaxSpeed`; the lock at rest labelled in radians and degrees | `drivingmath::lock` | — | — | test: Austin 0.8 at rest; 0.184, 0.332, 0.492 at 90 km/h |
| Response chart | Share of lock against seconds for turning in and for release, from the two inertia values; drawn only when `SteeringInertia` ≥ 0 | `drivingmath::response` | — | — | test: 0.70 after 1 s turning in, 0.16 after 1 s released |
| Response chart, rate mode | When `SteeringInertia` < 0: lock against seconds at the rate fields' rad/s, at rest and at `LowSpeedLimit` | same | — | — | test: Flatbed Trailer (inertia −1) |
| Steering card | Steered axle and wheel count, max angle, high speed loss and its two multipliers, the three inertias, low speed limit, low speed boost | named fields, wheels, `Fleet` | — | — | test |
| Rate card | Steer speed and low speed steer speed, dim when `SteeringInertia` ≥ 0 | named fields | — | — | test: dim for the Austin, lit for the Flatbed |
| Not read by the PC exe | `SteerMaxVisualAngle` | constant list | — | — | test |
| SHOW FULL LOCK | Toggle: each front wheel's yaw node, with what hangs from it, turned by `SteerMaxAngle` about its own up axis; a wheel node the model lacks leaves the model at rest and says why beside the chip | `drivingpage::turn_nodes` over `rotor::spin_subtree`, `Wheel::yaw_bone` | — | the turn at the node | capture `driving_steering_lock` (front view); test: other nodes unmoved |

### Coupling (`Driving`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Hardpoints table | Every named node: name, position, and the records that name it (class and link name); click selects | `Preview.hier_nodes`, `index.names`, a scan of the template's records for the node hash | — | the scan | test: Austin `hp_winch_top` ← ConnectPoint; hitch ← nothing |
| Selected node | The table row and a marker in the viewport highlight; a readout shows name and position | `hp_selected`, `scene.set_glow_cards` | — | the Mods gate widened to Driving; readout label | capture |
| Node detail | Hash, position, the list of records naming it, and the vehicle models whose hierarchy has the node, by name | `drivingdata::models_by_node` over the models of the vehicle list, read once with the templates | — | cross-model scan | test: hitch on six models |
| On this vehicle | Hitch node if present, winch point count, `PhysicalLink` and `ConstraintLink` record counts | records | — | — | test: Austin 2 winch points, 0 and 0 |
| Trailers card | The seven trailer templates; click selects one as the Trailer picker's value | `Fleet` trailers | — | — | test |
| Trailer attached | With a trailer chosen, its model draws with its origin on the hitch node, following the tractor; the camera frames the two together, and the tractor alone again when the trailer comes off | spawn as `layerview.rs:610-613`, `node_worlds`; `scene.model_bounds`, `drivingpage::enclosing` | — | second model in the preview world | capture: flatbed deck at cab-roof height, as the game shows; test: the sphere holding both |
| Ride height | The trailer origin's height above the tractor's origin, which is the hitch node's height | node positions | — | — | test: Flatbed on Austin 1.20 |
| Trace rows for `ConstraintLink` | Not drawn: they are notes about the executable, not values of this vehicle | — | — | — | — |

## Viewport

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Orbit, pan, zoom | As Inspect | `app.rs` camera | — | — | existing |
| Overlay chips | The toggles of the open system, top left; green when on | `theme::chip` variant | — | green-on chip | capture |
| Trailer picker | None or a trailer template; the same value as the Trailers card | `Panel.trailer` | — | picker row on the viewport | test: `apply(Act::Trailer)` |
| Exhaust picker | Off or On, drawn for a template with an exhaust part: on plays each type-6 `VehiclePart`'s effect at its node, turned as the node is, repeating; a part whose `PartData` names no effect-starting template, or whose node the model lacks, turns the picker off and says why beside it | `drivingdata::started_by_name` over `index.effect_starters`, `game_world::load_effect`, `scene.fx_start_effect`, `EffectSim::set_rotation` | — | effects beside a live preview (today opening an effect closes it); the instance's rotation after it starts | capture `driving_exhaust`; test: Austin plays two, one each side at 4.30 |
| Model caption | Model name and hash, bottom right | `Preview` | — | — | capture |

## Not in this pass

| Element | What it needs first |
|---|---|
| Lights picker | A read of `Vehicle.SetParts` → `0x513C00`: what a light part of type 1, 2 or 3 does to its node |
| Riders picker | The seat's pose: which clip a rider plays and how the seat node places the character |
| Idle and running exhaust | Evidence that the game plays different effects or rates; types 7 and 8 (`ExhaustStart`, `ExhaustStop`) are on no semi |
| `ConstraintLink` preview | A run of the game with a `ConstraintLink` record, to see what it does |
| Occupants, Lights & FX, Damage, Cameras systems | Boards |
| Tank, boat, helicopter, jet systems | Boards, and each class's field read |

## Corrections to the boards

- Contribution chips read New model, Replace texture, New texture, Destruction states (D8).
- Exhaust picker values are Off and On (D7); Riders and Lights pickers come off until built.
- The systems bar shows four tabs (D5).
- The Coupling bench's `ConstraintLink` and type rows come off; the Link card comes off with them.
- A card row holds one field: the Engine card's rpm row is three (min, opt, max), the Gearbox card's shift
  row two (up, down), each with its range bar; the Coupling card's hitch is two (node, position). The
  boards draw each as one row, which does not fit the inspector at its narrowest.
- The node detail lists the node's hash, its position, the records naming it, the trailer on it, and the
  vehicle models that carry it.

## Order of work

1. `drivingdata` (named fields, fleet, wheels, nodes, trailers) and `drivingmath`, with their tests.
2. `gui.rs` helpers: tab strip, range tick, chart with axes, scaled drawing, green chip.
3. The page shell: module, navigator, template picker, systems bar, bench, inspector cards, crumb, `--ui-state`.
4. Chassis, Powertrain, Steering, Coupling benches and cards, each with its capture.
5. Overlays, then the Trailer and Exhaust pickers.
6. `docs/cli/mercs2_workshop.md` and the crate README for the new keys and page.
