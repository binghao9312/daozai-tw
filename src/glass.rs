//! 玻璃材質的數值，逐條取自 oyenza-studio（MIT）。
//!
//! 對應關係見 `CHANGES-v0.5.5.md` 的對照表：
//! - `crates/poni-core/src/glass.rs`：煙燻不透明度、最小/最大著色、對比度目標、
//!   厚度、陰影；
//! - `crates/poni/src/glass.rs`：霜層模糊、rim/glow 強度、飽和度；
//! - `crates/poni/res/glass.hlsl`：`dark = mat * (1 - tint)`、rim 與 glow 的疊加方式；
//! - `crates/poni-core/src/look.rs`：玻璃底色與描邊。
//!
//! 這裡只保留「黑色疊加」這一條路徑：`col = mat * (1 - tint)` 與「在背景上疊
//! 一層黑色、alpha = tint」在數學上等價，正好能用 egui 的半透明填充表達。

/// 看不到螢幕時的煙燻玻璃不透明度（oyenza `glass.rs:21`）。
pub const SMOKED_OPACITY: f32 = 0.82;
/// 深色玻璃的著色範圍（`crates/poni/src/glass.rs:673`）。
pub const TINT_MIN: f32 = 0.34;
pub const TINT_MAX: f32 = 0.85;
/// 文字要達到的對比度（`poni-core/src/glass.rs:173`）。
pub const TEXT_CONTRAST: f32 = 4.8;
/// 玻璃最薄/最厚的高度（DIP，`poni-core/src/glass.rs:25-26`）。
const THIN_H: f32 = 37.0;
const THICK_H: f32 = 220.0;
/// 邊緣高光與內發光的強度（`crates/poni/src/glass.rs:693-694`）。
pub const RIM_STRENGTH: f32 = 0.80;
pub const RIM_FACING_MIN: f32 = 0.30;
pub const GLOW_STRENGTH: f32 = 0.09;
/// 兩層霜的模糊半徑，DIP（`crates/poni/src/glass.rs:29-30`）。
pub const FROST_LO: f32 = 3.0;
pub const FROST_HI: f32 = 9.0;
/// 顏色飽和度與色散（`crates/poni/src/glass.rs:699-700, 725`）。
pub const SATURATION: f32 = 1.45;
pub const DISPERSION: f32 = 0.03;
/// 深色玻璃的底色與描邊（`poni-core/src/look.rs:139, 142`）。
pub const GROUND_DARK: (u8, u8, u8) = (0x1C, 0x1C, 0x1E);
pub const BORDER_ALPHA: f32 = 0.10;

/// 玻璃厚度：0 = 收起的小藥丸，1 = 展開的面板。
pub fn thickness(height: f32) -> f32 {
    ((height - THIN_H) / (THICK_H - THIN_H)).clamp(0.0, 1.0)
}

/// 陰影強度（`poni-core/src/glass.rs:238-240`）。
pub fn shadow_alpha(busy: f32, thick: f32) -> f32 {
    (0.12 + 0.06 * thick + 1.1 * busy).clamp(0.12, 0.34)
}

/// WCAG 相對亮度（`poni-core/src/glass.rs:120-123`）。
pub fn rel_luminance(c: [f32; 3]) -> f32 {
    fn lin(v: f32) -> f32 {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG 對比度，1..21。
pub fn contrast(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (la, lb) = (rel_luminance(a), rel_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// 讓 `ink` 在「底色 `material` 混入 `toward`」上至少達到 `target` 對比度，
/// 取滿足條件的最小混合比例；範圍是 `min..=max`（`poni-core/src/glass.rs:139-157`）。
pub fn tint_for_contrast(
    material: [f32; 3],
    ink: [f32; 3],
    toward: [f32; 3],
    min: f32,
    max: f32,
    target: f32,
) -> f32 {
    let ok = |t: f32| contrast(mix3(material, toward, t), ink) >= target;
    if ok(min) {
        return min;
    }
    if !ok(max) {
        return max;
    }
    let (mut lo, mut hi) = (min, max);
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        if ok(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// 深色玻璃在給定背景上的著色量。沒有背景樣本時用 oyenza 的兜底值 `0.34`
/// （`crates/poni/src/glass.rs:678`）。
pub fn dark_tint(backdrop: Option<[f32; 3]>) -> f32 {
    match backdrop {
        Some(b) => tint_for_contrast(b, [1.0; 3], [0.0; 3], TINT_MIN, TINT_MAX, TEXT_CONTRAST),
        None => TINT_MIN,
    }
}

/// 邊緣高光的基礎強度：rim 0.80 × 背光面 0.30（`glass.hlsl:149`）。
pub fn rim_alpha() -> f32 {
    RIM_STRENGTH * RIM_FACING_MIN
}

/// 毛玻璃皮膚的著色量（疊在系統模糊之上的深色玻璃底，0..1）。
///
/// - 開啟系統模糊：背景已被抹平，按對比度剛好夠白字 4.8:1 即可，再留 0.03 餘量
///   應對取樣點之外的亮斑；最淺 0.20，深色桌面上儘量通透。
/// - 沒有模糊：背後細節仍然清楚，需要更深的煙燻（不低於 0.62）才壓得住紋理。
/// - 還沒有采樣結果時按中等亮度背景估計，寧可略深也不讓文字發虛。
pub fn glass_tint(backdrop: Option<[f32; 3]>, blurred: bool) -> f32 {
    // 按真實的玻璃底色（不是純黑）計算，白字剛好達到目標對比度。
    let ground = [GROUND_DARK.0 as f32 / 255.0, GROUND_DARK.1 as f32 / 255.0, GROUND_DARK.2 as f32 / 255.0];
    let need = tint_for_contrast(backdrop.unwrap_or([0.6; 3]), [1.0; 3], ground, 0.0, 0.95, TEXT_CONTRAST);
    if blurred {
        (need + 0.03).clamp(0.20, 0.95)
    } else {
        (need + 0.20).clamp(SMOKED_OPACITY - 0.20, 0.90)
    }
}

/// 淺色毛玻璃的著色量（疊在模糊之上的白色玻璃底，0..1），讓深色文字達到 4.8:1。
/// 深色背景對深色文字最不利，所以沒有采樣時按偏暗的背景估計。
pub fn light_tint(backdrop: Option<[f32; 3]>, blurred: bool) -> f32 {
    let ink = [22.0 / 255.0, 22.0 / 255.0, 26.0 / 255.0];
    let need = tint_for_contrast(backdrop.unwrap_or([0.35; 3]), ink, LIGHT_GROUND, 0.0, 0.95, TEXT_CONTRAST);
    if blurred { (need + 0.03).clamp(0.12, 0.95) } else { (need + 0.20).clamp(0.72, 0.96) }
}

/// 淺色玻璃的底色。
pub const LIGHT_GROUND: [f32; 3] = [248.0 / 255.0, 248.0 / 255.0, 250.0 / 255.0];

/// 把島體四周的若干螢幕取樣合成一個「背景色」：取平均色與最亮樣本的中點，
/// 偏向亮處，因為白字最怕的是亮背景。
pub fn backdrop_estimate(samples: &[[f32; 3]]) -> Option<[f32; 3]> {
    if samples.is_empty() {
        return None;
    }
    let n = samples.len() as f32;
    let mean = samples.iter().fold([0.0f32; 3], |a, c| [a[0] + c[0] / n, a[1] + c[1] / n, a[2] + c[2] / n]);
    let bright = samples
        .iter()
        .copied()
        .max_by(|a, b| rel_luminance(*a).total_cmp(&rel_luminance(*b)))?;
    Some(mix3(mean, bright, 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_tint_keeps_white_text_readable_on_any_backdrop() {
        for b in [[1.0; 3], [0.95, 0.9, 0.7], [0.5; 3], [0.05; 3]] {
            for blurred in [true, false] {
                let t = glass_tint(Some(b), blurred);
                let ground = [GROUND_DARK.0 as f32 / 255.0, GROUND_DARK.1 as f32 / 255.0, GROUND_DARK.2 as f32 / 255.0];
                assert!(contrast(mix3(b, ground, t), [1.0; 3]) >= 4.5, "{b:?} {blurred} {t}");
            }
        }
        // 深色背景更通透，淺色背景更深；無模糊始終比有模糊更深
        assert!(glass_tint(Some([0.05; 3]), true) < glass_tint(Some([1.0; 3]), true));
        assert!(glass_tint(Some([0.05; 3]), false) > glass_tint(Some([0.05; 3]), true));
        assert!(glass_tint(None, true) >= 0.20);
    }

    #[test]
    fn light_glass_keeps_dark_text_readable() {
        let ink = [22.0 / 255.0, 22.0 / 255.0, 26.0 / 255.0];
        for b in [[0.0; 3], [0.1, 0.2, 0.5], [0.5; 3], [1.0; 3]] {
            for blurred in [true, false] {
                let t = light_tint(Some(b), blurred);
                assert!(contrast(mix3(b, LIGHT_GROUND, t), ink) >= 4.5, "{b:?} {blurred} {t}");
            }
        }
        assert!(light_tint(Some([1.0; 3]), true) < light_tint(Some([0.0; 3]), true), "亮背景更通透");
    }

    #[test]
    fn backdrop_estimate_leans_toward_bright_samples() {
        assert_eq!(backdrop_estimate(&[]), None);
        let e = backdrop_estimate(&[[0.0; 3], [0.0; 3], [0.0; 3], [1.0; 3]]).unwrap();
        assert!((e[0] - 0.625).abs() < 1e-5, "{e:?}");
        assert_eq!(backdrop_estimate(&[[0.2, 0.4, 0.6]]), Some([0.2, 0.4, 0.6]));
    }

    #[test]
    fn contrast_matches_wcag() {
        assert!((contrast([1.0; 3], [0.0; 3]) - 21.0).abs() < 0.01);
        assert!((contrast([0.5; 3], [0.5; 3]) - 1.0).abs() < 1e-5);
        let g = 0x76 as f32 / 255.0;
        assert!((contrast([g; 3], [1.0; 3]) - 4.54).abs() < 0.02);
    }

    #[test]
    fn tint_stops_where_text_becomes_readable() {
        let orange = [0.98, 0.70, 0.45];
        let t = tint_for_contrast(orange, [1.0; 3], [0.0; 3], TINT_MIN, TINT_MAX, 4.5);
        assert!(t > TINT_MIN && t < TINT_MAX, "{t}");
        assert!(contrast(mix3(orange, [0.0; 3], t), [1.0; 3]) >= 4.5);
        assert!(contrast(mix3(orange, [0.0; 3], t - 0.02), [1.0; 3]) < 4.5);
        // 深色背景：最小著色就夠
        assert_eq!(tint_for_contrast([0.10, 0.10, 0.12], [1.0; 3], [0.0; 3], TINT_MIN, TINT_MAX, 4.5), TINT_MIN);
        // 白底：著色抬到剛夠 4.8:1，而不是無腦拉滿
        let white = dark_tint(Some([1.0; 3]));
        assert!(white > TINT_MIN && white < TINT_MAX, "{white}");
        assert!(contrast(mix3([1.0; 3], [0.0; 3], white), [1.0; 3]) >= TEXT_CONTRAST);
    }

    #[test]
    fn dark_tint_falls_back_to_oyenza_minimum() {
        assert_eq!(dark_tint(None), TINT_MIN);
        // 深色頁面按最小著色，淺色頁面抬到能讀
        assert!(dark_tint(Some([0.12, 0.12, 0.14])) < dark_tint(Some([0.9, 0.9, 0.9])));
    }

    #[test]
    fn busy_backdrops_get_deeper_shadows() {
        assert!(shadow_alpha(0.0, 0.0) < shadow_alpha(0.1, 0.0));
        assert!(shadow_alpha(0.0, 0.0) < shadow_alpha(0.0, 1.0));
        assert_eq!(shadow_alpha(5.0, 1.0), 0.34);
        assert!((shadow_alpha(0.0, 0.0) - 0.12).abs() < 1e-6);
    }

    #[test]
    fn thickness_and_rim_follow_oyenza() {
        assert_eq!(thickness(37.0), 0.0);
        assert_eq!(thickness(220.0), 1.0);
        assert!(thickness(140.0) > 0.5 && thickness(140.0) < 0.6);
        assert!((rim_alpha() - 0.24).abs() < 1e-6);
    }
}
