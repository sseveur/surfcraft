//! User preferences, persisted between runs by eframe.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use surfcraft_core::{LengthUnit, SurfaceModel};

pub const STORAGE_KEY: &str = "surfcraft-settings";
/// Key used before the rename to SurfCraft (read once, then saved under [`STORAGE_KEY`]).
pub const OLD_STORAGE_KEY: &str = "boardcad-settings";
const RECENT_LIMIT: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    FeetInchesFraction,
    FeetInchesDecimal,
    Millimetres,
    Centimetres,
    Metres,
}

impl From<Unit> for LengthUnit {
    fn from(u: Unit) -> Self {
        match u {
            Unit::FeetInchesFraction => LengthUnit::FeetInchesFraction,
            Unit::FeetInchesDecimal => LengthUnit::FeetInchesDecimal,
            Unit::Millimetres => LengthUnit::Millimetres,
            Unit::Centimetres => LengthUnit::Centimetres,
            Unit::Metres => LengthUnit::Metres,
        }
    }
}

impl From<LengthUnit> for Unit {
    fn from(u: LengthUnit) -> Self {
        match u {
            LengthUnit::FeetInchesFraction => Unit::FeetInchesFraction,
            LengthUnit::FeetInchesDecimal => Unit::FeetInchesDecimal,
            LengthUnit::Millimetres => Unit::Millimetres,
            LengthUnit::Centimetres => Unit::Centimetres,
            LengthUnit::Metres => Unit::Metres,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    SBlend,
    ControlPoint,
}

impl From<Model> for SurfaceModel {
    fn from(m: Model) -> Self {
        match m {
            Model::SBlend => SurfaceModel::SBlend,
            Model::ControlPoint => SurfaceModel::ControlPoint,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewToggles {
    pub grid: bool,
    pub control_points: bool,
    pub guide_points: bool,
    pub ghost: bool,
    pub original: bool,
    pub curvature: bool,
    pub section_positions: bool,
    pub fins: bool,
    pub center_line: bool,
    pub apex_line: bool,
    pub tuck_line: bool,
    pub center_of_mass: bool,
    pub other_sections: bool,
    pub measurements: bool,
    pub background_image: bool,
}

impl Default for ViewToggles {
    fn default() -> Self {
        Self {
            grid: true,
            control_points: true,
            guide_points: true,
            ghost: true,
            original: false,
            curvature: false,
            section_positions: true,
            fins: true,
            center_line: true,
            apex_line: false,
            tuck_line: false,
            center_of_mass: false,
            other_sections: true,
            measurements: true,
            background_image: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub unit: Unit,
    /// Finest inch fraction shown (16 = 1/16").
    pub fraction: u32,
    pub model: Model,
    pub view: ViewToggles,
    pub recent: Vec<PathBuf>,
    pub last_dir: Option<PathBuf>,
    /// Hide spec panel etc.
    pub show_side_panel: bool,
    /// Accept commands from the local MCP server (AI assistant). Off by default.
    pub allow_automation: bool,
    /// Profile view vertical exaggeration (1 = true scale).
    pub profile_exaggeration: f32,
    /// Interface language; `None` follows the operating system.
    pub language: Option<crate::i18n::Language>,
    /// The first-launch tour was finished or closed.
    pub tour_done: bool,
    /// Also log debug-level events (edits, clicks, view changes) to help reproduce a bug.
    pub detailed_logging: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            unit: Unit::FeetInchesFraction,
            fraction: 16,
            model: Model::SBlend,
            view: ViewToggles::default(),
            recent: Vec::new(),
            last_dir: None,
            show_side_panel: true,
            allow_automation: false,
            profile_exaggeration: 1.0,
            language: None,
            tour_done: false,
            detailed_logging: false,
        }
    }
}

impl Settings {
    pub fn length_unit(&self) -> LengthUnit {
        self.unit.into()
    }

    pub fn fmt(&self, cm: f64) -> String {
        self.length_unit().format(cm, self.fraction)
    }

    pub fn add_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(RECENT_LIMIT);
        self.last_dir = path.parent().map(|p| p.to_path_buf());
    }
}
