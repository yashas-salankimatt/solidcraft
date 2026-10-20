//! The checks a fillet or chamfer a geometry provider made must pass (`provider`), judged from
//! the body it was given and the edges it was asked to blend, without SolidCraft's own blends:
//!
//! - it stays within the body's box (a round or chamfer takes material off a convex edge and fills
//!   a concave one, both inside it);
//! - its volume changes by no more than blends of these edges can;
//! - its surface away from the blended edges (and the edges continuing them smoothly, which a
//!   blend follows) is the body's surface, both ways: nothing moved, dropped or added there;
//! - its new surface lies on the side of the body a blend takes it to: inside the body beside a
//!   convex edge (a blend there only takes material off), outside beside a concave one (it only
//!   fills). A blend built the wrong way round, adding a wedge to a convex edge, fails here;
//! - near each edge asked for there is new surface: the edge was blended, not left as it was.

use solidcraft_geom::{Mesh, Vec3};

use crate::body::Body;
use crate::boolean_guard::Near;
use crate::provider::{check_tolerance, samples};

/// How far from a blended edge, in blend sizes, the surface may change.
const REACH: f64 = 3.0;

/// Most volume a blend of size `s` changes per millimetre of edge, in `s²`: a round of a 20°
/// edge takes about 2.4 s², of a right angle 0.21 s².
const VOLUME_PER_LENGTH: f64 = 4.0;

/// Whether `r` can be a fillet or chamfer of size `size` (radius or distance) of `input`'s edges
/// through `edges`; else why not.
pub(crate) fn blend_check(input: &Body, edges: &[[f64; 3]], size: f64, r: &Body) -> Result<(), String> {
    if !(size.is_finite() && size > 0.0) || edges.is_empty() {
        return Err("the blend had no size or no edges".into());
    }
    let t = check_tolerance(input, input);
    let mesh = |b: &Body| b.tessellate_with(t, true).map_err(|e| format!("it could not be checked: {e}"));
    let (mi, mr) = (mesh(input)?, mesh(r)?);
    let picked = picked_edges(&mi, edges, input.size())?;
    let chains = chains(&mi, &picked);

    let tol = 2.0 * t;
    let (bi, br) = (mi.bounds(), mr.bounds());
    let outside = br.min.x < bi.min.x - tol
        || br.min.y < bi.min.y - tol
        || br.min.z < bi.min.z - tol
        || br.max.x > bi.max.x + tol
        || br.max.y > bi.max.y + tol
        || br.max.z > bi.max.z + tol;
    if outside {
        return Err("it reaches outside the body it blends".into());
    }

    let (vi, vr) = (mi.measure().volume, mr.measure().volume);
    let length: f64 = chains.iter().map(|&e| mi.edges.get(e).map_or(0.0, |c| polyline_length(c))).sum();
    let most = VOLUME_PER_LENGTH * size * size * length + 2e-4 * vi.abs() + 1e-9;
    if !vr.is_finite() || (vr - vi).abs() > most {
        return Err(format!("its volume changes by {:.4} mm³, more than a blend of these edges can ({most:.4} mm³ at most)", vr - vi));
    }

    let reach = 4.0 * t;
    let unchecked = || "it could not be checked: a surface has no triangles".to_string();
    let (ni, nr) = (Near::new(&mi, reach).ok_or_else(unchecked)?, Near::new(&mr, reach).ok_or_else(unchecked)?);
    let zone = REACH * size + 4.0 * t;
    let near_blend = |p: Vec3| chains.iter().any(|&e| mi.edges.get(e).is_some_and(|c| polyline_distance(c, p) < zone));
    // A few stray samples are the meshes' chords where they curve; a wrong result misses faces.
    let too_many = |bad: usize, of: usize| bad > of / 200 + 2;
    for (name, from, onto) in [("the body's", &mi, &nr), ("its", &mr, &ni)] {
        let (mut of, mut missing) = (0, 0);
        for p in samples(from).into_iter().filter(|p| !near_blend(*p)) {
            of += 1;
            if !onto.close(p) {
                missing += 1;
            }
        }
        if too_many(missing, of) {
            let whose = if name == "its" { "the body's" } else { "its" };
            return Err(format!("{missing} of {of} points sampled on {name} surface away from the blended edges are not on {whose}"));
        }
    }

    // New surface lies inside the body beside a convex edge and outside beside a concave one.
    let inside = mi.inside_index();
    let probe = (size * 0.25).min(input.size() * 0.01).max(8.0 * t);
    let convex: Vec<Option<bool>> = chains.iter().map(|&e| mi.edges.get(e).and_then(|c| convexity(&inside, c, probe))).collect();
    let (mut of, mut wrong) = (0, 0);
    for p in samples(&mr).into_iter().filter(|p| !ni.close(*p)) {
        let nearest = chains
            .iter()
            .zip(&convex)
            .filter_map(|(&e, c)| Some((mi.edges.get(e).map(|poly| polyline_distance(poly, p))?, *c)))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((d, Some(is_convex))) = nearest else { continue };
        if d >= zone {
            continue;
        }
        of += 1;
        if inside.contains(p) != is_convex {
            wrong += 1;
        }
    }
    if too_many(wrong, of) {
        return Err(format!(
            "{wrong} of {of} points sampled on its new surface lie on the wrong side of the body: the blend was built the wrong way round"
        ));
    }

    // Each edge asked for has new surface beside it.
    let beside = 1.5 * size + 4.0 * t;
    for (n, &e) in picked.iter().enumerate() {
        let Some(edge) = mi.edges.get(e) else { continue };
        let new = mr
            .triangles
            .iter()
            .filter_map(|tri| mr.tri(tri))
            .map(|[a, b, c]| (a + b + c) * (1.0 / 3.0))
            .any(|p| polyline_distance(edge, p) < beside && !ni.close(p));
        if !new {
            return Err(format!("edge {} of the {} asked for was not blended", n + 1, picked.len()));
        }
    }
    Ok(())
}

/// The input mesh's edge nearest each point (within a twentieth of the body's size).
fn picked_edges(mesh: &Mesh, edges: &[[f64; 3]], size: f64) -> Result<Vec<usize>, String> {
    let within = size * 0.05 + 1e-3;
    let mut picked = Vec::new();
    for (n, p) in edges.iter().enumerate() {
        let p = Vec3::new(p[0], p[1], p[2]);
        let nearest = mesh
            .edges
            .iter()
            .enumerate()
            .map(|(i, c)| (i, polyline_distance(c, p)))
            .filter(|(_, d)| d.is_finite())
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match nearest {
            Some((i, d)) if d <= within => {
                if !picked.contains(&i) {
                    picked.push(i);
                }
            }
            _ => return Err(format!("edge {} of the {} asked for is not on the body", n + 1, edges.len())),
        }
    }
    Ok(picked)
}

/// The picked edges with the edges continuing them smoothly (within 2°), which a blend follows.
fn chains(mesh: &Mesh, picked: &[usize]) -> Vec<usize> {
    let mut all: Vec<usize> = Vec::new();
    for &e in picked {
        for c in mesh.tangent_chain(e, 2f64.to_radians()) {
            if !all.contains(&c) {
                all.push(c);
            }
        }
        if !all.contains(&e) {
            all.push(e);
        }
    }
    all
}

/// Whether the body is convex at the edge `c` (`Some(true)`: a quarter of a small circle around its
/// middle lies in the body, as at a box's edge; `Some(false)`: three quarters, as in an inside
/// corner), probed `r` away; `None` when it is neither (faces meeting smoothly, a seam).
fn convexity(inside: &solidcraft_geom::InsideIndex<'_>, c: &[Vec3], r: f64) -> Option<bool> {
    let total = polyline_length(c);
    if c.len() < 2 || !(total > 0.0) {
        return None;
    }
    // The point halfway along, and the direction there.
    let mut left = total * 0.5;
    let mut at = None;
    for w in c.windows(2) {
        let l = (w[1] - w[0]).len();
        if l > 0.0 && left <= l {
            at = Some((w[0] + (w[1] - w[0]) * (left / l), (w[1] - w[0]) * (1.0 / l)));
            break;
        }
        left -= l;
    }
    let (m, dir) = at?;
    let helper = if dir.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = dir.cross(helper).normalized()?;
    let v = dir.cross(u);
    const STEPS: usize = 32;
    let n = (0..STEPS)
        .filter(|k| {
            let a = std::f64::consts::TAU * (*k as f64 + 0.5) / STEPS as f64;
            inside.contains(m + (u * a.cos() + v * a.sin()) * r)
        })
        .count();
    let share = n as f64 / STEPS as f64;
    if share < 0.4 {
        Some(true)
    } else if share > 0.6 {
        Some(false)
    } else {
        None
    }
}

fn polyline_length(c: &[Vec3]) -> f64 {
    c.windows(2).map(|w| (w[1] - w[0]).len()).sum()
}

/// The distance from `p` to the polyline `c`.
fn polyline_distance(c: &[Vec3], p: Vec3) -> f64 {
    match c {
        [] => f64::INFINITY,
        [only] => (p - *only).len(),
        _ => c
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let ab = b - a;
                let l2 = ab.dot(ab);
                let s = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
                (p - (a + ab * s)).len()
            })
            .fold(f64::INFINITY, f64::min),
    }
}
