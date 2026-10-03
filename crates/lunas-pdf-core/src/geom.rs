//! Transformations affines (espace utilisateur PDF ↔ points d'affichage).

use crate::types::Rect;

/// x' = a·x + c·y + e ; y' = b·x + d·y + f
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    pub fn invert(&self) -> Affine {
        let det = self.a * self.d - self.b * self.c;
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        }
    }

    /// Boîte englobante d'un rectangle (x0, y0, x1, y1) transformé.
    pub fn bbox(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> (f32, f32, f32, f32) {
        let pts = [self.apply(x0, y0), self.apply(x1, y0), self.apply(x0, y1), self.apply(x1, y1)];
        let (mut lx, mut ly, mut hx, mut hy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in pts {
            lx = lx.min(x);
            ly = ly.min(y);
            hx = hx.max(x);
            hy = hy.max(y);
        }
        (lx, ly, hx, hy)
    }

    /// Rectangle d'affichage (haut-gauche, largeur, hauteur) d'un rectangle transformé.
    pub fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        let (lx, ly, hx, hy) = self.bbox(x0, y0, x1, y1);
        Rect {
            x: lx,
            y: ly,
            w: hx - lx,
            h: hy - ly,
        }
    }

    /// Partie linéaire arrondie au quart de tour (rotation de page multiple de 90°).
    pub fn snapped_linear(&self) -> [f32; 4] {
        [self.a, self.b, self.c, self.d].map(|v| if v.abs() < 0.5 { 0.0 } else { v.signum() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invert_round_trip() {
        // Page A4 tournée de 90° : x' = y, y' = x (avec décalage).
        let t = Affine {
            a: 0.0,
            b: 1.0,
            c: 1.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        };
        let i = t.invert();
        let (x, y) = t.apply(100.0, 30.0);
        let (bx, by) = i.apply(x, y);
        assert!((bx - 100.0).abs() < 1e-4 && (by - 30.0).abs() < 1e-4);
        let flip = Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            e: 0.0,
            f: 842.0,
        };
        assert_eq!(
            flip.rect(10.0, 800.0, 20.0, 810.0),
            Rect {
                x: 10.0,
                y: 32.0,
                w: 10.0,
                h: 10.0
            }
        );
    }
}
