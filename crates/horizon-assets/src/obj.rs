//! Wavefront OBJ block models, as NeoForge's `neoforge:obj` model loader reads them for mods'
//! block models: a model JSON with `"loader": "neoforge:obj"`, `"model": "<ns>:models/<path>.obj"`
//! and optionally `"flip_v": true` names an OBJ mesh in block units (0 to 1 across a block),
//! whose materials' `map_Kd #slot` name the JSON's texture slots. Only the format Horizon draws
//! is read: vertices, texture coordinates, triangles and quads (larger faces as fans), and
//! `usemtl` materials; normals are recomputed from the faces.
use crate::json::{Result, error};

/// One face of a mesh: three or four corners, each with its texture coordinate (0 to 1 across
/// the sprite, top to bottom), and the texture slot of its material.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshFace {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub slot: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub faces: Vec<MeshFace>,
}

/// The `neoforge:obj` reference of a model JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjReference {
    /// `<ns>:models/<path>.obj`.
    pub model: String,
    pub flip_v: bool,
}

/// `map_Kd` per material of an MTL file: `#slot` names a texture slot of the model JSON; a
/// texture path names the texture itself, used as a slot of the same name.
pub fn parse_mtl(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("newmtl") => current = words.next().map(str::to_owned),
            Some("map_Kd") => {
                if let (Some(name), Some(map)) = (&current, words.next()) {
                    out.push((name.clone(), map.trim_start_matches('#').to_owned()));
                }
            }
            _ => {}
        }
    }
    out
}

/// The mesh of an OBJ file, its materials resolved through `materials` (from `parse_mtl`).
pub fn parse_obj(text: &str, materials: &[(String, String)], flip_v: bool) -> Result<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut slot = String::from("missingno");
    let mut faces = Vec::new();
    let number = |s: Option<&str>| -> Result<f32> {
        s.ok_or_else(|| error("short OBJ line"))?
            .parse::<f32>()
            .map_err(|_| error("invalid OBJ number"))
    };
    // OBJ indices count from 1; negative ones count back from the end.
    let index = |s: &str, len: usize| -> Result<usize> {
        let i: i64 = s.parse().map_err(|_| error("invalid OBJ index"))?;
        let at = if i < 0 { len as i64 + i } else { i - 1 };
        usize::try_from(at)
            .ok()
            .filter(|&a| a < len)
            .ok_or_else(|| error("OBJ index out of range"))
    };
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => positions.push([
                number(words.next())?,
                number(words.next())?,
                number(words.next())?,
            ]),
            Some("vt") => {
                let u = number(words.next())?;
                let v = number(words.next())?;
                uvs.push([u, if flip_v { 1.0 - v } else { v }]);
            }
            Some("usemtl") => {
                let name = words.next().unwrap_or_default();
                slot = materials
                    .iter()
                    .find(|(m, _)| m == name)
                    .map_or_else(|| name.to_owned(), |(_, s)| s.clone());
            }
            Some("f") => {
                let mut corners = Vec::new();
                for corner in words {
                    let mut parts = corner.split('/');
                    let p = index(parts.next().unwrap_or_default(), positions.len())?;
                    let t = match parts.next() {
                        Some(t) if !t.is_empty() => Some(index(t, uvs.len())?),
                        _ => None,
                    };
                    corners.push((positions[p], t.map_or([0.0, 0.0], |t| uvs[t])));
                }
                if corners.len() < 3 {
                    return Err(error("OBJ face with fewer than three corners"));
                }
                // Triangles and quads as they are; larger faces as a fan of triangles.
                let mut push = |c: &[([f32; 3], [f32; 2])]| {
                    faces.push(MeshFace {
                        positions: c.iter().map(|x| x.0).collect(),
                        uvs: c.iter().map(|x| x.1).collect(),
                        slot: slot.clone(),
                    })
                };
                if corners.len() <= 4 {
                    push(&corners);
                } else {
                    for i in 1..corners.len() - 1 {
                        push(&[corners[0], corners[i], corners[i + 1]]);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Mesh { faces })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_triangles_quads_and_materials() {
        let mtl = "newmtl wood\nmap_Kd #planks\nnewmtl metal\nmap_Kd create:block/metal\n";
        let obj = "mtllib x.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
                   usemtl wood\nf 1/1 2/2 3/3\nusemtl metal\nf 1/1/1 2/2/1 3/3/1 4/4/1\nf -4/1 -3/2 -2/3 -1/4 1/1\n";
        let mesh = parse_obj(obj, &parse_mtl(mtl), true).unwrap();
        assert_eq!(
            mesh.faces.len(),
            1 + 1 + 3,
            "a pentagon is a fan of three triangles"
        );
        assert_eq!(mesh.faces[0].slot, "planks");
        assert_eq!(mesh.faces[0].positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(mesh.faces[0].uvs[2], [1.0, 0.0], "flip_v turns v over");
        assert_eq!(mesh.faces[1].slot, "create:block/metal");
        assert_eq!(mesh.faces[1].positions.len(), 4);
        assert!(
            parse_obj("f 1 2 3", &[], false).is_err(),
            "indices out of range"
        );
    }
}
