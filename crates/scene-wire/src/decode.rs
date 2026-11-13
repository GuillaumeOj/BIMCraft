//! Reading a delta, input hostile.

use crate::check::{in_world, is_orthonormal, is_unit, point_in_world};
use crate::{
    Element, FLAG_FULL, HEADER_LEN, MAGIC, MAX_DELTA_BYTES, MAX_ELEMENTS, MAX_MESH_INDICES,
    MAX_MESH_VERTICES, MAX_MESHES, MAX_STRING_BYTES, MAX_STRINGS, Mesh, SceneDelta, Section,
    VERSION, WireError,
};

/// The size of the smallest added mesh: id, origin, counts, bounds (no vertices).
const MESH_MIN_BYTES: usize = 8 + 24 + 4 + 4 + 24 + 24;
/// The size of an element record.
const ELEMENT_BYTES: usize = 16 + 8 + 72 + 24 + 4 + 4;

/// Reads a delta, checking every length against the buffer before reading, and every value
/// against its range.
///
/// # Errors
///
/// A [`WireError`] naming what is wrong and where, for input that is too large, truncated,
/// of another version, or whose counts, indices or values break the format.
pub fn decode(bytes: &[u8]) -> Result<SceneDelta, WireError> {
    if bytes.len() > MAX_DELTA_BYTES {
        return Err(WireError::TooLarge {
            max: MAX_DELTA_BYTES,
        });
    }
    let mut reader = Reader { rest: bytes };
    let mut delta = SceneDelta::default();
    read_header(&mut reader, &mut delta)?;
    delta.strings = read_strings(&mut reader)?;
    delta.meshes = read_meshes(&mut reader)?;
    delta.removed_meshes = read_ids(&mut reader, Section::RemovedMeshes, MAX_MESHES, |chunk| {
        u64::from_le_bytes(chunk)
    })?;
    delta.elements = read_elements(&mut reader, delta.strings.len())?;
    delta.removed_elements = read_ids(
        &mut reader,
        Section::RemovedElements,
        MAX_ELEMENTS,
        |chunk| chunk,
    )?;
    if reader.rest.is_empty() {
        Ok(delta)
    } else {
        Err(WireError::TrailingBytes {
            count: reader.rest.len(),
        })
    }
}

struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], WireError> {
        if len > self.rest.len() {
            return Err(WireError::Truncated);
        }
        let (head, tail) = self.rest.split_at(len);
        self.rest = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn f64s<const N: usize>(&mut self) -> Result<[f64; N], WireError> {
        let mut out = [0.0; N];
        for value in &mut out {
            *value = f64::from_le_bytes(self.array()?);
        }
        Ok(out)
    }

    fn f32s<const N: usize>(&mut self) -> Result<[f32; N], WireError> {
        let mut out = [0.0; N];
        for value in &mut out {
            *value = f32::from_le_bytes(self.array()?);
        }
        Ok(out)
    }

    /// A section's entry count, below `max`, and small enough for the bytes that remain at
    /// `entry_min` bytes an entry, so a lying count allocates nothing.
    fn count(
        &mut self,
        section: Section,
        max: usize,
        entry_min: usize,
    ) -> Result<usize, WireError> {
        let count = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        if count > max {
            return Err(WireError::TooMany {
                section,
                count,
                max,
            });
        }
        if count.saturating_mul(entry_min) > self.rest.len() {
            return Err(WireError::Truncated);
        }
        Ok(count)
    }
}

fn read_header(reader: &mut Reader<'_>, delta: &mut SceneDelta) -> Result<(), WireError> {
    if HEADER_LEN > reader.rest.len() {
        return Err(WireError::Truncated);
    }
    if reader.array::<4>()? != MAGIC {
        return Err(WireError::BadMagic);
    }
    let version = reader.u16()?;
    if version != VERSION {
        return Err(WireError::UnsupportedVersion { version });
    }
    let flags = reader.u16()?;
    if flags & !FLAG_FULL != 0 {
        return Err(WireError::UnknownFlags);
    }
    delta.full = flags & FLAG_FULL != 0;
    delta.revision = reader.u64()?;
    delta.base = reader.u64()?;
    if reader.u64()? != 0 {
        return Err(WireError::ReservedNotZero);
    }
    if delta.full && delta.base != 0 {
        return Err(WireError::FullWithBase);
    }
    Ok(())
}

fn read_strings(reader: &mut Reader<'_>) -> Result<Vec<String>, WireError> {
    let count = reader.count(Section::Strings, MAX_STRINGS, 2)?;
    let mut strings = Vec::with_capacity(count);
    for index in 0..count {
        let len = usize::from(reader.u16()?);
        if len > MAX_STRING_BYTES {
            return Err(WireError::StringTooLong { index, len });
        }
        let text =
            std::str::from_utf8(reader.take(len)?).map_err(|_| WireError::InvalidUtf8 { index })?;
        strings.push(text.to_owned());
    }
    Ok(strings)
}

fn read_meshes(reader: &mut Reader<'_>) -> Result<Vec<Mesh>, WireError> {
    let count = reader.count(Section::Meshes, MAX_MESHES, MESH_MIN_BYTES)?;
    let mut meshes = Vec::with_capacity(count);
    for index in 0..count {
        meshes.push(read_mesh(reader, index)?);
    }
    reject_duplicates(meshes.iter().map(|m| m.id), |index| {
        WireError::DuplicateMesh { index }
    })?;
    Ok(meshes)
}

fn read_mesh(reader: &mut Reader<'_>, index: usize) -> Result<Mesh, WireError> {
    let id = reader.u64()?;
    let origin = reader.f64s::<3>()?;
    let vertices = usize::try_from(reader.u32()?).unwrap_or(usize::MAX);
    let index_count = usize::try_from(reader.u32()?).unwrap_or(usize::MAX);
    for (count, max) in [
        (vertices, MAX_MESH_VERTICES),
        (index_count, MAX_MESH_INDICES),
    ] {
        if count > max {
            return Err(WireError::MeshTooLarge { index, count, max });
        }
    }
    // Both counts are bounded by the limits above, so the sum cannot overflow; the take fails
    // before any allocation when the buffer is shorter.
    let payload = reader.take(vertices * 24 + index_count * 4)?;
    let (positions, rest) = payload.split_at(vertices * 12);
    let (normals, indices) = rest.split_at(vertices * 12);
    let mesh = Mesh {
        id,
        origin,
        positions: vec3s(positions),
        normals: vec3s(normals),
        indices: indices
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&c| u32::from_le_bytes(c))
            .collect(),
        bounds_min: reader.f32s()?,
        bounds_max: reader.f32s()?,
    };
    check_mesh(&mesh, index)?;
    Ok(mesh)
}

fn vec3s(bytes: &[u8]) -> Vec<[f32; 3]> {
    bytes
        .as_chunks::<12>()
        .0
        .iter()
        .map(|c| {
            let component = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
            [component(0), component(4), component(8)]
        })
        .collect()
}

fn check_mesh(mesh: &Mesh, index: usize) -> Result<(), WireError> {
    if mesh.positions.is_empty() || mesh.indices.is_empty() {
        return Err(WireError::EmptyMesh { index });
    }
    if !mesh.origin.iter().all(|&c| in_world(c))
        || !mesh.positions.iter().all(|&p| point_in_world(p))
        || !point_in_world(mesh.bounds_min)
        || !point_in_world(mesh.bounds_max)
    {
        return Err(WireError::MeshCoordinate { index });
    }
    if mesh
        .bounds_min
        .iter()
        .zip(&mesh.bounds_max)
        .any(|(lo, hi)| lo > hi)
    {
        return Err(WireError::InvertedBounds { index });
    }
    if let Some(vertex) = mesh.normals.iter().position(|&n| !is_unit(n)) {
        return Err(WireError::InvalidNormal { index, vertex });
    }
    if !mesh.indices.len().is_multiple_of(3) {
        return Err(WireError::IndexCountNotMultipleOfThree { index });
    }
    let vertices = mesh.positions.len();
    match mesh
        .indices
        .iter()
        .position(|&i| usize::try_from(i).map_or(true, |i| i >= vertices))
    {
        Some(position) => Err(WireError::IndexOutOfRange { index, position }),
        None => Ok(()),
    }
}

fn read_elements(reader: &mut Reader<'_>, strings: usize) -> Result<Vec<Element>, WireError> {
    let count = reader.count(Section::Elements, MAX_ELEMENTS, ELEMENT_BYTES)?;
    let mut elements = Vec::with_capacity(count);
    for index in 0..count {
        let element = Element {
            id: reader.array()?,
            mesh: reader.u64()?,
            rotation: reader.f64s()?,
            translation: reader.f64s()?,
            category: reader.u32()?,
            discipline: reader.u32()?,
        };
        if !element.translation.iter().all(|&c| in_world(c)) {
            return Err(WireError::ElementTranslation { index });
        }
        if !is_orthonormal(&element.rotation) {
            return Err(WireError::InvalidRotation { index });
        }
        let in_range = |i: u32| usize::try_from(i).is_ok_and(|i| i < strings);
        if !in_range(element.category) || !in_range(element.discipline) {
            return Err(WireError::StringIndexOutOfRange { index });
        }
        elements.push(element);
    }
    reject_duplicates(elements.iter().map(|e| e.id), |index| {
        WireError::DuplicateElement { index }
    })?;
    Ok(elements)
}

/// A list of ids (`N` bytes each), whose count is checked against the bytes left first.
fn read_ids<T, const N: usize>(
    reader: &mut Reader<'_>,
    section: Section,
    max: usize,
    make: fn([u8; N]) -> T,
) -> Result<Vec<T>, WireError> {
    let count = reader.count(section, max, N)?;
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(make(reader.array()?));
    }
    Ok(ids)
}

/// Fails with `error(position)` at the position of the first id seen before.
fn reject_duplicates<K: Ord + Copy>(
    keys: impl Iterator<Item = K>,
    error: impl FnOnce(usize) -> WireError,
) -> Result<(), WireError> {
    let mut seen: Vec<(K, usize)> = keys.enumerate().map(|(i, k)| (k, i)).collect();
    seen.sort_unstable();
    match seen
        .windows(2)
        .filter(|w| w[0].0 == w[1].0)
        .map(|w| w[1].1)
        .min()
    {
        Some(index) => Err(error(index)),
        None => Ok(()),
    }
}
