//! The tray icon, drawn in code: a round mark whose colour tells the agent's state.

use crate::app::Look;

/// Side of the icon, in pixels.
pub(crate) const SIZE: u32 = 32;

/// RGBA pixels (row by row, `SIZE × SIZE`) of the icon for a state: grey — stopped, violet —
/// waiting, violet ring with a light centre — working, amber — needs you, red — error.
pub(crate) fn rgba(look: Look) -> Vec<u8> {
    let (r, g, b) = match look {
        Look::Stopped => (140, 140, 140),
        Look::Waiting | Look::Working => (124, 58, 237),
        Look::NeedsYou => (245, 158, 11),
        Look::Error => (220, 38, 38),
    };
    let half = SIZE as f32 / 2.0 - 0.5;
    let mut out = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 - half, y as f32 - half);
            let d = (dx * dx + dy * dy).sqrt();
            let alpha = (half - d).clamp(0.0, 1.0);
            // Working: a light centre inside the ring.
            let inner = if look == Look::Working {
                (9.0 - d).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mix = |c: u8| (f32::from(c) * (1.0 - inner) + 255.0 * inner) as u8;
            out.extend_from_slice(&[mix(r), mix(g), mix(b), (alpha * 255.0) as u8]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_its_own_icon() {
        let looks = [
            Look::Stopped,
            Look::Waiting,
            Look::Working,
            Look::NeedsYou,
            Look::Error,
        ];
        let icons: Vec<Vec<u8>> = looks.iter().map(|l| rgba(*l)).collect();
        for (i, a) in icons.iter().enumerate() {
            assert_eq!(a.len(), (SIZE * SIZE * 4) as usize);
            for b in icons.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
        // Corners are transparent, the centre is opaque.
        let waiting = rgba(Look::Waiting);
        assert_eq!(waiting.get(3), Some(&0));
        let centre = ((SIZE / 2 * SIZE + SIZE / 2) * 4 + 3) as usize;
        assert_eq!(waiting.get(centre), Some(&255));
    }
}
