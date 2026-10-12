//! Pure geometry helpers for painting the organism (no GPUI types).

/// Splits a polyline into dash segments. `phase` is the offset along the pattern (a negative
/// phase moves the pattern forward, like the canvas `lineDashOffset`). Each returned piece is an
/// open polyline.
#[must_use]
pub fn dash_polyline(
    pts: &[[f32; 2]],
    closed: bool,
    on: f32,
    off: f32,
    phase: f32,
) -> Vec<Vec<[f32; 2]>> {
    let mut out = Vec::new();
    if pts.len() < 2 || on <= 0.0 || !on.is_finite() || !off.is_finite() || off < 0.0 {
        return out;
    }
    let period = on + off;
    // Position inside the pattern at the start of the path.
    let pos = phase.rem_euclid(period);
    let mut drawing = pos < on;
    let mut left = if drawing { on - pos } else { period - pos };
    let mut cur: Vec<[f32; 2]> = Vec::new();
    if drawing {
        cur.push(pts[0]);
    }
    let n = if closed { pts.len() } else { pts.len() - 1 };
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        if len <= f32::EPSILON {
            continue;
        }
        let mut travelled = 0.0;
        while len - travelled > left {
            travelled += left;
            let p = [a[0] + dx * travelled / len, a[1] + dy * travelled / len];
            if drawing {
                cur.push(p);
                out.push(std::mem::take(&mut cur));
                left = off;
                drawing = false;
                if off == 0.0 {
                    drawing = true;
                    cur.push(p);
                    left = on;
                }
            } else {
                cur.push(p);
                left = on;
                drawing = true;
            }
        }
        left -= len - travelled;
        if drawing {
            cur.push(b);
        }
    }
    if drawing && cur.len() >= 2 {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_a_straight_line() {
        let d = dash_polyline(&[[0.0, 0.0], [10.0, 0.0]], false, 2.0, 2.0, 0.0);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0], vec![[0.0, 0.0], [2.0, 0.0]]);
        assert_eq!(d[1][0], [4.0, 0.0]);
        assert_eq!(d[2][0], [8.0, 0.0]);
        assert_eq!(d[2].last().copied(), Some([10.0, 0.0]));
    }

    #[test]
    fn phase_shifts_the_pattern() {
        let d = dash_polyline(&[[0.0, 0.0], [10.0, 0.0]], false, 2.0, 2.0, 2.0);
        // Starts in the gap: first dash begins at x = 2.
        assert_eq!(d[0][0], [2.0, 0.0]);
        let d = dash_polyline(&[[0.0, 0.0], [10.0, 0.0]], false, 2.0, 2.0, -1.0);
        // Negative phase advances the pattern.
        assert_eq!(d[0][0], [1.0, 0.0]);
    }

    #[test]
    fn dashes_follow_corners_and_closed_paths() {
        let sq = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let d = dash_polyline(&sq, true, 3.0, 1.0, 0.0);
        let total: f32 = d
            .iter()
            .map(|s| {
                s.windows(2)
                    .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                    .sum::<f32>()
            })
            .sum();
        assert!((total - 12.0).abs() < 1e-3, "{total}");
    }

    #[test]
    fn degenerate_inputs_are_empty() {
        assert!(dash_polyline(&[[0.0, 0.0]], false, 2.0, 2.0, 0.0).is_empty());
        assert!(dash_polyline(&[[0.0, 0.0], [1.0, 0.0]], false, 0.0, 2.0, 0.0).is_empty());
    }
}
