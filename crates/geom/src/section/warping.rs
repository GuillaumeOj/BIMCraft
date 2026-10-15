//! The Saint-Venant warping function on linear triangles, and the shear centre it gives
//! (Trefftz). Rationale: `docs/dev/adr/0011-shear-centre.md`.

use super::ShearCentreError;
use super::mesh::Mesh;
use crate::Point2;

/// Residual, relative to the load, at which the conjugate gradients stop.
const RELATIVE_RESIDUAL: f64 = 1e-12;

/// A symmetric sparse matrix, row by row, columns sorted.
struct Sparse {
    rows: Vec<Vec<(usize, f64)>>,
}

impl Sparse {
    fn product(&self, x: &[f64]) -> Vec<f64> {
        self.rows
            .iter()
            .map(|row| row.iter().map(|&(j, k)| k * x[j]).sum())
            .collect()
    }

    fn diagonal(&self) -> Vec<f64> {
        self.rows
            .iter()
            .enumerate()
            .map(|(i, row)| row.iter().find(|&&(j, _)| j == i).map_or(0.0, |&(_, k)| k))
            .collect()
    }
}

/// One linear triangle, in coordinates relative to the section's centroid.
struct Element {
    nodes: [usize; 3],
    x: [f64; 3],
    y: [f64; 3],
    area: f64,
    /// Gradients of the three shape functions, constant over the element.
    dx: [f64; 3],
    dy: [f64; 3],
}

impl Element {
    fn new(nodes: [usize; 3], points: &[Point2], centroid: Point2) -> Self {
        let x = nodes.map(|n| points[n].x - centroid.x);
        let y = nodes.map(|n| points[n].y - centroid.y);
        let twice_area = (x[1] - x[0]) * (y[2] - y[0]) - (x[2] - x[0]) * (y[1] - y[0]);
        let dx = [y[1] - y[2], y[2] - y[0], y[0] - y[1]].map(|v| v / twice_area);
        let dy = [x[2] - x[1], x[0] - x[2], x[1] - x[0]].map(|v| v / twice_area);
        Self {
            nodes,
            x,
            y,
            area: twice_area / 2.0,
            dx,
            dy,
        }
    }

    /// ∫ u v over the element, for linear fields given at the nodes.
    fn product(&self, u: [f64; 3], v: [f64; 3]) -> f64 {
        let sum = |w: [f64; 3]| w[0] + w[1] + w[2];
        let diagonal = u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
        self.area / 12.0 * (sum(u) * sum(v) + diagonal)
    }
}

/// The shear centre of the meshed section, whose centroid is `centroid`.
pub(crate) fn shear_centre(mesh: &Mesh, centroid: Point2) -> Result<Point2, ShearCentreError> {
    // Conjugate gradients converge in at most n steps in exact arithmetic; rounding needs more.
    solve_with(mesh, centroid, 4 * mesh.nodes.len())
}

/// [`shear_centre`] with a given cap on the solver's iterations.
pub(crate) fn solve_with(
    mesh: &Mesh,
    centroid: Point2,
    iterations: usize,
) -> Result<Point2, ShearCentreError> {
    let elements: Vec<Element> = mesh
        .triangles
        .iter()
        .map(|&t| Element::new(t, &mesh.nodes, centroid))
        .collect();
    let (stiffness, load) = assemble(&elements, mesh.nodes.len());
    let warping = solve(&stiffness, &load, iterations)?;
    let (mut ixx, mut iyy, mut ixy, mut ixw, mut iyw) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for e in &elements {
        let w = e.nodes.map(|n| warping[n]);
        ixx += e.product(e.y, e.y);
        iyy += e.product(e.x, e.x);
        ixy += e.product(e.x, e.y);
        ixw += e.product(e.x, w);
        iyw += e.product(e.y, w);
    }
    // Rotating about (a, b) adds a y − b x to the warping; the shear centre makes the result
    // orthogonal to x and y.
    let det = ixx * iyy - ixy * ixy;
    let a = (ixw * ixy - iyy * iyw) / det;
    let b = (ixx * ixw - ixy * iyw) / det;
    Ok(Point2::new(centroid.x + a, centroid.y + b))
}

/// The stiffness matrix ∫ ∇Nᵢ·∇Nⱼ and the load ∫ ∇Nᵢ·(y, −x), which equals the boundary term
/// ∮ Nᵢ (y nₓ − x `n_y`) by the divergence theorem, since (y, −x) is divergence-free.
fn assemble(elements: &[Element], nodes: usize) -> (Sparse, Vec<f64>) {
    let mut rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); nodes];
    let mut load = vec![0.0; nodes];
    for e in elements {
        let (mean_x, mean_y) = (
            (e.x[0] + e.x[1] + e.x[2]) / 3.0,
            (e.y[0] + e.y[1] + e.y[2]) / 3.0,
        );
        for i in 0..3 {
            load[e.nodes[i]] += e.area * (e.dx[i] * mean_y - e.dy[i] * mean_x);
            for j in 0..3 {
                let k = e.area * (e.dx[i] * e.dx[j] + e.dy[i] * e.dy[j]);
                rows[e.nodes[i]].push((e.nodes[j], k));
            }
        }
    }
    for row in &mut rows {
        // Stable sort, then merge: the sums happen in element order, the same on every target.
        row.sort_by_key(|&(j, _)| j);
        row.dedup_by(|next, kept| {
            if next.0 == kept.0 {
                kept.1 += next.1;
                true
            } else {
                false
            }
        });
    }
    (Sparse { rows }, load)
}

/// Conjugate gradients with a Jacobi preconditioner, node 0 pinned to zero: the warping
/// function is only defined up to a constant.
fn solve(matrix: &Sparse, load: &[f64], iterations: usize) -> Result<Vec<f64>, ShearCentreError> {
    let size = load.len();
    let pinned = |mut v: Vec<f64>| {
        v[0] = 0.0;
        v
    };
    let diagonal = matrix.diagonal();
    let precondition =
        |r: &[f64]| -> Vec<f64> { pinned(r.iter().zip(&diagonal).map(|(r, d)| r / d).collect()) };
    let dot = |a: &[f64], b: &[f64]| -> f64 { a.iter().zip(b).map(|(a, b)| a * b).sum() };
    let mut solution = vec![0.0; size];
    let mut residual = pinned(load.to_vec());
    let target = dot(&residual, &residual) * RELATIVE_RESIDUAL * RELATIVE_RESIDUAL;
    let mut preconditioned = precondition(&residual);
    let mut direction = preconditioned.clone();
    let mut rz = dot(&residual, &preconditioned);
    for _ in 0..iterations {
        if dot(&residual, &residual) <= target {
            return Ok(solution);
        }
        let image = pinned(matrix.product(&direction));
        let alpha = rz / dot(&direction, &image);
        for i in 0..size {
            solution[i] = alpha.mul_add(direction[i], solution[i]);
            residual[i] = (-alpha).mul_add(image[i], residual[i]);
        }
        preconditioned = precondition(&residual);
        let next = dot(&residual, &preconditioned);
        let beta = next / rz;
        rz = next;
        for i in 0..size {
            direction[i] = beta.mul_add(direction[i], preconditioned[i]);
        }
    }
    Err(ShearCentreError::NotConverged)
}
