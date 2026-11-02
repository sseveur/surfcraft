//! glTF 2.0 binary (`.glb`): one mesh with positions, normals and u32 indices in a single buffer, and a plain
//! PBR material in a light foam colour.
//!
//! glTF uses metres and a right-handed, Y-up frame whose front faces +Z. The board is mapped as
//! `glTF (X, Y, Z) = board (y across, z up, x along)`: the deck faces +Y and the nose points +Z. This is a cyclic
//! permutation of the axes, so handedness and triangle winding are unchanged. `scale` gives scale models.

use serde_json::json;
use surfcraft_core::{DVec3, Mesh};

const M_PER_CM: f64 = 0.01;
pub const MAGIC: u32 = 0x4654_6C67; // "glTF"
pub const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
pub const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"

/// Light board (foam under clear glass) colour, linear RGBA.
pub const BOARD_COLOUR: [f64; 4] = [0.92, 0.9, 0.84, 1.0];

/// Board coordinates (cm) to glTF (m, Y up).
pub fn to_gltf(p: DVec3, scale: f64) -> [f32; 3] {
    let f = M_PER_CM * scale;
    [(p.y * f) as f32, (p.z * f) as f32, (p.x * f) as f32]
}

fn pad(v: &mut Vec<u8>, with: u8) {
    while !v.len().is_multiple_of(4) {
        v.push(with);
    }
}

/// A complete `.glb` file.
pub fn write(mesh: &Mesh, name: &str, scale: f64) -> Vec<u8> {
    let n = mesh.positions.len();
    let mut bin = Vec::with_capacity(n * 24 + mesh.triangles.len() * 12);
    let mut lo = [0f32; 3];
    let mut hi = [0f32; 3];
    for (i, &p) in mesh.positions.iter().enumerate() {
        let g = to_gltf(p, scale);
        for k in 0..3 {
            if i == 0 {
                lo[k] = g[k];
                hi[k] = g[k];
            }
            lo[k] = lo[k].min(g[k]);
            hi[k] = hi[k].max(g[k]);
            bin.extend_from_slice(&g[k].to_le_bytes());
        }
    }
    let normals_offset = bin.len();
    for i in 0..n {
        // Normals must be unit length; fall back to "up" where a vertex has none.
        let nm = mesh
            .normals
            .get(i)
            .copied()
            .filter(|v| v.length() > 0.5)
            .unwrap_or(DVec3::Z)
            .normalize();
        for c in [nm.y as f32, nm.z as f32, nm.x as f32] {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    let indices_offset = bin.len();
    for t in &mesh.triangles {
        for i in t {
            bin.extend_from_slice(&i.to_le_bytes());
        }
    }
    let indices_len = bin.len() - indices_offset;
    let doc = json!({
        "asset": {"version": "2.0", "generator": "SurfCraft"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": name}],
        "meshes": [{"name": name, "primitives": [{
            "attributes": {"POSITION": 0, "NORMAL": 1},
            "indices": 2,
            "material": 0,
            "mode": 4
        }]}],
        "materials": [{
            "name": "Board",
            "pbrMetallicRoughness": {"baseColorFactor": BOARD_COLOUR, "metallicFactor": 0.0, "roughnessFactor": 0.35}
        }],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": normals_offset, "byteStride": 12, "target": 34962},
            {"buffer": 0, "byteOffset": normals_offset, "byteLength": indices_offset - normals_offset, "byteStride": 12, "target": 34962},
            {"buffer": 0, "byteOffset": indices_offset, "byteLength": indices_len, "target": 34963}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": n, "type": "VEC3", "min": lo, "max": hi},
            {"bufferView": 1, "componentType": 5126, "count": n, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5125, "count": mesh.triangles.len() * 3, "type": "SCALAR"}
        ]
    });
    let mut json = serde_json::to_vec(&doc).expect("glTF JSON");
    pad(&mut json, b' ');
    pad(&mut bin, 0);
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
    out.extend_from_slice(&bin);
    out
}

/// Split a `.glb` into its JSON document and binary chunk, checking the header, chunk types, lengths and
/// 4-byte alignment.
pub fn read(bytes: &[u8]) -> Result<(serde_json::Value, Vec<u8>), String> {
    let u = |o: usize| -> Result<u32, String> {
        let b = bytes.get(o..o + 4).ok_or("truncated glb")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if u(0)? != MAGIC || u(4)? != 2 {
        return Err("not a glTF 2.0 binary".into());
    }
    if u(8)? as usize != bytes.len() {
        return Err("length in header does not match file".into());
    }
    let json_len = u(12)? as usize;
    if u(16)? != CHUNK_JSON || !json_len.is_multiple_of(4) {
        return Err("bad JSON chunk".into());
    }
    let json = bytes.get(20..20 + json_len).ok_or("truncated JSON")?;
    let b = 20 + json_len;
    let bin_len = u(b)? as usize;
    if u(b + 4)? != CHUNK_BIN || !bin_len.is_multiple_of(4) || b + 8 + bin_len != bytes.len() {
        return Err("bad BIN chunk".into());
    }
    let doc = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    Ok((doc, bytes[b + 8..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_map_nose_forward_deck_up() {
        // 1 m along, 0.5 m across, 0.1 m up.
        assert_eq!(to_gltf(DVec3::new(100.0, 50.0, 10.0), 1.0), [0.5, 0.1, 1.0]);
        let g = to_gltf(DVec3::new(100.0, 0.0, 0.0), 0.1);
        assert!((g[2] - 0.1).abs() < 1e-7);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read(b"glTF").is_err());
        assert!(read(&[0; 40]).is_err());
    }
}
