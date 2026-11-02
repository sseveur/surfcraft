//! Heavy board calculations (volume, centre of mass, 3D mesh) on a background thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use surfcraft_core::{Board, Mesh, MeshDetail, Specs};

#[derive(Clone, Default)]
pub struct Results {
    pub revision: u64,
    pub specs: Specs,
    pub center_of_mass: f64,
    pub mesh: Arc<Mesh>,
    /// Fins under the bottom, drawn in the 3D view when View > Fins is on.
    pub fins: Arc<Mesh>,
}

#[derive(Default)]
pub struct Worker {
    latest: Arc<Mutex<Option<Results>>>,
    busy: Arc<AtomicBool>,
    pending: Option<(Board, u64)>,
    requested: Option<u64>,
}

impl Worker {
    /// Ask for results for this board revision (ignored if already requested).
    pub fn request(&mut self, board: &Board, revision: u64, ctx: &egui::Context) {
        if self.requested == Some(revision) {
            return;
        }
        self.requested = Some(revision);
        self.pending = Some((board.clone(), revision));
        self.pump(ctx);
    }

    fn pump(&mut self, ctx: &egui::Context) {
        if self.busy.load(Ordering::Acquire) {
            return;
        }
        let Some((board, revision)) = self.pending.take() else {
            return;
        };
        self.busy.store(true, Ordering::Release);
        let latest = self.latest.clone();
        let busy = self.busy.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let specs = board.specs();
            let center_of_mass = board.center_of_mass();
            let mesh = Arc::new(Mesh::of_board(&board, MeshDetail::PREVIEW));
            let fins = Arc::new(board.fin_mesh());
            *latest.lock().unwrap() = Some(Results {
                revision,
                specs,
                center_of_mass,
                mesh,
                fins,
            });
            busy.store(false, Ordering::Release);
            ctx.request_repaint();
        });
    }

    /// Latest finished results (possibly for an older revision while a newer one is computing).
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Results> {
        self.pump(ctx);
        self.latest.lock().unwrap().clone()
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire) || self.pending.is_some()
    }
}
