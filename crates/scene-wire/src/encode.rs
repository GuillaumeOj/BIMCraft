//! Writing a delta.

use crate::{FLAG_FULL, HEADER_LEN, MAGIC, SceneDelta, VERSION};

/// Writes `delta` in the version 1 format.
///
/// The writer does not validate: a delta [`decode`](crate::decode) would refuse (a string over
/// 256 bytes, an index out of range…) is written so that the decoder refuses it too; counts and
/// lengths that do not fit their field saturate, which no reader accepts. The exception is a mesh
/// with fewer normals than positions: the writer writes the normals it has, so what follows
/// shifts and is read as something else, which the decoder may or may not refuse. Callers give
/// every position a normal, as the host does. The host builds its deltas from meshes already
/// validated.
#[must_use]
pub fn encode(delta: &SceneDelta) -> Vec<u8> {
    let mut out = Vec::with_capacity(encoded_len(delta));
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

/// The size of the bytes [`encode`] writes, so the buffer is allocated once: a delta may hold
/// hundreds of megabytes, which growing by doubling would copy several times.
fn encoded_len(delta: &SceneDelta) -> usize {
    let strings: usize = delta.strings.iter().map(|s| 2 + s.len()).sum();
    let meshes: usize = delta
        .meshes
        .iter()
        .map(|m| {
            8 + 24 + 4 + 4 + 12 * (m.positions.len() + m.normals.len()) + 4 * m.indices.len() + 24
        })
        .sum();
    HEADER_LEN
        + 5 * 4
        + strings
        + meshes
        + 8 * delta.removed_meshes.len()
        + 128 * delta.elements.len()
        + 16 * delta.removed_elements.len()
}
