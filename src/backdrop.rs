//! 毛玻璃皮膚的背景亮度取樣。
//!
//! 在島體四周（視窗區域之外）取少量螢幕畫素，估計島背後的明暗，讓著色深淺剛好
//! 保證白字可讀。`GetPixel` 讀屏較慢，所以放在後臺執行緒裡每 1.5 秒採一次，
//! 渲染執行緒只讀結果；切回經典皮膚時執行緒隨 `Sampler` 一起退出。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};

#[derive(Default)]
struct Shared {
    points: Vec<(i32, i32)>,
    color: Option<[f32; 3]>,
}

pub struct Sampler {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
}

impl Sampler {
    pub fn start() -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (state, flag) = (shared.clone(), stop.clone());
        let _ = std::thread::Builder::new().name("daozai-backdrop".into()).spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                let points = state.lock().map(|s| s.points.clone()).unwrap_or_default();
                if !points.is_empty() {
                    let color = crate::glass::backdrop_estimate(&sample(&points));
                    if let (Some(color), Ok(mut s)) = (color, state.lock()) {
                        s.color = Some(color);
                    }
                }
                for _ in 0..15 {
                    if flag.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        Self { shared, stop }
    }

    /// 取樣點（螢幕物理座標）。
    pub fn set_points(&self, points: Vec<(i32, i32)>) {
        if let Ok(mut s) = self.shared.lock() {
            if s.points != points {
                s.points = points;
            }
        }
    }

    pub fn color(&self) -> Option<[f32; 3]> {
        self.shared.lock().ok().and_then(|s| s.color)
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn sample(points: &[(i32, i32)]) -> Vec<[f32; 3]> {
    unsafe {
        let dc = GetDC(None);
        if dc.0.is_null() {
            return Vec::new();
        }
        let out = points
            .iter()
            .filter_map(|&(x, y)| {
                let c = GetPixel(dc, x, y).0;
                // CLR_INVALID：點在螢幕外或被裁掉。
                (c != 0xFFFF_FFFF).then(|| [(c & 0xFF) as f32 / 255.0, ((c >> 8) & 0xFF) as f32 / 255.0, ((c >> 16) & 0xFF) as f32 / 255.0])
            })
            .collect();
        let _ = ReleaseDC(None, dc);
        out
    }
}
