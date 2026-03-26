use gpui::*;

/// Paint a bar chart from (min, max) data, one bar per pixel.
/// Used by counter sparklines, minimap trendline, and heatmap.
pub fn paint_bars(
    data: &[(u64, u64)],
    bounds: &Bounds<Pixels>,
    width: f32,
    height: f32,
    color: Hsla,
    window: &mut Window,
) {
    if data.is_empty() {
        return;
    }
    let global_max = data.iter().map(|(_, mx)| *mx).max().unwrap_or(1).max(1);

    for pixel in 0..(width as usize) {
        let bucket = (pixel as f32 / width * data.len() as f32) as usize;
        let bucket = bucket.min(data.len().saturating_sub(1));
        let (_, max_d) = data[bucket];
        let bar_top = max_d as f32 / global_max as f32;
        if bar_top <= 0.0 {
            continue;
        }
        let bar_h = (bar_top * height).max(1.0);
        let y_top = height - bar_h;

        window.paint_quad(fill(
            Bounds::new(
                point(
                    bounds.origin.x + px(pixel as f32),
                    bounds.origin.y + px(y_top),
                ),
                size(px(1.0), px(bar_h)),
            ),
            color,
        ));
    }
}
