//! 3MF and glTF exports of a real board, checked against the STL mesh.

use surfcraft_core::{Mesh, MeshDetail};
use surfcraft_export::{gltf, mesh_formats, threemf, zip};

fn board() -> surfcraft_core::Board {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/shortboard.brd"
    ))
    .unwrap();
    surfcraft_io::brd::read(&bytes).unwrap()
}

#[test]
fn threemf_package_matches_stl_mesh() {
    let b = board();
    let mesh = Mesh::of_board(&b, MeshDetail::default());
    assert!(mesh.is_watertight());
    let stl_tris = mesh_formats::stl_triangles(&mesh_formats::stl(&mesh, "x"))
        .unwrap()
        .len();
    for (label, scale) in mesh_formats::SCALES {
        let bytes = threemf::write(&mesh, "shortboard", scale);
        let entries = zip::read(&bytes).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            names,
            ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]
        );
        let ct = String::from_utf8(entries[0].1.clone()).unwrap();
        assert!(ct.contains("Extension=\"model\"") && ct.contains("Extension=\"rels\""));
        let rels = String::from_utf8(entries[1].1.clone()).unwrap();
        assert!(rels.contains("Target=\"/3D/3dmodel.model\""));
        let xml = String::from_utf8(entries[2].1.clone()).unwrap();
        assert!(xml.contains("<model unit=\"millimeter\""));
        // Every vertex of the watertight mesh is distinct, so the counts equal the STL mesh's.
        assert_eq!(xml.matches("<vertex ").count(), mesh.positions.len());
        assert_eq!(xml.matches("<triangle ").count(), stl_tris, "{label}");
        // Extent along x = length at scale, starting at 0.
        let xs: Vec<f64> = xml
            .split("<vertex x=\"")
            .skip(1)
            .map(|s| s.split('"').next().unwrap().parse().unwrap())
            .collect();
        let lo = xs.iter().copied().fold(f64::MAX, f64::min);
        let hi = xs.iter().copied().fold(f64::MIN, f64::max);
        assert!(lo.abs() < 1e-3);
        assert!(
            (hi - b.length() * 10.0 * scale).abs() < 2.0 * scale + 0.01,
            "{label}: {hi}"
        );
    }
}

#[test]
fn glb_structure_and_accessors() {
    let b = board();
    let mesh = Mesh::of_board(&b, MeshDetail::default());
    let bytes = gltf::write(&mesh, "short \"board\"", 1.0);
    assert_eq!(bytes.len() % 4, 0);
    assert_eq!(&bytes[0..4], b"glTF");
    let (doc, bin) = gltf::read(&bytes).unwrap();
    assert_eq!(doc["asset"]["version"], "2.0");
    assert_eq!(doc["nodes"][0]["name"], "short \"board\"");
    assert_eq!(
        doc["buffers"][0]["byteLength"].as_u64().unwrap() as usize,
        bin.len()
    );
    let n = mesh.positions.len();
    let acc = &doc["accessors"];
    assert_eq!(acc[0]["count"].as_u64().unwrap() as usize, n);
    assert_eq!(acc[1]["count"].as_u64().unwrap() as usize, n);
    assert_eq!(
        acc[2]["count"].as_u64().unwrap() as usize,
        mesh.triangles.len() * 3
    );
    assert_eq!(acc[2]["componentType"], 5125);
    let views = doc["bufferViews"].as_array().unwrap();
    for v in views {
        assert_eq!(v["byteOffset"].as_u64().unwrap() % 4, 0);
    }
    let end = views
        .iter()
        .map(|v| v["byteOffset"].as_u64().unwrap() + v["byteLength"].as_u64().unwrap())
        .max()
        .unwrap() as usize;
    assert!(end <= bin.len());
    // POSITION min/max match the data; metres, nose along +Z, deck up +Y.
    let f = |o: usize| f32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for i in 0..n {
        for k in 0..3 {
            lo[k] = lo[k].min(f(i * 12 + k * 4));
            hi[k] = hi[k].max(f(i * 12 + k * 4));
        }
    }
    for k in 0..3 {
        assert_eq!(acc[0]["min"][k].as_f64().unwrap() as f32, lo[k]);
        assert_eq!(acc[0]["max"][k].as_f64().unwrap() as f32, hi[k]);
    }
    assert!(((hi[2] - lo[2]) as f64 - b.length() / 100.0).abs() < 0.003);
    assert!(((hi[0] - lo[0]) as f64 - b.max_width() / 100.0).abs() < 0.003);
    // Normals are unit length; indices in range.
    let no = views[1]["byteOffset"].as_u64().unwrap() as usize;
    for i in 0..n {
        let v = [f(no + i * 12), f(no + i * 12 + 4), f(no + i * 12 + 8)];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        assert!((l - 1.0).abs() < 1e-3, "normal {i}: {l}");
    }
    let io = views[2]["byteOffset"].as_u64().unwrap() as usize;
    for i in 0..mesh.triangles.len() * 3 {
        let idx = u32::from_le_bytes(bin[io + i * 4..io + i * 4 + 4].try_into().unwrap());
        assert!((idx as usize) < n);
    }
    // 1:10 shrinks the extent tenfold.
    let small = gltf::write(&mesh, "s", 0.1);
    let (d2, _) = gltf::read(&small).unwrap();
    let z = d2["accessors"][0]["max"][2].as_f64().unwrap();
    assert!((z - hi[2] as f64 / 10.0).abs() < 1e-4);
}

#[test]
fn scaled_stl_is_smaller() {
    let b = board();
    let mesh = Mesh::of_board(&b, MeshDetail::PREVIEW);
    let small = mesh_formats::scaled(&mesh, 0.1);
    let tris = mesh_formats::stl_triangles(&mesh_formats::stl(&small, "s")).unwrap();
    let max_x = tris.iter().flatten().map(|v| v[0]).fold(f32::MIN, f32::max);
    assert!((max_x as f64 - b.length()).abs() < 0.3, "{max_x}");
    assert!((small.volume() - mesh.volume() / 1000.0).abs() < 1e-6 * mesh.volume());
}
