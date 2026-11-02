//! 3MF (3D Manufacturing Format) for 3D printing: core specification, one mesh object in millimetres.
//!
//! The package is a ZIP holding `[Content_Types].xml`, `_rels/.rels` and `3D/3dmodel.model`. Vertices are
//! deduplicated (shared between triangles) and degenerate triangles dropped, as the specification requires. The
//! model is moved so its bounding box starts at the origin (on the print bed); `scale` gives scale models
//! (0.1 = 1:10).

use std::collections::HashMap;
use std::fmt::Write;

use surfcraft_core::Mesh;

use crate::zip::ZipWriter;

const MM_PER_CM: f64 = 10.0;

pub const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
 <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
 <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>
"#;

pub const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
 <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>
"#;

pub const MODEL_PATH: &str = "3D/3dmodel.model";

/// Escape text for an XML attribute or element.
fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => o.push(' '),
            c => o.push(c),
        }
    }
    o
}

/// Vertices in mm (deduplicated after rounding to 1 µm) and triangles over them, without degenerate ones.
pub fn indexed_mm(mesh: &Mesh, scale: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let f = MM_PER_CM * scale;
    let mut map: HashMap<[i64; 3], u32> = HashMap::with_capacity(mesh.positions.len());
    let mut vertices = Vec::with_capacity(mesh.positions.len());
    let remap: Vec<u32> = mesh
        .positions
        .iter()
        .map(|p| {
            let v = [p.x * f, p.y * f, p.z * f];
            let key = v.map(|c| (c * 1000.0).round() as i64);
            *map.entry(key).or_insert_with(|| {
                vertices.push(v);
                (vertices.len() - 1) as u32
            })
        })
        .collect();
    let triangles = mesh
        .triangles
        .iter()
        .map(|t| t.map(|i| remap[i as usize]))
        .filter(|[a, b, c]| a != b && b != c && a != c)
        .collect();
    (vertices, triangles)
}

/// The `3D/3dmodel.model` XML.
pub fn model_xml(mesh: &Mesh, name: &str, scale: f64) -> String {
    let (mut vertices, triangles) = indexed_mm(mesh, scale);
    // Sit the model on the bed at the origin.
    let mut lo = [0.0f64; 3];
    for k in 0..3 {
        lo[k] = vertices.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min);
        if !lo[k].is_finite() {
            lo[k] = 0.0;
        }
    }
    for v in &mut vertices {
        for k in 0..3 {
            v[k] -= lo[k];
        }
    }
    let name = xml_escape(name);
    let mut s = String::with_capacity(64 * vertices.len() + 48 * triangles.len() + 1024);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n");
    let _ = writeln!(s, " <metadata name=\"Title\">{name}</metadata>");
    s.push_str(" <metadata name=\"Application\">SurfCraft</metadata>\n");
    if (scale - 1.0).abs() > 1e-12 && scale > 0.0 {
        let _ = writeln!(
            s,
            " <metadata name=\"Description\">Scale 1:{}</metadata>",
            fmt_ratio(1.0 / scale)
        );
    }
    s.push_str(" <resources>\n");
    let _ = writeln!(s, "  <object id=\"1\" type=\"model\" name=\"{name}\">");
    s.push_str("   <mesh>\n    <vertices>\n");
    for v in &vertices {
        let _ = writeln!(
            s,
            "     <vertex x=\"{:.4}\" y=\"{:.4}\" z=\"{:.4}\"/>",
            v[0], v[1], v[2]
        );
    }
    s.push_str("    </vertices>\n    <triangles>\n");
    for [a, b, c] in &triangles {
        let _ = writeln!(s, "     <triangle v1=\"{a}\" v2=\"{b}\" v3=\"{c}\"/>");
    }
    s.push_str("    </triangles>\n   </mesh>\n  </object>\n </resources>\n");
    s.push_str(" <build>\n  <item objectid=\"1\"/>\n </build>\n</model>\n");
    s
}

fn fmt_ratio(r: f64) -> String {
    let s = format!("{r:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A complete `.3mf` package.
pub fn write(mesh: &Mesh, name: &str, scale: f64) -> Vec<u8> {
    let mut z = ZipWriter::new();
    z.add("[Content_Types].xml", CONTENT_TYPES.as_bytes());
    z.add("_rels/.rels", RELS.as_bytes());
    z.add(MODEL_PATH, model_xml(mesh, name, scale).as_bytes());
    z.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use surfcraft_core::DVec3;

    fn tetra() -> Mesh {
        let mut m = Mesh {
            positions: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 0.0, 0.0),
                DVec3::new(0.0, 1.0, 0.0),
                DVec3::new(0.0, 0.0, 1.0),
                DVec3::new(1.0, 0.0, 0.0), // duplicate of 1
            ],
            normals: vec![],
            triangles: vec![[0, 2, 1], [0, 4, 3], [0, 3, 2], [1, 2, 3], [1, 4, 2]],
        };
        m.compute_normals();
        m
    }

    #[test]
    fn dedups_and_drops_degenerate() {
        let (v, t) = indexed_mm(&tetra(), 1.0);
        assert_eq!(v.len(), 4);
        assert_eq!(
            t.len(),
            4,
            "the triangle using both copies of vertex 1 is dropped"
        );
        assert_eq!(v[1], [10.0, 0.0, 0.0]);
    }

    #[test]
    fn package_layout() {
        let bytes = write(&tetra(), "a<b>&\"c\"", 0.1);
        let entries = crate::zip::read(&bytes).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", MODEL_PATH]);
        let xml = String::from_utf8(entries[2].1.clone()).unwrap();
        assert!(xml.contains("unit=\"millimeter\""));
        assert!(xml.contains("a&lt;b&gt;&amp;&quot;c&quot;"));
        assert!(xml.contains("Scale 1:10<"));
        assert_eq!(xml.matches("<vertex ").count(), 4);
        assert_eq!(xml.matches("<triangle ").count(), 4);
        // 1:10 of a 1 cm edge is 1 mm.
        assert!(xml.contains("x=\"1.0000\""));
    }
}
