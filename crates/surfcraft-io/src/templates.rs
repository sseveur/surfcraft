//! Built-in boards offered by "New board". The first one is the board the application opens with.

use surfcraft_core::Board;

/// A built-in board stored as `.brd` text. `name` and `description` are translation keys.
#[derive(Clone, Copy, Debug)]
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    brd: &'static str,
}

impl Template {
    /// The board, freshly read, named after the template.
    pub fn board(&self) -> Board {
        let mut b = crate::brd::read_text(self.brd)
            .unwrap_or_else(|e| panic!("built-in board {}: {e}", self.name));
        b.info.name = self.name.to_owned();
        b
    }
}

pub const TEMPLATES: [Template; 3] = [
    Template {
        name: "Shortboard",
        description: "6'0\" performance shortboard",
        brd: include_str!("../../../tests/fixtures/shortboard.brd"),
    },
    Template {
        name: "Mid-length",
        description: "7'6\" mid-length",
        brd: include_str!("../../../tests/fixtures/funboard.brd"),
    },
    Template {
        name: "Longboard",
        description: "9'2\" longboard",
        brd: include_str!("../../../tests/fixtures/longboard.brd"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_reads() {
        for t in TEMPLATES {
            let b = t.board();
            assert!(b.length() > 150.0, "{}", t.name);
            assert!(b.real_sections().len() >= 2);
        }
        assert!((TEMPLATES[0].board().length() - 182.88).abs() < 1e-9);
    }
}
