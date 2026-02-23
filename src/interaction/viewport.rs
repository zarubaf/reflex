/// Viewport state: zoom, pan, visible range, coordinate conversion.
#[derive(Debug, Clone)]
pub struct ViewportState {
    /// Horizontal zoom: pixels per cycle.
    pub pixels_per_cycle: f32,
    /// Horizontal scroll position in cycles.
    pub scroll_cycle: f64,
    /// Height of each row in pixels.
    pub row_height: f32,
    /// Vertical scroll position in rows (fractional).
    pub scroll_row: f64,
    /// View dimensions in pixels.
    pub view_width: f32,
    pub view_height: f32,
    /// Total data bounds.
    pub max_cycle: u32,
    pub max_row: usize,
    /// Accumulated zoom debt in log-space. When one axis hits its clamp limit,
    /// further zoom is stored here. Reversing direction consumes debt first.
    zoom_debt: f32,
}

impl ViewportState {
    pub fn new() -> Self {
        Self {
            pixels_per_cycle: 12.0,
            scroll_cycle: 0.0,
            row_height: 20.0,
            scroll_row: 0.0,
            view_width: 800.0,
            view_height: 600.0,
            max_cycle: 0,
            max_row: 0,
            zoom_debt: 0.0,
        }
    }

    /// Zoom both axes preserving focal point and aspect ratio.
    ///
    /// When one axis hits its clamp limit, the requested zoom is stored as
    /// "debt" (in log-space). When the user reverses direction, the debt is
    /// consumed first before the actual zoom level changes. This prevents
    /// distortion at the extremes and makes the zoom feel reversible.
    pub fn zoom_both(&mut self, factor: f32, focal_x: f32, focal_y: f32) {
        let log_factor = factor.ln();

        // If debt exists and the new zoom is in the opposite direction, consume debt first.
        if self.zoom_debt != 0.0 && self.zoom_debt.signum() != log_factor.signum() {
            // Opposite direction: reduce debt.
            let old_debt = self.zoom_debt;
            self.zoom_debt += log_factor;

            // If debt changed sign, we consumed all of it and have leftover to apply.
            if self.zoom_debt.signum() != old_debt.signum() && self.zoom_debt != 0.0 {
                let leftover = self.zoom_debt.exp();
                self.zoom_debt = 0.0;
                self.apply_zoom(leftover, focal_x, focal_y);
            }
            // Otherwise debt was partially consumed, no actual zoom change.
            return;
        }

        // Same direction as debt (or no debt): try to apply the zoom.
        let new_ppc = (self.pixels_per_cycle * factor).clamp(0.01, 500.0);
        let new_rh = (self.row_height * factor).clamp(0.05, 500.0);
        let effective_h = new_ppc / self.pixels_per_cycle;
        let effective_v = new_rh / self.row_height;

        // Use the more restrictive factor for both axes.
        let effective = if factor > 1.0 {
            effective_h.min(effective_v)
        } else {
            effective_h.max(effective_v)
        };

        // If the effective factor is ~1.0 (both axes clamped), accumulate debt
        // but cap it so the user doesn't have to undo a huge dead zone.
        if (effective - 1.0).abs() < 1e-6 {
            self.zoom_debt = (self.zoom_debt + log_factor).clamp(-0.5, 0.5);
            return;
        }

        self.apply_zoom(effective, focal_x, focal_y);
    }

    fn apply_zoom(&mut self, factor: f32, focal_x: f32, focal_y: f32) {
        let cycle_at_focal = self.pixel_to_cycle(focal_x);
        self.pixels_per_cycle = (self.pixels_per_cycle * factor).clamp(0.01, 500.0);
        self.scroll_cycle =
            cycle_at_focal - (focal_x as f64 / self.pixels_per_cycle as f64);

        let row_at_focal = self.pixel_to_row(focal_y);
        self.row_height = (self.row_height * factor).clamp(0.05, 500.0);
        self.scroll_row = row_at_focal as f64 - (focal_y as f64 / self.row_height as f64);
    }

    /// Zoom horizontal only preserving focal point.
    #[allow(dead_code)]
    pub fn zoom(&mut self, factor: f32, focal_x: f32) {
        let cycle_at_focal = self.pixel_to_cycle(focal_x);
        self.pixels_per_cycle = (self.pixels_per_cycle * factor).clamp(0.01, 500.0);
        self.scroll_cycle =
            cycle_at_focal - (focal_x as f64 / self.pixels_per_cycle as f64);
    }

    /// Zoom vertical only preserving focal point.
    #[allow(dead_code)]
    pub fn zoom_vertical(&mut self, factor: f32, focal_y: f32) {
        let row_at_focal = self.pixel_to_row(focal_y);
        self.row_height = (self.row_height * factor).clamp(0.05, 500.0);
        self.scroll_row = row_at_focal as f64 - (focal_y as f64 / self.row_height as f64);
    }

    /// Pan by pixel delta.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.scroll_cycle -= dx as f64 / self.pixels_per_cycle as f64;
        self.scroll_row -= dy as f64 / self.row_height as f64;
        self.clamp();
    }

    /// Clamp scroll positions — only used for panning so user can't scroll
    /// past content edges. Zoom never clamps to preserve focal-point math.
    pub fn clamp(&mut self) {
        let content_cycles = self.max_cycle as f64;
        let view_cycles = self.view_width as f64 / self.pixels_per_cycle as f64;
        if content_cycles <= view_cycles {
            self.scroll_cycle = 0.0;
        } else {
            self.scroll_cycle = self.scroll_cycle.clamp(0.0, content_cycles - view_cycles);
        }

        let content_rows = self.max_row as f64;
        let view_rows = self.view_height as f64 / self.row_height as f64;
        if content_rows <= view_rows {
            self.scroll_row = 0.0;
        } else {
            self.scroll_row = self.scroll_row.clamp(0.0, content_rows - view_rows);
        }
    }

    pub fn pixel_to_cycle(&self, px: f32) -> f64 {
        self.scroll_cycle + px as f64 / self.pixels_per_cycle as f64
    }

    pub fn pixel_to_row(&self, py: f32) -> f64 {
        self.scroll_row + py as f64 / self.row_height as f64
    }

    pub fn cycle_to_pixel(&self, cycle: f64) -> f32 {
        ((cycle - self.scroll_cycle) * self.pixels_per_cycle as f64) as f32
    }

    pub fn row_to_pixel(&self, row: f64) -> f32 {
        ((row - self.scroll_row) * self.row_height as f64) as f32
    }

    pub fn visible_row_range(&self) -> (usize, usize) {
        let start = self.scroll_row.floor().max(0.0) as usize;
        let end = ((self.scroll_row + self.view_height as f64 / self.row_height as f64).ceil()
            as usize)
            .min(self.max_row);
        (start, end)
    }

    pub fn visible_cycle_range(&self) -> (u32, u32) {
        let start = self.scroll_cycle.floor().max(0.0) as u32;
        let end = ((self.scroll_cycle + self.view_width as f64 / self.pixels_per_cycle as f64)
            .ceil() as u32)
            .min(self.max_cycle);
        (start, end)
    }
}

impl Default for ViewportState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focal_point_zoom() {
        let mut vp = ViewportState {
            pixels_per_cycle: 10.0,
            view_width: 1000.0,
            view_height: 500.0,
            max_cycle: 1000,
            max_row: 100,
            ..Default::default()
        };
        let focal_cycle_before = vp.pixel_to_cycle(500.0);
        vp.zoom_both(2.0, 500.0, 250.0);
        let focal_cycle_after = vp.pixel_to_cycle(500.0);
        assert!(
            (focal_cycle_before - focal_cycle_after).abs() < 0.1,
            "Focal point cycle should be preserved: {} vs {}",
            focal_cycle_before,
            focal_cycle_after
        );
        assert!((vp.pixels_per_cycle - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_clamping() {
        let mut vp = ViewportState {
            pixels_per_cycle: 10.0,
            view_width: 100.0,
            view_height: 100.0,
            max_cycle: 50,
            max_row: 10,
            row_height: 20.0,
            ..Default::default()
        };
        vp.scroll_cycle = -100.0;
        vp.scroll_row = -100.0;
        vp.clamp();
        assert_eq!(vp.scroll_cycle, 0.0);
        assert_eq!(vp.scroll_row, 0.0);

        vp.scroll_cycle = 10000.0;
        vp.scroll_row = 10000.0;
        vp.clamp();
        assert!((vp.scroll_cycle - 40.0).abs() < 0.01);
        assert!((vp.scroll_row - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let vp = ViewportState {
            pixels_per_cycle: 15.0,
            scroll_cycle: 10.0,
            row_height: 25.0,
            scroll_row: 5.0,
            view_width: 800.0,
            view_height: 600.0,
            max_cycle: 1000,
            max_row: 100,
            ..Default::default()
        };
        let px = 123.0;
        let cycle = vp.pixel_to_cycle(px);
        let px2 = vp.cycle_to_pixel(cycle);
        assert!((px - px2).abs() < 0.01);

        let py = 234.0;
        let row = vp.pixel_to_row(py);
        let py2 = vp.row_to_pixel(row);
        assert!((py - py2).abs() < 0.01);
    }

    #[test]
    fn test_visible_ranges() {
        let vp = ViewportState {
            pixels_per_cycle: 10.0,
            scroll_cycle: 5.0,
            row_height: 20.0,
            scroll_row: 2.0,
            view_width: 200.0,
            view_height: 100.0,
            max_cycle: 100,
            max_row: 50,
            ..Default::default()
        };
        let (rs, re) = vp.visible_row_range();
        assert_eq!(rs, 2);
        assert_eq!(re, 7);

        let (cs, ce) = vp.visible_cycle_range();
        assert_eq!(cs, 5);
        assert_eq!(ce, 25);
    }
}
