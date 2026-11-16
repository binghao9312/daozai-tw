//! 懸浮雲：微灰玻璃底 + 封面主色淡彩 + 游標光暈 + 進度條拖動光效。
//!
//! 這裡只負責「算顏色」——給定島體區域性座標返回一個頂點顏色，由
//! [`crate::shape::fill_shaded`] 在填充輪廓網格時逐頂點取樣，因此所有光都天然
//! 被藥丸／面板的圓角輪廓裁住，不會溢位到桌面，也不需要額外裁剪層。
//!
//! 設計取向（對齊參考效果）：
//! - **底色是中性微灰**，不是自帶的霓虹漸變；顏色只從封面主色和桌面背景裡來；
//! - **換曲只換色，不閃**：底色約 0.4 s 交叉淡入到新封面主色，全程沒有額外提亮，
//!   所以切歌不會突兀；音樂關掉時同樣淡回中性灰；
//! - **游標光暈**跟隨滑鼠在島上的位置，很淡很散，移開即衰減消失；
//! - **追光**：進度條頭部常駐一團柔光，隨播放前進，按住拖動時略微加強；
//! - **靜止即靜止**：沒有播放、沒有游標移動、沒有換曲與拖動時，
//!   [`Fluid::animating`] 返回 false，介面可以降到低頻重新整理，這是控制功耗的關鍵。

use eframe::egui::{Color32, ColorImage};

/// 換曲時底色交叉淡入的時長（秒）。
const TINT_FADE: f64 = 0.45;
/// 游標光暈的橫向 / 縱向半徑（邏輯點）。
const GLOW_RX: f32 = 152.0;
const GLOW_RY: f32 = 100.0;
/// 進度條光斑：頭部往左的拖尾長度、頭前收邊寬度與豎向尺度。
const SEEK_TAIL: f32 = 96.0;
const SEEK_LEAD: f32 = 16.0;
const SEEK_RY: f32 = 30.0;

/// 沒有封面時用的中性色：接近白，乘上低強度後就是一層灰霧。
pub const NEUTRAL: [f32; 3] = [0.62, 0.66, 0.74];

pub struct Fluid {
    t: f64,
    /// 交叉淡入的兩端：`tint_from → tint_to`，`tint_at` 為起點。
    tint_from: [f32; 3],
    tint_to: [f32; 3],
    tint_at: f64,
    /// 游標目標位置（島體區域性座標）與平滑後的實際位置。
    cursor: Option<(f32, f32)>,
    glow: (f32, f32),
    glow_a: f32,
    /// 光暈是否還在追隨（用於判斷要不要繼續重新整理）。
    glow_moving: bool,
    /// 拖動進度條時的光斑位置與強度。
    seek: Option<(f32, f32)>,
    seek_xy: (f32, f32),
    seek_a: f32,
    /// 追光的目標亮度：暫停 0.3、播放 0.5、按住拖動 1.0。
    seek_hot: f32,
}

impl Default for Fluid {
    fn default() -> Self {
        Self {
            t: 0.0,
            tint_from: NEUTRAL,
            tint_to: NEUTRAL,
            tint_at: f64::NEG_INFINITY,
            cursor: None,
            glow: (0.0, 0.0),
            glow_a: 0.0,
            glow_moving: false,
            seek: None,
            seek_xy: (0.0, 0.0),
            seek_a: 0.0,
            seek_hot: 0.0,
        }
    }
}

impl Fluid {
    pub fn new() -> Self {
        Self::default()
    }

    /// 中性色（音樂停止時淡回的顏色）。
    pub fn neutral() -> [f32; 3] {
        NEUTRAL
    }

    // ---------- 每幀推進 ----------

    pub fn advance(&mut self, t: f64, dt: f32) {
        self.t = t;
        let k = (dt * 15.0).clamp(0.0, 1.0);
        match self.cursor {
            Some((tx, ty)) => {
                let (dx, dy) = (tx - self.glow.0, ty - self.glow.1);
                self.glow.0 += dx * k;
                self.glow.1 += dy * k;
                self.glow_a += (1.0 - self.glow_a) * (dt * 5.5).clamp(0.0, 1.0);
                self.glow_moving = dx.abs() + dy.abs() > 0.35;
            }
            None => {
                self.glow_a += (0.0 - self.glow_a) * (dt * 6.5).clamp(0.0, 1.0);
                self.glow_moving = self.glow_a > 0.004;
            }
        }
        if self.glow_a < 0.003 {
            self.glow_a = 0.0;
        }

        // 進度條光斑：拖動時緊跟滑塊，鬆手後快速收掉。
        match self.seek {
            Some((sx, sy)) => {
                let k = (dt * 18.0).clamp(0.0, 1.0);
                self.seek_xy.0 += (sx - self.seek_xy.0) * k;
                self.seek_xy.1 += (sy - self.seek_xy.1) * k;
                self.seek_a += (self.seek_hot - self.seek_a) * (dt * 6.0).clamp(0.0, 1.0);
            }
            None => {
                self.seek_a += (0.0 - self.seek_a) * (dt * 6.0).clamp(0.0, 1.0);
            }
        }
        if self.seek_a < 0.003 {
            self.seek_a = 0.0;
        }
    }

    /// 游標位置（島體區域性座標）；移出島體傳 None。
    pub fn set_cursor(&mut self, at: Option<(f32, f32)>) {
        if let (Some(p), None) = (at, self.cursor) {
            // 首次進入：光暈直接落在游標處，不要從左上角飛過來。
            self.glow = p;
        }
        self.cursor = at;
    }

    /// 拖動進度條時光斑所在的位置（島體區域性座標）；鬆手傳 None。
    /// 追光位置（島體區域性座標，進度條頭部）與目標亮度；沒有音源傳 None。
    pub fn set_seek(&mut self, at: Option<(f32, f32)>, hot: f32) {
        if let (Some(p), None) = (at, self.seek) {
            self.seek_xy = p;
            self.seek_a = 0.0;
        }
        self.seek = at;
        self.seek_hot = hot.clamp(0.0, 1.0);
    }

    // ---------- 變色 ----------

    /// 目標主色變化：底色交叉淡入過去。**只換色，不提亮**，所以切歌不會突兀。
    pub fn set_tint(&mut self, rgb: [f32; 3]) {
        if rgb == self.tint_to {
            return;
        }
        self.tint_from = self.tint_now();
        self.tint_to = rgb;
        self.tint_at = self.t;
    }

    /// 當前封面主色（向白色提亮 `to_white`），供進度條追光等元素共用，
    /// 這樣換歌時介面上的光會跟著一起換色，整體才協調。
    pub fn tint_color(&self, to_white: f32) -> Color32 {
        let c = self.tint_now();
        let k = to_white.clamp(0.0, 1.0);
        let q = |v: f32| ((v + (1.0 - v) * k) * 255.0).clamp(0.0, 255.0) as u8;
        Color32::from_rgb(q(c[0]), q(c[1]), q(c[2]))
    }

    /// 當前應當顯示的封面主色。
    pub fn tint_now(&self) -> [f32; 3] {
        let u = ((self.t - self.tint_at) / TINT_FADE).clamp(0.0, 1.0) as f32;
        let e = 1.0 - (1.0 - u).powi(3);
        let mut out = [0.0; 3];
        for i in 0..3 {
            out[i] = self.tint_from[i] + (self.tint_to[i] - self.tint_from[i]) * e;
        }
        out
    }

    // ---------- 底色 ----------

    /// 玻璃底色透明度：(頂部, 底部)。合上整體半透，展開頂部實、下半漸透。
    pub fn glass_alpha(pc: f32) -> (f32, f32) {
        let pc = pc.clamp(0.0, 1.0);
        let collapsed = 0.52;
        (collapsed + (0.88 - collapsed) * pc, collapsed + (0.34 - collapsed) * pc)
    }

    /// 中性微灰的明度（0..255）：收起時偏亮（像磨砂玻璃），展開時偏深（壓得住文字）。
    pub fn gray_at(expand: f32, v: f32) -> f32 {
        let expand = expand.clamp(0.0, 1.0);
        let base = 54.0 + (36.0 - 54.0) * expand;
        let drop = (4.0 + 10.0 * expand) * v.clamp(0.0, 1.0);
        (base - drop).clamp(18.0, 80.0)
    }

    // ---------- 合成 ----------

    /// 把一個頂點合成成顏色（未預乘）。`x/y` 是島體區域性座標。
    pub fn compose(&self, x: f32, y: f32, w: f32, h: f32, expand: f32) -> Color32 {
        let w = w.max(1.0);
        let h = h.max(1.0);
        let (u, v) = ((x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0));
        let expand = expand.clamp(0.0, 1.0);

        // 1) 中性微灰底
        let (at, ab) = Self::glass_alpha(expand);
        let a0 = at + (ab - at) * v.powf(1.3);
        let g = Self::gray_at(expand, v) / 255.0;
        let mut a = a0;
        let mut c = [g * a0; 3];

        // 2) 封面主色淡彩：貼著封面那一側最濃，向右迅速衰減
        let tint = self.tint_now();
        let edge = 0.22 + 0.78 * (-(u / 0.32).powi(2)).exp();
        over(&mut c, &mut a, tint, 0.22 * edge * (0.55 + 0.45 * expand));

        // 3) 游標光暈：跟著滑鼠的一團散光
        if self.glow_a > 0.003 {
            let dx = (x - self.glow.0) / GLOW_RX;
            let dy = (y - self.glow.1) / GLOW_RY;
            let e = (-(dx * dx + dy * dy) * 1.5).exp();
            over(&mut c, &mut a, [1.0, 1.0, 1.0], 0.12 * e * self.glow_a);
        }

        // 4) 追光：頭部往**後**拖一條很淡的尾光，頭前幾乎立刻收掉。
        //    和進度條上那道彗尾同方向，面板上才不會出現「一團圓燈」。
        if self.seek_a > 0.003 {
            let dx = x - self.seek_xy.0;
            let along = if dx >= 0.0 {
                (-(dx / SEEK_LEAD).powi(2) * 1.6).exp()
            } else {
                (-(-dx / SEEK_TAIL).powf(1.3)).exp()
            };
            let dy = (y - self.seek_xy.1) / SEEK_RY;
            let e = along * (-(dy * dy) * 1.6).exp();
            over(&mut c, &mut a, tint, 0.065 * e * self.seek_a);
        }

        unmultiply(c, a)
    }

    /// 是否還需要連續重新整理（功耗閘門）。
    pub fn animating(&self) -> bool {
        (self.t - self.tint_at) < TINT_FADE
            || self.glow_moving
            || (self.glow_a > 0.0 && self.glow_a < 0.996)
            || (self.seek_a - if self.seek.is_some() { self.seek_hot } else { 0.0 }).abs() > 0.004
    }

    // ---------- 取色 ----------

    /// 從封面圖取一個「不太髒」的主色：暗部和低飽和畫素權重更低，
    /// 取完再把明度歸一，保證淡彩看得見但不會變成霓虹。
    pub fn tint_from_image(image: &ColorImage) -> [f32; 3] {
        let [w, h] = image.size;
        if w == 0 || h == 0 {
            return NEUTRAL;
        }
        let step = (w.min(h) / 16).max(1);
        let (mut r, mut g, mut b, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut y = 0;
        while y < h {
            let mut x = 0;
            while x < w {
                let p = image.pixels[y * w + x];
                let (rr, gg, bb) = (p.r() as f32 / 255.0, p.g() as f32 / 255.0, p.b() as f32 / 255.0);
                let mx = rr.max(gg).max(bb);
                let mn = rr.min(gg).min(bb);
                let sat = if mx > 0.001 { (mx - mn) / mx } else { 0.0 };
                let wgt = (0.25 + sat) * (0.30 + mx);
                r += rr * wgt;
                g += gg * wgt;
                b += bb * wgt;
                n += wgt;
                x += step;
            }
            y += step;
        }
        if n <= 0.0 {
            return NEUTRAL;
        }
        normalize([r / n, g / n, b / n])
    }

    /// 沒有封面時的兜底色：去飽和的標題雜湊色，只作很淡的暗示。
    pub fn tint_from_title(title: &str) -> [f32; 3] {
        if title.trim().is_empty() {
            return NEUTRAL;
        }
        let mut h: u32 = 2166136261;
        for b in title.bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(16777619);
        }
        normalize(hsv((h % 360) as f32, 0.30, 0.92))
    }
}

/// 把顏色歸一：最大分量抬到 0.9，避免深色封面把淡彩壓沒。
fn normalize(c: [f32; 3]) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]).max(1e-3);
    let k = 0.90 / mx;
    [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)]
}

/// 預乘空間裡的 source-over 疊加。
fn over(c: &mut [f32; 3], a: &mut f32, src: [f32; 3], sa: f32) {
    let sa = sa.clamp(0.0, 1.0);
    if sa <= 0.0005 {
        return;
    }
    for i in 0..3 {
        c[i] = c[i] * (1.0 - sa) + src[i] * sa;
    }
    *a += sa * (1.0 - *a);
}

fn unmultiply(c: [f32; 3], a: f32) -> Color32 {
    let inv = if a > 1e-4 { 1.0 / a } else { 0.0 };
    let q = |v: f32| (v * inv * 255.0).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(q(c[0]), q(c[1]), q(c[2]), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_of(color: Color32) -> ColorImage {
        ColorImage { size: [8, 8], pixels: vec![color; 64] }
    }

    fn lum(c: Color32) -> u32 {
        c.r() as u32 + c.g() as u32 + c.b() as u32
    }

    #[test]
    fn collapsed_is_translucent_and_expanded_fades_toward_the_bottom() {
        let (top, bottom) = Fluid::glass_alpha(0.0);
        assert!((top - bottom).abs() < 1e-6, "收起時是一條均勻的半透藥丸");
        assert!(top > 0.45 && top < 0.75, "半透但不能糊掉文字：{top}");

        let (top, bottom) = Fluid::glass_alpha(1.0);
        assert!(top > 0.85, "展開後頂部要壓得住文字：{top}");
        assert!(bottom < 0.45, "展開後下半段要透出桌面：{bottom}");
        assert!(top > bottom);
    }

    #[test]
    fn base_is_neutral_gray_not_a_neon_wash() {
        let f = Fluid::new();
        let a = f.compose(40.0, 12.0, 163.0, 30.0, 0.0);
        let b = f.compose(120.0, 26.0, 163.0, 30.0, 0.0);
        for c in [a, b] {
            let mx = c.r().max(c.g()).max(c.b()) as i32;
            let mn = c.r().min(c.g()).min(c.b()) as i32;
            assert!(mx - mn <= 8, "底色應接近中性灰，色差只有 {}", mx - mn);
            assert!(c.r() > 18, "微灰不能退化成純黑");
        }
    }

    #[test]
    fn track_change_fades_color_without_any_brightness_step() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        f.set_tint([0.9, 0.2, 0.2]);
        f.advance(1.0 + TINT_FADE, 0.016);
        let red = f.compose(60.0, 10.0, 419.0, 140.0, 1.0);

        // 換曲：逐幀看亮度，任何一幀都不許跳（這就是「突兀」的來源）。
        f.set_tint([0.2, 0.8, 0.4]);
        let mut prev = lum(f.compose(60.0, 10.0, 419.0, 140.0, 1.0));
        for i in 0..40 {
            f.advance(f.t + 0.016, 0.016);
            let cur = lum(f.compose(60.0, 10.0, 419.0, 140.0, 1.0));
            let step = (cur as i32 - prev as i32).abs();
            assert!(step <= 8, "第 {i} 幀亮度跳了 {step}，切歌會顯得突兀");
            prev = cur;
        }
        let green = f.compose(60.0, 10.0, 419.0, 140.0, 1.0);
        assert!(green.g() > red.g(), "淡入結束應當換成新主色");
    }

    #[test]
    fn color_returns_to_neutral_when_music_stops() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        f.set_tint([0.95, 0.15, 0.15]);
        f.advance(1.0 + TINT_FADE, 0.016);
        let colored = f.compose(60.0, 70.0, 419.0, 140.0, 1.0);
        f.set_tint(Fluid::neutral());
        f.advance(20.0, 0.016);
        f.advance(20.0 + TINT_FADE + 0.05, 0.016);
        let after = f.compose(60.0, 70.0, 419.0, 140.0, 1.0);
        assert!(lum(colored) != lum(after) || colored.r() != after.r(), "必須淡回中性");
        // 回到中性後不應再帶明顯色相
        let mx = after.r().max(after.g()).max(after.b()) as i32;
        let mn = after.r().min(after.g()).min(after.b()) as i32;
        assert!(mx - mn <= 10, "音樂停止後底色應當回到中性灰：{}", mx - mn);
    }

    #[test]
    fn bloom_is_gone_so_animating_stops_after_the_fade() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        f.set_tint([0.9, 0.3, 0.2]);
        assert!(f.animating(), "淡入期間要重新整理");
        f.advance(1.0 + TINT_FADE + 0.1, 0.016);
        assert!(!f.animating(), "淡入結束就該停手");
    }

    #[test]
    fn cursor_glow_fades_in_and_out() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        f.set_cursor(Some((80.0, 15.0)));
        for _ in 0..40 {
            f.advance(f.t + 0.016, 0.016);
        }
        assert!(f.glow_a > 0.9, "遊標在島上時光暈要亮起來：{}", f.glow_a);
        f.set_cursor(None);
        for _ in 0..60 {
            f.advance(f.t + 0.016, 0.016);
        }
        assert_eq!(f.glow_a, 0.0, "移開後光暈必須完全消失");
    }

    #[test]
    fn glow_reaches_middle_and_top_regions() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        // 面板正中偏上：這正是舊版網格被抹平、完全看不到光的位置。
        let point = (209.5f32, 46.0f32);
        let before = f.compose(point.0, point.1, 419.0, 140.0, 1.0);
        f.set_cursor(Some(point));
        for _ in 0..40 {
            f.advance(f.t + 0.016, 0.016);
        }
        let after = f.compose(point.0, point.1, 419.0, 140.0, 1.0);
        assert!(lum(after) > lum(before) + 15, "中間偏上區域必須有明顯光暈");
    }

    #[test]
    fn cursor_glow_is_smooth_across_neighbouring_samples() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        f.set_cursor(Some((209.5, 70.0)));
        for _ in 0..40 {
            f.advance(f.t + 0.016, 0.016);
        }
        // 沿豎線取樣：相鄰取樣點的亮度不應出現臺階（網格抹平會表現為突變或平段）。
        let mut prev = lum(f.compose(209.5, 2.0, 419.0, 140.0, 1.0));
        for i in 1..28 {
            let y = 2.0 + i as f32 * 5.0;
            let cur = lum(f.compose(209.5, y, 419.0, 140.0, 1.0));
            let step = (cur as i32 - prev as i32).abs();
            assert!(step <= 60, "第 {i} 個取樣點亮度跳了 {step}，光暈不連續");
            prev = cur;
        }
    }

    #[test]
    fn chase_light_follows_the_head_and_is_soft() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        let before = f.compose(120.0, 92.0, 419.0, 140.0, 1.0);
        // 頭前 / 頭後各取一個基準點（此時還沒有追光），後面用來算各自的抬升
        let base_behind = f.compose(260.0, 92.0, 419.0, 140.0, 1.0);
        let base_ahead = f.compose(340.0, 92.0, 419.0, 140.0, 1.0);

        // 播放中：追光常駐，亮度只有拖動時的一半左右。
        f.set_seek(Some((120.0, 92.0)), 0.5);
        for _ in 0..40 {
            f.advance(f.t + 0.016, 0.016);
        }
        let playing = f.compose(120.0, 92.0, 419.0, 140.0, 1.0);
        let spread = f.compose(120.0, 62.0, 419.0, 140.0, 1.0);
        assert!(lum(playing) > lum(before), "進度頭要亮於底色");
        assert!(lum(spread) > lum(before), "光要散得開，不能只是一顆硬點");
        // 柔和：單通道抬升不超過 30 級（舊版是 40 級以上的硬白核）
        let lift = (lum(playing) - lum(before)) as f32 / 3.0;
        assert!(lift < 30.0, "追光太亮了：+{lift:.0}/255");

        // 拖動時更亮，但不能亮到突兀
        f.set_seek(Some((120.0, 92.0)), 1.0);
        for _ in 0..40 {
            f.advance(f.t + 0.016, 0.016);
        }
        let dragging = f.compose(120.0, 92.0, 419.0, 140.0, 1.0);
        assert!(lum(dragging) > lum(playing), "拖動應當比播放時更亮");
        let lift = (lum(dragging) - lum(before)) as f32 / 3.0;
        assert!(lift < 55.0, "拖動光也太亮了：+{lift:.0}/255");

        // 追光隨位置移動：同一個取樣點，頭壓過來時才亮
        // （底色本身左邊偏彩、右邊偏灰，跨點比亮度會被底色差淹沒，所以固點比）。
        let at300 = f.compose(300.0, 92.0, 419.0, 140.0, 1.0);
        f.set_seek(Some((300.0, 92.0)), 1.0);
        for _ in 0..40 {
            f.advance(f.t + 0.016, 0.016);
        }
        let moved = f.compose(300.0, 92.0, 419.0, 140.0, 1.0);
        assert!(lum(moved) > lum(at300), "追光要跟著走");

        // 並且是**朝後**拖：同樣離開頭部 40 DIP，頭後的抬升要遠大於頭前。
        let behind = f.compose(260.0, 92.0, 419.0, 140.0, 1.0);
        let ahead = f.compose(340.0, 92.0, 419.0, 140.0, 1.0);
        let lift_behind = lum(behind) as i32 - lum(base_behind) as i32;
        let lift_ahead = lum(ahead) as i32 - lum(base_ahead) as i32;
        assert!(lift_behind > lift_ahead + 6, "光應當朝後拖：頭後 +{lift_behind}，頭前 +{lift_ahead}");

        // 沒有音源後收乾淨
        f.set_seek(None, 0.0);
        for _ in 0..80 {
            f.advance(f.t + 0.016, 0.016);
        }
        assert_eq!(f.seek_a, 0.0, "沒有音源後必須收乾淨");
        assert!(!f.animating(), "收乾淨後不應繼續重新整理");
    }

    #[test]
    fn tint_from_image_ignores_dark_borders() {
        let mut img = image_of(Color32::from_rgb(0, 0, 0));
        for y in 2..6 {
            for x in 2..6 {
                img.pixels[y * 8 + x] = Color32::from_rgb(220, 60, 90);
            }
        }
        let t = Fluid::tint_from_image(&img);
        assert!(t[0] > t[1] && t[0] > t[2], "主色應當偏紅：{t:?}");
    }

    #[test]
    fn compose_alpha_stays_within_bounds() {
        let mut f = Fluid::new();
        f.advance(1.0, 0.016);
        f.set_tint([0.9, 0.3, 0.2]);
        f.set_cursor(Some((40.0, 10.0)));
        for _ in 0..30 {
            f.advance(f.t + 0.016, 0.016);
        }
        for expand in [0.0, 0.5, 1.0] {
            for y in [0.0, 14.0, 139.0] {
                let c = f.compose(80.0, y, 419.0, 140.0, expand);
                let (at, ab) = Fluid::glass_alpha(expand);
                let floor = at.min(ab) * 255.0 - 1.0;
                assert!(c.a() as f32 >= floor, "alpha 不能被壓到低於底色：{}", c.a());
            }
        }
    }
}
