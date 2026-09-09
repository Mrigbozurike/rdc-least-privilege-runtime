//! Maps agent-facing image pixels to logical desktop coordinates.
//!
//! The agent only ever sees downscaled screenshots. Every screenshot records the desktop
//! rectangle it covers; a [`ViewMap`] turns "pixel (x, y) in that image" back into desktop
//! points, so the agent never has to know about display scaling or multi-monitor offsets.

use crate::proto::{Rect, Screenshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewMap {
    pub rect: Rect,
    pub width: u32,
    pub height: u32,
}

impl ViewMap {
    pub fn from_screenshot(s: &Screenshot) -> Self {
        Self { rect: s.rect, width: s.width, height: s.height }
    }

    /// Image pixel → desktop point. The result always lies inside the covered desktop
    /// rectangle (half-open on the far edge), so an edge pixel never maps onto a neighbouring
    /// display.
    pub fn to_desktop(self, px: f64, py: f64) -> (i32, i32) {
        let px = if px.is_finite() { px.clamp(0.0, self.width as f64) } else { 0.0 };
        let py = if py.is_finite() { py.clamp(0.0, self.height as f64) } else { 0.0 };
        let sx = self.rect.w as f64 / self.width.max(1) as f64;
        let sy = self.rect.h as f64 / self.height.max(1) as f64;
        let x = (self.rect.x as f64 + px * sx).round() as i64;
        let y = (self.rect.y as f64 + py * sy).round() as i64;
        let x_max = self.rect.x as i64 + (self.rect.w as i64 - 1).max(0);
        let y_max = self.rect.y as i64 + (self.rect.h as i64 - 1).max(0);
        (x.clamp(self.rect.x as i64, x_max) as i32, y.clamp(self.rect.y as i64, y_max) as i32)
    }

    /// Desktop point → image pixel (for describing windows in image space).
    pub fn to_view(self, x: i32, y: i32) -> (i32, i32) {
        let sx = self.width as f64 / self.rect.w.max(1) as f64;
        let sy = self.height as f64 / self.rect.h.max(1) as f64;
        (((x - self.rect.x) as f64 * sx).round() as i32, ((y - self.rect.y) as f64 * sy).round() as i32)
    }

    pub fn describe(self) -> String {
        format!(
            "{}x{} image of desktop region x={} y={} w={} h={}",
            self.width, self.height, self.rect.x, self.rect.y, self.rect.w, self.rect.h
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_two_single_monitor() {
        let v = ViewMap { rect: Rect { x: 0, y: 0, w: 1440, h: 960 }, width: 1568, height: 1045 };
        assert_eq!(v.to_desktop(0.0, 0.0), (0, 0));
        assert_eq!(v.to_desktop(784.0, 522.5), (720, 480));
        assert_eq!(v.to_desktop(1568.0, 1045.0), (1439, 959));
        let (vx, vy) = v.to_view(720, 480);
        assert!((vx - 784).abs() <= 1 && (vy - 522).abs() <= 1);
    }

    #[test]
    fn multi_monitor_offset() {
        // Second monitor to the left of the primary: union starts at negative x.
        let v = ViewMap { rect: Rect { x: -1920, y: -100, w: 3360, h: 1180 }, width: 1568, height: 551 };
        assert_eq!(v.to_desktop(0.0, 0.0), (-1920, -100));
        let (x, y) = v.to_desktop(1568.0, 551.0);
        assert_eq!((x, y), (1439, 1079));
    }

    #[test]
    fn clamps_out_of_range() {
        let v = ViewMap { rect: Rect { x: 0, y: 0, w: 100, h: 100 }, width: 50, height: 50 };
        assert_eq!(v.to_desktop(-10.0, 999.0), (0, 99));
        assert_eq!(v.to_desktop(f64::NAN, f64::INFINITY), (0, 0));
    }

    #[test]
    fn edge_pixel_stays_on_its_display() {
        // 200x200 image of a 100x100 display: the last pixel must not map to x=100.
        let v = ViewMap { rect: Rect { x: 0, y: 0, w: 100, h: 100 }, width: 200, height: 200 };
        assert_eq!(v.to_desktop(199.0, 199.0), (99, 99));
    }
}
