//! Translation responsibilities are exposed through their owning modules.
//! Start with `api` for translation, `analysis` / `diagnostic` for explanation,
//! `behavior_type` / `runtime_review` for evidence, and `piston_construction`
//! for ordered construction. Domain primitives retain their original crate
//! through `world` and `ir`. Former flat-root compatibility exports are removed.

pub mod abstract_behavior;
pub mod analysis;
pub mod api;
pub mod assembly;
pub mod assembly_transform;
mod autonomous;
pub mod behavior;
mod behavior_review;
pub mod behavior_type;
pub mod blueprint;
pub mod blueprint_connection;
pub mod blueprint_generation;
pub mod blueprint_update;
pub mod building;
pub mod cell_generators;
pub mod cell_library;
pub mod cells;
pub mod circuits;
pub mod compiler;
pub mod connectivity;
pub mod diagnostic;
pub mod dust_law;
pub mod electrical;
pub mod finite_burst;
pub mod flying_machine;
pub mod location_behavior;
pub mod runtime_behavior;
pub mod runtime_review;
pub mod torch_law;
pub use dustroute_ir as ir;
pub mod liveness;
pub mod minecraft_export;
pub mod minecraft_semantics;
pub mod multinet;
pub mod native_state;
pub mod observed_piston_door;
pub mod periodic;
pub mod physical;
pub mod physical_behavior;
pub mod physics_trace;
pub mod piston_construction;
pub mod piston_door;
pub mod piston_door_type;
pub mod port_realization;
pub mod promotion;
pub mod repair;
pub mod review_diagnostics;
pub mod routing;
pub mod routing_resources;
pub mod scenario;
pub mod sim;
pub mod snapshot;
pub mod transition_conformance;
pub mod vanilla_instrumentation;
pub mod wire;
pub use dustroute_minecraft as world;
pub mod world_reverse;

pub mod piston_observation;

mod world_laws;
