//! 動效基礎件：彈簧、緩動與插值。
//!
//! 所有形變都用「帶速度的彈簧」驅動：目標隨時可改，速度保留，
//! 所以展開途中移開、收起途中回來都不會跳變，這是「絲滑」的關鍵。

/// 二階阻尼彈簧 `x'' = -k(x - target) - c·x'`，半隱式尤拉 + 子步長積分。
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub pos: f32,
    pub vel: f32,
    pub target: f32,
    k: f32,
    c: f32,
}

impl Spring {
    /// `stiffness` = k，`zeta` = 阻尼比（<1 有過沖，=1 臨界阻尼）。
    pub fn new(value: f32, stiffness: f32, zeta: f32) -> Self {
        let mut s = Self { pos: value, vel: 0.0, target: value, k: 0.0, c: 0.0 };
        s.tune(stiffness, zeta);
        s
    }

    pub fn tune(&mut self, stiffness: f32, zeta: f32) {
        self.k = stiffness;
        self.c = 2.0 * zeta * stiffness.sqrt();
    }

    pub fn snap(&mut self, value: f32) {
        self.pos = value;
        self.target = value;
        self.vel = 0.0;
    }

    pub fn step(&mut self, dt: f32) {
        // 掉幀時限幅，避免一幀跨太遠；子步 ≤ 1/480 s，保證在 30–240 Hz 下軌跡一致。
        let dt = dt.clamp(0.0, 0.1);
        let n = (dt * 480.0).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            let a = -self.k * (self.pos - self.target) - self.c * self.vel;
            self.vel += a * h;
            self.pos += self.vel * h;
        }
    }

    /// 相對 `scale` 足夠接近目標且幾乎靜止。
    pub fn settled(&self, scale: f32) -> bool {
        (self.pos - self.target).abs() < 0.002 * scale && self.vel.abs() < 0.02 * scale
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// 把 `x` 從 [a, b] 線性對映到 [0, 1] 並鉗制。
pub fn remap(x: f32, a: f32, b: f32) -> f32 {
    // 允許 a > b（反向區間，用於“越靠近邊緣越淡”這類漸隱）
    let d = b - a;
    if d.abs() < 1e-6 {
        return if x >= b { 1.0 } else { 0.0 };
    }
    clamp01((x - a) / d)
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = remap(x, a, b);
    t * t * (3.0 - 2.0 * t)
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - clamp01(t);
    1.0 - u * u * u
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = clamp01(t);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// 幀率無關的指數逼近：`rate` 越大越快（約 1/rate 秒到 63%）。
pub fn approach(cur: f32, target: f32, rate: f32, dt: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-rate * dt).exp())
}

/// 確定性的偽隨機（0..1），給吉祥物眨眼/張望用，錄屏可復現。
pub fn hash01(n: u64) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_supports_reversed_ranges() {
        assert_eq!(remap(200.0, 392.0, 358.0), 1.0);
        assert_eq!(remap(392.0, 392.0, 358.0), 0.0);
        assert!((smoothstep(392.0, 358.0, 375.0) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn underdamped_spring_overshoots_then_settles() {
        let mut s = Spring::new(0.0, 320.0, 0.7);
        s.target = 1.0;
        let mut peak: f32 = 0.0;
        for _ in 0..120 {
            s.step(1.0 / 60.0);
            peak = peak.max(s.pos);
        }
        assert!(peak > 1.02 && peak < 1.08, "peak {peak}");
        assert!(s.settled(1.0));
    }

    #[test]
    fn critically_damped_never_overshoots() {
        let mut s = Spring::new(1.0, 560.0, 1.0);
        s.target = 0.0;
        for _ in 0..60 {
            s.step(1.0 / 60.0);
            assert!(s.pos > -1e-4);
        }
    }

    #[test]
    fn frame_rate_independent() {
        let mut a = Spring::new(0.0, 300.0, 0.75);
        let mut b = a;
        a.target = 1.0;
        b.target = 1.0;
        for _ in 0..30 {
            a.step(1.0 / 30.0);
        }
        for _ in 0..144 {
            b.step(1.0 / 144.0);
        }
        assert!((a.pos - b.pos).abs() < 0.01);
    }
}
