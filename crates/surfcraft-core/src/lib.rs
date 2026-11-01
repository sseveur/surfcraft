//! Surfboard geometry: the Bezier board model (outline, rocker, deck, cross-sections), its two surface models,
//! the measurements shown to the user, board operations, the weight estimate, units and meshing.
//!
//! Lengths are centimetres. x runs from the tail (0) to the nose, y across from the stringer, z up.

pub mod board;
pub mod knot;
pub mod math;
pub mod mesh;
pub mod section;
pub mod segment;
pub mod spline;
pub mod surface;
pub mod units;
pub mod weight;

pub use board::{Board, BoardInfo, MachineData, Specs, SurfaceModel, fin};
pub use glam::{DVec2, DVec3};
pub use knot::{Handle, Knot, Locks};
pub use mesh::{Mesh, MeshDetail};
pub use section::CrossSection;
pub use segment::Segment;
pub use spline::{Spline, polyline};
pub use surface::Station;
pub use units::LengthUnit;
