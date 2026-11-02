//! Triangle mesh files. Coordinates are written in millimetres: x along the board, y across, z up.

use surfcraft_core::Mesh;

const MM_PER_CM: f64 = 10.0;

/// Common model scales for scale models: (label, factor).
pub const SCALES: [(&str, f64); 4] = [("1:1", 1.0), ("1:5", 0.2), ("1:10", 0.1), ("1:20", 0.05)];

/// A copy of the mesh scaled about the origin (normals are unchanged by a uniform scale).
pub fn scaled(mesh: &Mesh, factor: f64) -> Mesh {
    if factor == 1.0 {
        return mesh.clone();
    }
    Mesh {
        positions: mesh.positions.iter().map(|p| *p * factor).collect(),
        normals: mesh.normals.clone(),
        triangles: mesh.triangles.clone(),
    }
}

/// Binary STL.
pub fn stl(mesh: &Mesh, name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + mesh.triangles.len() * 50);
    let mut header = format!("SurfCraft {name}").into_bytes();
    header.resize(80, b' ');
    out.extend_from_slice(&header);
    out.extend_from_slice(&(mesh.triangles.len() as u32).to_le_bytes());
    for &t in &mesh.triangles {
        let n = mesh.face_normal(t);
        for v in [n.x, n.y, n.z] {
            out.extend_from_slice(&(v as f32).to_le_bytes());
        }
        for i in t {
            let p = mesh.positions[i as usize] * MM_PER_CM;
            for v in [p.x, p.y, p.z] {
                out.extend_from_slice(&(v as f32).to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

/// Wavefront OBJ with per-vertex normals.
pub fn obj(mesh: &Mesh, name: &str) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# SurfCraft board, millimetres\no {}",
        name.replace(char::is_whitespace, "_")
    );
    for p in &mesh.positions {
        let p = *p * MM_PER_CM;
        let _ = writeln!(s, "v {:.4} {:.4} {:.4}", p.x, p.y, p.z);
    }
    for n in &mesh.normals {
        let _ = writeln!(s, "vn {:.5} {:.5} {:.5}", n.x, n.y, n.z);
    }
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| i + 1);
        let _ = writeln!(s, "f {a}//{a} {b}//{b} {c}//{c}");
    }
    s
}

/// Read back a binary STL's triangle count and vertices (for tests and sanity checks).
pub fn stl_triangles(bytes: &[u8]) -> Option<Vec<[[f32; 3]; 3]>> {
    let count = u32::from_le_bytes(bytes.get(80..84)?.try_into().ok()?) as usize;
    let mut tris = Vec::with_capacity(count);
    for i in 0..count {
        let base = 84 + i * 50 + 12;
        let mut tri = [[0f32; 3]; 3];
        for (v, vert) in tri.iter_mut().enumerate() {
            for (c, coord) in vert.iter_mut().enumerate() {
                let o = base + v * 12 + c * 4;
                *coord = f32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?);
            }
        }
        tris.push(tri);
    }
    Some(tris)
}
