//! 皮膚調色盤：把按深色主題畫好的島體內容，整體對映到淺色皮膚的配色。
//!
//! 島上的頁面、圖示與控制元件都按「黑底白字」設計，顏色散落在幾十處繪製程式碼裡。
//! 這裡不逐處改寫，而是在一幀畫完之後對島體內容的圖形統一換色：
//! - 中性色按亮度對調：白字 → 墨色，黑底卡片 → 紙色，灰階按比例落在兩者之間；
//! - 強調色（藍）換成皮膚的主題色，其餘彩色略加深，保證在淺底上可讀；
//! - 帶紋理的圖形（專輯封面、應用圖示）原樣保留。

use eframe::egui::{self, Color32};
use eframe::epaint::{shape_transform::adjust_colors, ColorMode, Shape};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// 原來的「黑」對映到的底色（sRGB 0..255）。
    pub paper: [u8; 3],
    /// 原來的「白」對映到的文字色。
    pub ink: [u8; 3],
    /// 藍色強調色替換成的主題色；None 表示保留色相，只加深。
    pub accent: Option<[u8; 3]>,
    /// 深色塊（原卡片、按鈕底）的不透明度係數：毛玻璃讓卡片也帶一點通透。
    pub dark_alpha: f32,
    /// 文字類中性色的亮度曲線（1 = 線性；小於 1 讓灰字更亮，用於深色玻璃）。
    pub lift: f32,
    /// 非強調彩色的加深係數（淺底上需要加深；深色玻璃保持 1）。
    pub darken: f32,
    /// 深色塊（卡片、按鈕底）對映時的亮度係數：越小越接近紙色，毛玻璃淺色用它做白色磨砂按鈕。
    pub chip: f32,
}

/// 奶油（參考 Oyen）：暖白底、深棕字、橙色強調。
pub const CREAM: Palette = Palette { paper: [255, 243, 228], ink: [46, 32, 22], accent: Some([240, 138, 36]), dark_alpha: 1.0, lift: 1.0, darken: 0.78, chip: 1.0 };
/// 毛玻璃·淺：白色磨砂、近黑字、系統藍。
pub const GLASS_LIGHT: Palette = Palette { paper: [255, 255, 255], ink: [22, 22, 26], accent: Some([10, 112, 230]), dark_alpha: 0.8, lift: 1.0, darken: 0.78, chip: 0.6 };
/// 毛玻璃·深：保持黑底白字，只把灰色文字提亮、卡片底調透，適配更通透的深色玻璃。
pub const GLASS_DARK: Palette = Palette { paper: [0, 0, 0], ink: [255, 255, 255], accent: None, dark_alpha: 0.5, lift: 0.6, darken: 1.0, chip: 1.0 };
// /// 液態玻璃：極輕冷調底色配高對比亮字與字體黑邊、超通透內卡片，突顯背景穿透與曲面 Fresnel 彎月面。
// pub const LIQUID_GLASS: Palette = Palette { paper: [10, 14, 22], ink: [255, 255, 255], accent: None, dark_alpha: 0.20, lift: 0.42, darken: 1.0, chip: 1.0 };

/// 單色圖示紋理（白色圖形 + 透明度，如 AI 廠商標識）：和文字一樣參與換色。
/// 其餘紋理（專輯封面、應用圖示、檔案預覽）保持原樣。
fn monochrome() -> &'static std::sync::Mutex<std::collections::HashSet<egui::TextureId>> {
    static SET: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<egui::TextureId>>> = std::sync::OnceLock::new();
    SET.get_or_init(Default::default)
}

/// 登記一張單色圖示紋理。
pub fn mark_monochrome(id: egui::TextureId) {
    monochrome().lock().unwrap_or_else(|e| e.into_inner()).insert(id);
}

/// 彩色標識不參與皮膚換色，同時清除紋理編號複用時的舊登記。
pub fn mark_colored(id:egui::TextureId){
    monochrome().lock().unwrap_or_else(|e|e.into_inner()).remove(&id);
}

fn is_monochrome(id: egui::TextureId) -> bool {
    monochrome().lock().unwrap_or_else(|e| e.into_inner()).contains(&id)
}

/// 島仔自己的主題藍（按鈕、進度、飲水等）。只有這幾種藍會換成皮膚的主題色；
/// 使用者為 AI 帳號等選的品牌色一律保留色相。
fn theme_blue(c: [f32; 3]) -> Option<bool> {
    let near = |t: eframe::egui::Color32| {
        let d = |a: f32, b: u8| (a * 255.0 - b as f32).abs();
        d(c[0], t.r()) <= 10.0 && d(c[1], t.g()) <= 10.0 && d(c[2], t.b()) <= 10.0
    };
    use eframe::egui::Color32 as C;
    // 介面裡實際用到的藍：按鈕、選中態、連結、進度、飲水；不含 AI 帳號可選的品牌藍。
    const BRIGHT: [C; 8] = [crate::theme::BLUE, crate::theme::CYAN, C::from_rgb(82, 162, 255), C::from_rgb(10, 132, 255), C::from_rgb(70, 150, 255), C::from_rgb(64, 156, 255), C::from_rgb(86, 204, 242), C::from_rgb(77, 192, 242)];
    const DEEP: [C; 4] = [crate::theme::BLUE_DEEP, crate::theme::WATER_TRACK, C::from_rgb(45, 105, 165), C::from_rgb(28, 65, 100)];
    if BRIGHT.iter().any(|t| near(*t)) {
        Some(true)
    } else if DEEP.iter().any(|t| near(*t)) {
        Some(false)
    } else {
        None
    }
}

fn light_paper(p: &Palette) -> bool {
    luma([p.paper[0] as f32 / 255.0, p.paper[1] as f32 / 255.0, p.paper[2] as f32 / 255.0]) > 0.5
}

fn contrast(a: [f32; 3], b: [f32; 3]) -> f32 {
    crate::glass::contrast(a, b)
}

fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// 對映單個顏色（Color32 為預乘 alpha）。
pub fn map_color(color: Color32, p: &Palette) -> Color32 {
    let a = color.a();
    if a == 0 {
        return color;
    }
    // egui 的淡入淡出（gamma_multiply）直接在 sRGB 數值上乘透明度，
    // 所以這裡也在 sRGB 空間裡還原/預乘，半透明的白字才會被識別為「白」。
    let af = a as f32 / 255.0;
    let c = [color.r() as f32 / 255.0 / af, color.g() as f32 / 255.0 / af, color.b() as f32 / 255.0 / af].map(|v| v.min(1.0));
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let f = |v: [u8; 3]| [v[0] as f32 / 255.0, v[1] as f32 / 255.0, v[2] as f32 / 255.0];
    let (out, alpha) = if max - min < 0.16 || max < 0.14 {
        // 中性色：亮度對調
        let t = luma(c).clamp(0.0, 1.0);
        let t = if t > 0.3 { t.powf(p.lift) } else { t * p.chip };
        let (paper, ink) = (f(p.paper), f(p.ink));
        let mixed = [paper[0] + (ink[0] - paper[0]) * t, paper[1] + (ink[1] - paper[1]) * t, paper[2] + (ink[2] - paper[2]) * t];
        let alpha = if t < 0.3 { a as f32 * p.dark_alpha } else { a as f32 };
        (mixed, alpha)
    } else {
        match (p.accent, theme_blue(c)) {
            (Some(accent), Some(bright)) => {
                // 主題藍換成皮膚主題色；深藍底色對映成淺色的主題色背景
                let acc = f(accent);
                if !bright {
                    let paper = f(p.paper);
                    ([paper[0] + (acc[0] - paper[0]) * 0.28, paper[1] + (acc[1] - paper[1]) * 0.28, paper[2] + (acc[2] - paper[2]) * 0.28], a as f32)
                } else {
                    (acc, a as f32)
                }
            }
            _ if light_paper(p) && max < 0.5 => {
                // 深色的彩色底塊（選中態按鈕、強調卡片）：在淺底上改為「紙色 + 淡淡的色相」，
                // 上面換成墨色的文字才看得清。
                let hue = [c[0] / max, c[1] / max, c[2] / max];
                let paper = f(p.paper);
                let k = 0.24;
                ([paper[0] + (hue[0] - paper[0]) * k, paper[1] + (hue[1] - paper[1]) * k, paper[2] + (hue[2] - paper[2]) * k], a as f32)
            }
            _ => {
                // 其它彩色（文字、圖示、進度）：淺底上逐步加深，直到對比度至少 3:1。
                let mut out = c;
                let mut alpha = a as f32;
                if light_paper(p) {
                    // 按「疊在紙色上的實際效果」計算對比度，半透明的彩色字也要可讀。
                    let paper = f(p.paper);
                    let seen = |c: [f32; 3], alpha: f32| {
                        let t = alpha / 255.0;
                        [paper[0] + (c[0] - paper[0]) * t, paper[1] + (c[1] - paper[1]) * t, paper[2] + (c[2] - paper[2]) * t]
                    };
                    let mut k = 1.0;
                    while k > 0.45 && contrast(seen(out, alpha), paper) < 3.0 {
                        k -= 0.05;
                        out = [c[0] * k, c[1] * k, c[2] * k];
                    }
                    // 只有原本就比較實（不是淡淡的裝飾色）的半透明彩色才補足不透明度。
                    if a >= 110 {
                        while alpha < 255.0 && contrast(seen(out, alpha), paper) < 3.0 {
                            alpha = (alpha * 1.15).min(255.0);
                        }
                    }
                } else if max > 0.6 {
                    out = [c[0] * p.darken, c[1] * p.darken, c[2] * p.darken];
                }
                (out, alpha)
            }
        }
    };
    let alpha = alpha.round().clamp(0.0, 255.0);
    let to = |v: f32| (v.clamp(0.0, 1.0) * alpha).round() as u8;
    Color32::from_rgba_premultiplied(to(out[0]), to(out[1]), to(out[2]), alpha as u8)
}

/// 遞迴地給一個圖形換色。
///
/// 直接改寫圖形裡**已經生效**的顏色（包括文字每個字形的頂點色），所以淡入淡出
/// 乘進去的透明度、一段文字裡的多種顏色都會原樣保留。
pub fn recolor(shape: &mut Shape, p: &Palette) {
    recolor_with(shape, p, map_color)
}

/// 同 `recolor`，但用指定的換色函式。
fn recolor_with(shape: &mut Shape, p: &Palette, tone: Tone) {
    let palette = *p;
    let adjust = move |c: &mut Color32| {
        if *c != Color32::PLACEHOLDER {
            *c = tone(*c, &palette);
        }
    };
    match shape {
        Shape::Vec(shapes) => shapes.iter_mut().for_each(|s| recolor_with(s, p, tone)),
        // 帶紋理的網格是圖片（專輯封面、圖示），保持原樣；單色圖示照常換色。
        Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() && !is_monochrome(mesh.texture_id) => {}
        // 帶紋理的矩形同理，只換描邊。
        Shape::Rect(s) if s.brush.as_ref().is_some_and(|b| !is_monochrome(b.fill_texture_id)) => adjust(&mut s.stroke.color),
        Shape::Callback(_) => {}
        _ => adjust_colors(shape, adjust),
    }
}

/// 實心彩色底塊（按鈕、開關、日期圓點）的範圍；落在上面的白字與白色圖示保持白色。
fn colored_fill(shape: &Shape) -> Option<egui::Rect> {
    let (color, rect) = match shape {
        Shape::Rect(r) if r.brush.is_none() => (r.fill, r.rect),
        Shape::Circle(c) => (c.fill, egui::Rect::from_center_size(c.center, egui::Vec2::splat(c.radius * 2.0))),
        Shape::Path(p) if p.closed => (p.fill, shape.visual_bounding_rect()),
        Shape::Mesh(m) if m.texture_id == egui::TextureId::default() && !m.vertices.is_empty() => (m.vertices[0].color, m.calc_bounds()),
        _ => return None,
    };
    let a = color.a();
    if a < 180 { return None; }
    let af = a as f32 / 255.0;
    let c = [color.r() as f32 / 255.0 / af, color.g() as f32 / 255.0 / af, color.b() as f32 / 255.0 / af];
    let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    (max - min > 0.3 && max > 0.45 && rect.is_positive()).then_some(rect)
}

/// 圖形的主要顏色是否為淺色中性色（白字、白色圖示）。
fn light_neutral(shape: &Shape) -> bool {
    let color = match shape {
        Shape::Text(t) => t.galley.rows.iter().flat_map(|r| r.visuals.mesh.vertices.first()).next().map(|v| v.color).unwrap_or(t.fallback_color),
        Shape::Circle(c) => if c.fill.a() > 0 { c.fill } else { c.stroke.color },
        Shape::Path(p) => if p.fill.a() > 0 { p.fill } else if let ColorMode::Solid(c) = p.stroke.color { c } else { return false },
        Shape::Rect(r) => r.fill,
        Shape::LineSegment { stroke, .. } => stroke.color,
        Shape::Mesh(m) if m.texture_id == egui::TextureId::default() => match m.vertices.first() { Some(v) => v.color, None => return false },
        _ => return false,
    };
    let a = color.a();
    if a < 90 { return false; }
    let af = a as f32 / 255.0;
    let c = [color.r() as f32 / 255.0 / af, color.g() as f32 / 255.0 / af, color.b() as f32 / 255.0 / af];
    let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    max - min < 0.12 && luma(c) > 0.8
}

/// 換色函式：輸入按深色主題畫的顏色，輸出該皮膚下的顏色。
pub type Tone = fn(Color32, &Palette) -> Color32;

/// 本幀登記過自帶配色的圖形區段：(圖層, 起, 止, 換色函式)，附帶登記時的幀號，跨幀自動作廢。
#[derive(Clone, Default)]
struct Toned { pass: u64, spans: Vec<(egui::LayerId, usize, usize, Tone)> }

fn toned_id() -> egui::Id { egui::Id::new("skin-paint-toned") }

/// 把 `from` 起到當前為止畫在 `layer` 上的圖形登記為自帶配色：換膚時改用 `tone`，
/// 不走通用的「彩色逐步加深」。用於天氣圖示這類本身就有淺色版配色的插畫式圖形。
pub fn tone_since(ctx: &egui::Context, layer: egui::LayerId, from: usize, tone: Tone) {
    let to = shape_count(ctx, layer);
    if to <= from { return; }
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let list = d.get_temp_mut_or_default::<Toned>(toned_id());
        if list.pass != pass { list.pass = pass; list.spans.clear(); }
        list.spans.push((layer, from, to, tone));
    });
}

fn toned_spans(ctx: &egui::Context, layer: egui::LayerId) -> Vec<(usize, usize, Tone)> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<Toned>(toned_id()))
        .filter(|list| list.pass == pass)
        .map(|list| list.spans.iter().filter(|s| s.0 == layer).map(|s| (s.1, s.2, s.3)).collect())
        .unwrap_or_default()
}

/*
fn unmultiplied_rgba(color: Color32) -> ([f32; 3], u8) {
    let a = color.a();
    if a == 0 { return ([0.0; 3], 0); }
    let af = a as f32 / 255.0;
    ([(color.r() as f32 / 255.0 / af).min(1.0), (color.g() as f32 / 255.0 / af).min(1.0), (color.b() as f32 / 255.0 / af).min(1.0)], a)
}

fn needs_dark_outline(color: Color32, min_alpha: u8) -> Option<u8> {
    let (rgb, a) = unmultiplied_rgba(color);
    if a < min_alpha { return None; }
    let lum = luma(rgb);
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    (lum >= 0.26 || max >= 0.40).then_some(a)
}

fn dark_outline_color(alpha: u8, scale: f32) -> Color32 {
    Color32::from_rgba_premultiplied(0, 0, 0, ((alpha as f32) * scale).round().clamp(0.0, 255.0) as u8)
}

/// 替液態玻璃中的淺色文字補上 8 向 1pt 細黑邊，保留可讀性並維持 `glyph_index_start` 冪等。
pub(crate) fn outline_liquid_text(text_shape: &mut eframe::epaint::TextShape) {
    if text_shape.galley.is_empty() { return; }
    const OFFSETS: [(f32, f32, f32); 8] = [
        (-1.0, 0.0, 0.92), (1.0, 0.0, 0.92), (0.0, -1.0, 0.92), (0.0, 1.0, 0.92),
        (-0.85, -0.85, 0.78), (0.85, -0.85, 0.78), (-0.85, 0.85, 0.78), (0.85, 0.85, 0.78),
    ];
    let override_color = text_shape.override_text_color;
    let fallback_color = text_shape.fallback_color;
    let opacity_factor = text_shape.opacity_factor;
    let galley = std::sync::Arc::make_mut(&mut text_shape.galley);
    let mut changed = false;
    for row in &mut galley.rows {
        let expected_start = (row.visuals.glyph_vertex_range.start / 4) * 6;
        if row.visuals.glyph_index_start > expected_start { continue; }
        let v_range = row.visuals.glyph_vertex_range.clone();
        let glyph_count = v_range.len() / 4;
        let i_start = row.visuals.glyph_index_start;
        if glyph_count == 0 || row.visuals.mesh.indices.len() < i_start + glyph_count * 6 { continue; }
        let mut outline_indices: Vec<u32> = Vec::new();
        for g in 0..glyph_count {
            let v0 = v_range.start + g * 4;
            let i0 = i_start + g * 6;
            let v_col = row.visuals.mesh.vertices[v0].color;
            let base_fg = if let Some(over) = override_color { over }
                else if v_col == Color32::PLACEHOLDER { fallback_color }
                else { v_col };
            let effective_fg = if opacity_factor < 1.0 { base_fg.gamma_multiply(opacity_factor) } else { base_fg };
            if needs_dark_outline(effective_fg, 20).is_none() { continue; }
            let quad = [
                row.visuals.mesh.vertices[v0],
                row.visuals.mesh.vertices[v0 + 1],
                row.visuals.mesh.vertices[v0 + 2],
                row.visuals.mesh.vertices[v0 + 3],
            ];
            let rel = [
                row.visuals.mesh.indices[i0] - v0 as u32,
                row.visuals.mesh.indices[i0 + 1] - v0 as u32,
                row.visuals.mesh.indices[i0 + 2] - v0 as u32,
                row.visuals.mesh.indices[i0 + 3] - v0 as u32,
                row.visuals.mesh.indices[i0 + 4] - v0 as u32,
                row.visuals.mesh.indices[i0 + 5] - v0 as u32,
            ];
            let base_alpha = base_fg.a();
            for (dx, dy, scale) in OFFSETS {
                let outline_col = dark_outline_color(base_alpha, scale);
                let base_v = row.visuals.mesh.vertices.len() as u32;
                for mut v in quad {
                    v.pos += egui::vec2(dx, dy);
                    v.color = outline_col;
                    row.visuals.mesh.vertices.push(v);
                }
                for r in rel {
                    outline_indices.push(base_v + r);
                }
            }
        }
        if !outline_indices.is_empty() {
            let added = outline_indices.len();
            row.visuals.mesh.indices.splice(i_start..i_start, outline_indices);
            row.visuals.glyph_index_start += added;
            changed = true;
        }
    }
    if changed {
        galley.num_vertices = galley.rows.iter().map(|r| r.visuals.mesh.vertices.len()).sum();
        galley.num_indices = galley.rows.iter().map(|r| r.visuals.mesh.indices.len()).sum();
    }
}

fn outline_liquid_shape(shape: &mut Shape) {
    match shape {
        Shape::Vec(shapes) => shapes.iter_mut().for_each(outline_liquid_shape),
        Shape::Text(t) => outline_liquid_text(t),
        Shape::Rect(r) if r.brush.is_none() && r.rect.width() >= 18.0 && r.rect.height() >= 14.0 && r.stroke.width == 0.0 && (16..=90).contains(&r.fill.a()) => {
            r.stroke = egui::Stroke::new(0.9_f32, Color32::from_rgba_unmultiplied(210, 238, 255, 56));
        }
        Shape::Circle(c) if c.fill.a() < 20 && c.stroke.width > 0.0 => {
            if let Some(a) = needs_dark_outline(c.stroke.color, 40) {
                let mut backing = *c;
                backing.fill = Color32::TRANSPARENT;
                backing.stroke.width += 1.4;
                backing.stroke.color = dark_outline_color(a, 0.82);
                let original = shape.clone();
                *shape = Shape::Vec(vec![Shape::Circle(backing), original]);
            }
        }
        Shape::Path(p) if (!p.closed || p.fill.a() < 20) && p.stroke.width > 0.0 => {
            if let ColorMode::Solid(color) = p.stroke.color {
                if let Some(a) = needs_dark_outline(color, 40) {
                    let mut backing = p.clone();
                    backing.fill = Color32::TRANSPARENT;
                    backing.stroke.width += 1.4;
                    backing.stroke.color = ColorMode::Solid(dark_outline_color(a, 0.82));
                    let original = shape.clone();
                    *shape = Shape::Vec(vec![Shape::Path(backing), original]);
                }
            }
        }
        Shape::LineSegment { points, stroke } if stroke.width > 0.0 => {
            if let Some(a) = needs_dark_outline(stroke.color, 40) {
                let backing = Shape::LineSegment {
                    points: *points,
                    stroke: egui::Stroke::new(stroke.width + 1.4, dark_outline_color(a, 0.82)),
                };
                let original = shape.clone();
                *shape = Shape::Vec(vec![backing, original]);
            }
        }
        _ => {}
    }
}
*/

/// 給某個圖層中 `ranges` 範圍內的圖形換色。
///
/// 先找出這些範圍裡的實心彩色底塊；之後畫在它們上面的白字、白色圖示保持白色
/// （「儲存」按鈕、播放鍵裡的三角、開關的滑塊），其餘照常換色。
pub fn recolor_layer(ctx: &egui::Context, layer: egui::LayerId, ranges: &[(usize, usize)], p: &Palette) {
    let toned = toned_spans(ctx, layer);
    // let liquid = *p == LIQUID_GLASS;
    ctx.graphics_mut(|g| {
        let list = g.entry(layer);
        let end = list.next_idx().0;
        let mut fills: Vec<(usize, egui::Rect)> = Vec::new();
        if light_paper(p) {
            for (i, entry) in list.all_entries().enumerate() {
                if ranges.iter().any(|&(from, to)| i >= from && i < to.min(end)) {
                    if let Some(rect) = colored_fill(&entry.shape) { fills.push((i, rect)); }
                }
            }
        }
        for &(from, to) in ranges {
            for i in from..to.min(end) {
                if let Some(&(_, _, tone)) = toned.iter().rev().find(|&&(a, b, _)| i >= a && i < b) {
                    list.mutate_shape(egui::layers::ShapeIdx(i), |s| {
                        recolor_with(&mut s.shape, p, tone);
                        // if liquid { outline_liquid_shape(&mut s.shape); }
                    });
                    continue;
                }
                list.mutate_shape(egui::layers::ShapeIdx(i), |s| {
                    if !fills.is_empty() && light_neutral(&s.shape) {
                        let center = s.shape.visual_bounding_rect().center();
                        if fills.iter().any(|&(at, rect)| at < i && rect.contains(center)) { return; }
                    }
                    recolor(&mut s.shape, p);
                    // if liquid { outline_liquid_shape(&mut s.shape); }
                });
            }
        }
    });
}

/// 當前圖層已有的圖形數量，用來標記換色範圍。
pub fn shape_count(ctx: &egui::Context, layer: egui::LayerId) -> usize {
    ctx.graphics_mut(|g| g.entry(layer).next_idx().0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutrals_swap_lightness_and_keep_alpha() {
        assert_eq!(map_color(Color32::WHITE, &CREAM), Color32::from_rgb(46, 32, 22));
        assert_eq!(map_color(Color32::BLACK, &CREAM), Color32::from_rgb(255, 243, 228));
        let faint = map_color(Color32::from_white_alpha(40), &CREAM);
        assert_eq!(faint.a(), 40, "半透明高光保留透明度");
        assert!(faint.r() <= 8, "半透明的白仍按白處理：{faint:?}");
        let gray = map_color(Color32::from_rgb(142, 142, 153), &CREAM).to_srgba_unmultiplied();
        assert!(gray[0] > 46 && gray[0] < 255, "灰階落在紙色與墨色之間");
        assert_eq!(map_color(Color32::TRANSPARENT, &CREAM), Color32::TRANSPARENT);
        // 毛玻璃淺色：深色卡片帶上通透感，文字仍不透明
        assert!(map_color(Color32::from_rgb(22, 22, 24), &GLASS_LIGHT).a() < 230);
        assert_eq!(map_color(Color32::WHITE, &GLASS_LIGHT).a(), 255);
    }

    #[test]
    fn blue_accents_take_the_skin_color_and_other_hues_darken() {
        let blue = map_color(Color32::from_rgb(62, 172, 248), &CREAM);
        assert_eq!(blue, Color32::from_rgb(240, 138, 36), "強調藍換成奶油皮膚的橙色");
        let deep = map_color(Color32::from_rgb(15, 38, 64), &CREAM).to_srgba_unmultiplied();
        assert!(deep[0] > 200, "深藍底色變成淺橙底：{deep:?}");
        let brand = map_color(Color32::from_rgb(40, 126, 255), &CREAM).to_srgba_unmultiplied();
        assert!(brand[2] > brand[0], "使用者選的品牌藍不能被換成橙色：{brand:?}");
        let red = map_color(Color32::from_rgb(242, 48, 70), &CREAM).to_srgba_unmultiplied();
        assert!(red[0] > red[1] && red[0] > 150, "紅色保留色相：{red:?}");
        let f = |c: [u8; 4]| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
        let paper = [255.0 / 255.0, 243.0 / 255.0, 228.0 / 255.0];
        for bright in [Color32::from_rgb(253, 104, 92), Color32::from_rgb(255, 196, 61), Color32::from_rgb(118, 196, 151), Color32::from_rgb(48, 209, 88)] {
            let m = map_color(bright, &CREAM).to_srgba_unmultiplied();
            assert!(crate::glass::contrast(f(m), paper) >= 2.95, "淺亮彩色在淺底上要加深到可讀：{bright:?} -> {m:?}");
        }
        let save = map_color(Color32::from_rgb(82, 162, 255), &CREAM);
        assert_eq!(save, Color32::from_rgb(240, 138, 36), "介面按鈕藍跟隨奶油主題色");
        let faded_red = map_color(Color32::from_rgb(253, 104, 92).gamma_multiply(0.6), &CREAM).to_srgba_unmultiplied();
        let fr = [faded_red[0] as f32 / 255.0, faded_red[1] as f32 / 255.0, faded_red[2] as f32 / 255.0];
        let seen = [paper[0] + (fr[0] - paper[0]) * faded_red[3] as f32 / 255.0, paper[1] + (fr[1] - paper[1]) * faded_red[3] as f32 / 255.0, paper[2] + (fr[2] - paper[2]) * faded_red[3] as f32 / 255.0];
        assert!(crate::glass::contrast(seen, paper) >= 2.9, "半透明的彩色字也要可讀：{faded_red:?}");
        // 深色彩色底塊（選中態）變成淺色淡彩，墨色文字壓在上面清晰可讀
        let chip = map_color(Color32::from_rgb(30, 58, 110), &GLASS_LIGHT).to_srgba_unmultiplied();
        assert!(chip[0] > 180 && chip[2] > chip[0], "深色選中底塊變為淺淡藍：{chip:?}");
        let ink = [22.0 / 255.0, 22.0 / 255.0, 26.0 / 255.0];
        assert!(crate::glass::contrast(f(chip), ink) >= 7.0);
    }

    #[test]
    fn dark_glass_brightens_gray_text_and_keeps_white_and_accents() {
        assert_eq!(map_color(Color32::WHITE, &GLASS_DARK), Color32::WHITE);
        let gray = map_color(Color32::from_rgb(142, 142, 153), &GLASS_DARK).to_srgba_unmultiplied();
        assert!(gray[0] > 165, "灰色文字提亮：{gray:?}");
        assert_eq!(map_color(Color32::from_rgb(62, 172, 248), &GLASS_DARK), Color32::from_rgb(62, 172, 248), "強調色不變");
        assert!(map_color(Color32::from_rgb(22, 22, 24), &GLASS_DARK).a() < 160, "卡片底帶通透感");
    }

    #[test]
    fn fading_text_keeps_its_fade_after_recoloring() {
        // 切頁時舊頁面的文字按透明度淡出；換色後必須仍是半透明，不能和新頁面疊在一起。
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            let mut faded = painter.clone();
            faded.multiply_opacity(0.25);
            faded.text(egui::Pos2::ZERO, egui::Align2::LEFT_TOP, "淡出", egui::FontId::proportional(14.0), Color32::WHITE);
            recolor_layer(ctx, egui::LayerId::background(), &[(0, 1)], &CREAM);
            let shapes: Vec<_> = ctx.graphics_mut(|g| g.entry(egui::LayerId::background()).all_entries().cloned().collect());
            let Shape::Text(t) = &shapes[0].shape else { panic!() };
            let color = t.galley.rows.iter().flat_map(|r| r.visuals.mesh.vertices.iter()).next().unwrap().color;
            assert_eq!(color.a(), 64, "淡出透明度要保留：{color:?}");
            assert!(color.r() <= 13, "淡出的白字換成同樣淡出的墨色：{color:?}");
            assert!(t.override_text_color.is_none(), "不能用統一顏色覆蓋整段文字");
        });
    }

    #[test]
    fn monochrome_icons_follow_the_text_color() {
        let id = egui::TextureId::User(4242);
        mark_monochrome(id);
        let mut shape = Shape::image(id, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(10.0)), egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        recolor(&mut shape, &CREAM);
        let tint = match &shape { Shape::Rect(r) => r.fill, Shape::Mesh(m) => m.vertices[0].color, other => panic!("{other:?}") };
        assert_eq!(tint, Color32::from_rgb(46, 32, 22), "白色標識在淺底上變成墨色");
    }

    #[test]
    fn white_marks_on_colored_buttons_stay_white() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let layer = egui::LayerId::background();
            let painter = ctx.layer_painter(layer);
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(60.0, 24.0)), 12.0, Color32::from_rgb(82, 162, 255));
            painter.text(egui::pos2(30.0, 12.0), egui::Align2::CENTER_CENTER, "儲存", egui::FontId::proportional(12.0), Color32::WHITE);
            painter.text(egui::pos2(120.0, 12.0), egui::Align2::CENTER_CENTER, "取消", egui::FontId::proportional(12.0), Color32::WHITE);
            recolor_layer(ctx, layer, &[(0, 3)], &CREAM);
            let shapes: Vec<_> = ctx.graphics_mut(|g| g.entry(layer).all_entries().cloned().collect());
            let first = |s: &Shape| match s { Shape::Text(t) => t.galley.rows.iter().flat_map(|r| r.visuals.mesh.vertices.first()).next().unwrap().color, _ => panic!() };
            assert_eq!(first(&shapes[1].shape), Color32::WHITE, "彩色按鈕上的白字保持白色");
            assert_eq!(first(&shapes[2].shape), Color32::from_rgb(46, 32, 22), "普通白字換成墨色");
            let Shape::Rect(r) = &shapes[0].shape else { panic!() };
            assert_eq!(r.fill, Color32::from_rgb(240, 138, 36), "按鈕本身換成主題色");
        });
    }

    #[test]
    fn textured_meshes_are_left_alone() {
        let mut image = egui::Mesh::with_texture(egui::TextureId::User(7));
        image.vertices.push(eframe::epaint::Vertex { pos: egui::Pos2::ZERO, uv: egui::Pos2::ZERO, color: Color32::WHITE });
        let mut shape = Shape::mesh(image);
        recolor(&mut shape, &CREAM);
        let Shape::Mesh(mesh) = shape else { unreachable!() };
        assert_eq!(mesh.vertices[0].color, Color32::WHITE);
    }

    /*
    #[test]
    fn liquid_glass_adds_black_outline_to_light_text_and_strokes() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let layer = egui::LayerId::background();
            let painter = ctx.layer_painter(layer);
            painter.text(egui::Pos2::ZERO, egui::Align2::LEFT_TOP, "13:30 進行中", egui::FontId::proportional(14.0), Color32::WHITE);
            painter.text(egui::pos2(0.0, 24.0), egui::Align2::LEFT_TOP, "深字", egui::FontId::proportional(14.0), Color32::from_rgb(20, 20, 24));
            painter.line_segment([egui::pos2(0.0, 48.0), egui::pos2(20.0, 48.0)], egui::Stroke::new(2.0_f32, Color32::WHITE));
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(0.0, 60.0), egui::vec2(120.0, 40.0)), 8.0, Color32::from_rgb(22, 22, 24));
            recolor_layer(ctx, layer, &[(0, 4)], &LIQUID_GLASS);
            let shapes: Vec<_> = ctx.graphics_mut(|g| g.entry(layer).all_entries().cloned().collect());

            // 淺色文字應獲得 8 向黑邊且 glyph_index_start 前移，再呼一次維持冪等
            let Shape::Text(mut light_text) = shapes[0].shape.clone() else { panic!() };
            let row = &light_text.galley.rows[0];
            assert!(row.visuals.glyph_index_start > 0, "應插入黑邊索引並推進 glyph_index_start");
            assert_eq!(row.visuals.mesh.vertices.len(), row.visuals.glyph_vertex_range.len() * 9);
            assert!(row.visuals.mesh.vertices[row.visuals.glyph_vertex_range.end..].iter().all(|v| v.color.r() == 0 && v.color.g() == 0 && v.color.b() == 0 && v.color.a() > 150));
            let before_v = row.visuals.mesh.vertices.len();
            outline_liquid_text(&mut light_text);
            assert_eq!(light_text.galley.rows[0].visuals.mesh.vertices.len(), before_v, "重複勾邊應冪等");

            // 深色文字不加黑邊
            let Shape::Text(dark_text) = &shapes[1].shape else { panic!() };
            assert_eq!(dark_text.galley.rows[0].visuals.glyph_index_start, 0);

            // 淺色向量線段應包成背襯黑邊 + 原線段
            let Shape::Vec(segs) = &shapes[2].shape else { panic!("線段應加上黑邊底襯：{:?}", shapes[2].shape) };
            assert_eq!(segs.len(), 2);

            // 內層卡片透明化並加上細微玻璃內描邊
            let Shape::Rect(card) = &shapes[3].shape else { panic!() };
            assert_eq!(card.fill.a(), 51);
            assert!(card.stroke.width > 0.5 && card.stroke.color.a() > 0);
        });
    }
    */
}
