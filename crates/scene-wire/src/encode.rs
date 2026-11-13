//! Writing a delta.

use crate::{FLAG_FULL, MAGIC, SceneDelta, VERSION};

/// Writes `delta` in the version 1 format.
///
/// The writer does not validate: a delta [`decode`](crate::decode) would refuse (a string over
/// 256 bytes, a mesh whose normals are fewer than its positions…) is written so that the
/// decoder refuses it too; counts and lengths that do not fit their field saturate, which no
/// reader accepts. The host builds its deltas from meshes already validated.
#[must_use]
pub fn encode(delta: &SceneDelta) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(if delta.full { FLAG_FULL } else { 0 }).to_le_bytes());
    out.extend_from_slice(&delta.revision.to_le_bytes());
    out.extend_from_slice(&delta.base.to_le_bytes());
    out.extend_from_slice(&0_u64.to_le_bytes());

    count(&mut out, delta.strings.len());
    for string in &delta.strings {
        let len = u16::try_from(string.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(string.as_bytes());
    }

    count(&mut out, delta.meshes.len());
    for mesh in &delta.meshes {
        out.extend_from_slice(&mesh.id.to_le_bytes());
        floats(&mut out, &mesh.origin, f64::to_le_bytes);
        count(&mut out, mesh.positions.len());
        count(&mut out, mesh.indices.len());
        for position in &mesh.positions {
            floats(&mut out, position, f32::to_le_bytes);
        }
        for normal in &mesh.normals {
            floats(&mut out, normal, f32::to_le_bytes);
        }
        for index in &mesh.indices {
            out.extend_from_slice(&index.to_le_bytes());
        }
        floats(&mut out, &mesh.bounds_min, f32::to_le_bytes);
        floats(&mut out, &mesh.bounds_max, f32::to_le_bytes);
    }

    count(&mut out, delta.removed_meshes.len());
    for id in &delta.removed_meshes {
        out.extend_from_slice(&id.to_le_bytes());
    }

    count(&mut out, delta.elements.len());
    for element in &delta.elements {
        out.extend_from_slice(&element.id);
        out.extend_from_slice(&element.mesh.to_le_bytes());
        floats(&mut out, &element.rotation, f64::to_le_bytes);
        floats(&mut out, &element.translation, f64::to_le_bytes);
        out.extend_from_slice(&element.category.to_le_bytes());
        out.extend_from_slice(&element.discipline.to_le_bytes());
    }

    count(&mut out, delta.removed_elements.len());
    for id in &delta.removed_elements {
        out.extend_from_slice(id);
    }
    out
}

fn count(out: &mut Vec<u8>, len: usize) {
    let len = u32::try_from(len).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
}

fn floats<T: Copy, const W: usize>(out: &mut Vec<u8>, values: &[T], to_bytes: fn(T) -> [u8; W]) {
    for &value in values {
        out.extend_from_slice(&to_bytes(value));
    }
}
