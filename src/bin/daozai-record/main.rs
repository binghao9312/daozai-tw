//! 演示錄製器：不開視窗，指令碼驅動滑鼠/鍵盤/拖放，egui 輸出用軟體光柵化，
//! 逐幀把 RGBA 寫到 stdout（或直接拉起 ffmpeg）。
//!
//! 用法：`cargo run --release --features record --bin daozai-record -- out.mp4`
//! 需要 PATH 裡有 ffmpeg。畫面裡的歌曲、檔案、待辦都是演示用的假資料。

mod raster;

use eframe::egui::{self, Color32, Event, Pos2, Rect, Shape, Stroke, Vec2};
use daozai::app::{App, Cmd, Env, FrameOut, Options, WIN_H, WIN_W};
use daozai::lyrics;
use daozai::smtc::{NowPlaying, Transport};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CW: f32 = 800.0; // 畫布（邏輯畫素）
const CH: f32 = 250.0;
const PPP: f32 = 2.0;
const FPS: f64 = 60.0;
const WX: f32 = (CW - WIN_W) / 2.0; // 視窗在畫布中的位置

/// 面板座標（相對展開面板左上角）→ 畫布座標
fn p(x: f32, y: f32) -> Pos2 {
    Pos2::new(CW / 2.0 - daozai::app::PW / 2.0 + x, y)
}

#[derive(Clone)]
enum Act {
    /// 在 dur 秒內把滑鼠移到目標（緩動）
    Move(Pos2, f64),
    Wait(f64),
    Click,
    Type(&'static str),
    Enter,
    /// 開始/結束拖檔案
    DragStart(PathBuf),
    DragDrop,
}

struct Media {
    np: NowPlaying,
    anchor_t: f64,
    anchor_pos: f64,
    polled_at: f64,
}

impl Media {
    fn truth(&self, t: f64) -> f64 {
        if self.np.playing { (self.anchor_pos + t - self.anchor_t).min(self.np.duration) } else { self.anchor_pos }
    }
    fn tick(&mut self, t: f64) {
        // 模擬 SMTC 輪詢：每秒重新整理一次位置，中間由應用自己外推
        if t - self.polled_at >= 1.0 {
            self.polled_at = t;
            self.np.position = self.truth(t);
        }
    }
    fn apply(&mut self, cmd: Transport, t: f64) {
        let pos = self.truth(t);
        match cmd {
            Transport::Toggle => self.np.playing = !self.np.playing,
            Transport::Seek(s) => {
                self.anchor_pos = s;
                self.anchor_t = t;
                self.np.position = s;
                return;
            }
            _ => {}
        }
        self.anchor_pos = pos;
        self.anchor_t = t;
        self.np.position = pos;
    }
}

fn ease(x: f64) -> f64 {
    // 滑鼠軌跡：帶一點加減速，像人手
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 { 4.0 * x * x * x } else { 1.0 - (-2.0 * x + 2.0).powi(3) / 2.0 }
}

fn setup_data() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("daozai-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("data");
    let files = root.join("files");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(files.join("素材")).unwrap();
    let mk = |name: &str, len: u64| {
        let f = std::fs::File::create(files.join(name)).unwrap();
        f.set_len(len).unwrap();
        files.join(name)
    };
    let shelf = [
        mk("產品方案.pdf", 2_431_000),
        mk("設計稿 v3.png", 1_284_000),
        mk("週報.docx", 48_200),
        files.join("素材"),
    ];
    let drag = mk("會議紀要.md", 6_300);
    let body: String = shelf.iter().map(|p| format!("{}\n", p.display())).collect();
    std::fs::write(data.join("shelf.txt"), body).unwrap();
    std::fs::write(data.join("settings.txt"), "pet=1\nminimal=0\n").unwrap();
    std::fs::write(data.join("todo.txt"), "o|-|回覆設計評審意見\nx|2026-10-28|整理上週資料\n").unwrap();
    std::fs::write(data.join("water.txt"), "bottle=2000\ngoal=2000\nstart=2026-10-28\n").unwrap();
    let mut hours = [0u32; 24];
    hours[9] = 250;
    hours[11] = 300;
    hours[13] = 200;
    let hs: Vec<String> = hours.iter().map(|h| h.to_string()).collect();
    std::fs::write(data.join("water-extra.txt"), format!("drunk=750\nrefills=0\nremind=1\nhours={}\n", hs.join(","))).unwrap();
    std::env::set_var("DAOZAI_DATA_DIR", &data);
    (root, drag)
}

fn script(drag: PathBuf) -> Vec<Act> {
    use Act::*;
    let rest = Pos2::new(560.0, 215.0);
    vec![
        Move(rest, 0.0),
        Wait(1.2),
        // 1 懸停展開 → 音樂頁
        Move(Pos2::new(424.0, 25.0), 0.55),
        Wait(1.3),
        Move(p(210.5, 118.0), 0.45),
        Wait(0.25),
        Click, // 播放
        Wait(1.6),
        Move(p(240.0, 92.0), 0.4),
        Wait(0.2),
        Click, // 拖進度
        Wait(1.2),
        // 2 右箭頭 → 專注
        Move(p(398.0, 68.0), 0.45),
        Wait(0.2),
        Click,
        Wait(0.9),
        Move(p(267.0, 106.5), 0.4),
        Wait(0.2),
        Click, // 開始專注
        Wait(1.8),
        // 3 → 飲水
        Move(p(398.0, 68.0), 0.4),
        Wait(0.15),
        Click,
        Wait(0.9),
        Move(p(74.0, 110.5), 0.45),
        Wait(0.2),
        Click,
        Wait(0.8),
        Click,
        Wait(1.0),
        Move(p(300.0, 80.0), 0.45),
        Wait(0.2),
        Click, // 圓環 ↔ 柱狀圖
        Wait(1.8),
        // 4 → 置物架
        Move(p(398.0, 68.0), 0.45),
        Wait(0.15),
        Click,
        Wait(0.8),
        Move(p(110.0, 70.0), 0.5),
        Wait(0.5),
        Move(p(250.0, 72.0), 0.6),
        Wait(0.6),
        // 5 → 待辦
        Move(p(398.0, 68.0), 0.45),
        Wait(0.15),
        Click,
        Wait(0.9),
        Move(p(78.0, 109.0), 0.45),
        Wait(0.2),
        Click, // 加一個
        Wait(0.5),
        Type("寫"),
        Wait(0.12),
        Type("周"),
        Wait(0.12),
        Type("報"),
        Wait(0.5),
        Enter,
        Wait(0.9),
        Move(p(212.0, 46.0), 0.45),
        Wait(0.2),
        Click, // 勾掉
        Wait(1.4),
        // 6 頁碼豆回到音樂
        Move(p(186.0, 132.8), 0.5),
        Wait(0.2),
        Click,
        Wait(1.4),
        // 7 離開 → 收起
        Move(rest, 0.6),
        Wait(1.3),
        // 8 從桌面拖檔案進來
        Move(Pos2::new(640.0, 200.0), 0.3),
        DragStart(drag),
        Wait(0.3),
        Move(Pos2::new(430.0, 22.0), 0.8),
        Wait(0.5),
        Move(p(260.0, 80.0), 0.5),
        Wait(0.4),
        Move(p(180.0, 70.0), 0.5),
        Wait(0.4),
        DragDrop,
        Wait(1.6),
        Move(rest, 0.6),
        // 9 閒置 → 小精靈
        Wait(8.0),
        Move(Pos2::new(420.0, 24.0), 0.6),
        Wait(1.0),
        Move(rest, 0.6),
        Wait(1.4),
    ]
}

/// 預計算一張 Win11 風格的“光暈”桌布（線性→gamma 後直接存預乘值）。
fn wallpaper(w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut v = vec![[0.0; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let u = x as f32 / w as f32;
            let t = y as f32 / h as f32;
            let base = [0.035 + 0.05 * t, 0.075 + 0.07 * t, 0.17 + 0.12 * t];
            let mut c = base;
            let blob = |cx: f32, cy: f32, rx: f32, ry: f32| {
                let dx = (u - cx) / rx;
                let dy = (t - cy) / ry;
                (-(dx * dx + dy * dy)).exp()
            };
            let a = blob(0.5, 1.25, 0.32, 0.75);
            let b = blob(0.18, 1.1, 0.25, 0.5);
            let d = blob(0.85, 1.05, 0.22, 0.45);
            for i in 0..3 {
                let glow = [0.25, 0.55, 1.0][i] * a * 0.75 + [0.45, 0.35, 0.95][i] * b * 0.35 + [0.1, 0.6, 0.9][i] * d * 0.3;
                c[i] = (c[i] + glow).min(1.0);
            }
            // 輕微顆粒，避免色帶
            let n = (daozai::anim::hash01((x * 7919 + y * 104729) as u64) - 0.5) * 0.008;
            v[y * w + x] = [c[0] + n, c[1] + n, c[2] + n, 1.0];
        }
    }
    v
}

fn cursor_shapes(at: Pos2, press: f32, dragging: Option<f32>) -> Vec<Shape> {
    let s = 1.0 - 0.08 * press;
    let pts: Vec<Pos2> = [(0.0, 0.0), (0.0, 16.5), (3.9, 12.8), (6.6, 19.0), (9.0, 18.0), (6.4, 12.0), (11.6, 12.0)]
        .iter()
        .map(|(x, y)| at + Vec2::new(*x, *y) * s)
        .collect();
    let mut out = Vec::new();
    if let Some(a) = dragging {
        // 被拖著的檔案：半透明小卡片跟在指標右下
        let r = Rect::from_min_size(at + Vec2::new(10.0, 14.0), Vec2::new(30.0, 36.0));
        out.push(Shape::rect_filled(r.translate(Vec2::new(1.5, 2.5)), 5.0, Color32::from_black_alpha((60.0 * a) as u8)));
        out.push(Shape::rect_filled(r, 5.0, Color32::from_rgba_unmultiplied(245, 246, 250, (235.0 * a) as u8)));
        let band = Rect::from_min_size(r.min + Vec2::new(6.0, 23.0), Vec2::new(18.0, 6.5));
        out.push(Shape::rect_filled(band, 2.0, Color32::from_rgba_unmultiplied(90, 120, 255, (255.0 * a) as u8)));
        for i in 0..3 {
            let l = Rect::from_min_size(r.min + Vec2::new(6.0, 7.0 + i as f32 * 4.5), Vec2::new(if i == 2 { 11.0 } else { 18.0 }, 1.8));
            out.push(Shape::rect_filled(l, 1.0, Color32::from_rgba_unmultiplied(150, 152, 162, (255.0 * a) as u8)));
        }
    }
    // 投影
    let shadow: Vec<Pos2> = pts.iter().map(|q| *q + Vec2::new(0.8, 1.4)).collect();
    let tris = [(0, 1, 2), (0, 2, 5), (0, 5, 6), (2, 3, 4), (2, 4, 5)];
    let mesh = |pts: &[Pos2], col: Color32| {
        let mut m = egui::Mesh::default();
        for q in pts {
            m.colored_vertex(*q, col);
        }
        for (a, b, c) in tris {
            m.add_triangle(a, b, c);
        }
        Shape::mesh(m)
    };
    out.push(mesh(&shadow, Color32::from_black_alpha(70)));
    out.push(mesh(&pts, Color32::WHITE));
    out.push(Shape::closed_line(pts, Stroke::new(1.0_f32, Color32::from_gray(12))));
    out
}

fn main() {
    if std::env::args().any(|a|a=="--gallery"){gallery();return;}
    let out_path = std::env::args().nth(1).unwrap_or_else(|| "daozai-demo.mp4".into());
    let (root, drag_file) = setup_data();

    // DAOZAI_FAST=1：只跑指令碼不出畫面，除錯點選位置用
    let fast = std::env::var("DAOZAI_FAST").is_ok();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, Options { fetch_lyrics: false, pet_idle: 4.0 });

    let title = "晚風與海";
    let artist = "示例樂隊";
    let dur = 214.0;
    let lrc = "[00:58.00]把今天折成紙船\n[01:03.50]放進晚風裡慢慢走\n[01:08.80]海面亮起一盞燈\n[01:13.60]像誰在遠處等候\n[01:19.00]我們不必急著靠岸\n[01:24.40]月光會替我回頭\n[01:30.00]把心事交給潮汐\n[01:35.50]明天也會溫柔\n[01:41.00]晚風吹過 你的名字\n[01:46.50]輕輕落在 海的盡頭\n[01:52.00]每一朵浪花 都在說\n[01:57.50]別害怕 慢慢走\n[02:03.00]我們不必急著靠岸\n[02:08.40]月光會替我回頭\n[02:14.00]風停在你的視窗\n[02:19.60]燈火一盞一盞亮透\n[02:25.20]我們沿著海岸線\n[02:30.80]把夏天慢慢走舊\n[02:36.40]晚風吹過 你的名字\n[02:42.00]輕輕落在 海的盡頭\n[02:47.60]每一朵浪花 都在說\n[02:53.20]別害怕 慢慢走\n[03:00.00]（間奏）\n[03:20.00]把今天折成紙船\n";
    app.set_lyrics(title, artist, dur, lyrics::parse_lrc(lrc));
    let mut media = Media {
        np: NowPlaying { title: title.into(), artist: artist.into(), playing: false, position: 63.0, duration: dur, ..Default::default() },
        anchor_t: 0.0,
        anchor_pos: 63.0,
        polled_at: 0.0,
    };

    let (pw, ph) = ((CW * PPP) as usize, (CH * PPP) as usize);
    let mut ras = raster::Raster::new(pw, ph);
    let wall = wallpaper(pw, ph);
    let mut ff = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{pw}x{ph}"), "-r", "60", "-i", "-"])
        .args(["-c:v", "libx264", "-preset", "slow", "-crf", "15", "-pix_fmt", "yuv420p", "-movflags", "+faststart", &out_path])
        .stdin(Stdio::piped())
        .spawn()
        .expect("需要 ffmpeg");
    let mut pipe = ff.stdin.take().unwrap();
    let mut rgba = Vec::new();

    // 展開指令碼為逐幀事件
    let acts = script(drag_file);
    let win = Rect::from_min_size(Pos2::new(WX, 0.0), Vec2::new(WIN_W, WIN_H));
    let mut cursor = Pos2::new(560.0, 215.0);
    let mut region = Rect::NOTHING;
    let mut t = 0.0f64;
    let mut frame_no = 0u64;
    let mut was_inside = false;
    let mut press_t = -10.0f64;
    let mut ripples: Vec<(Pos2, f64)> = Vec::new();
    let mut dragging: Option<PathBuf> = None;
    let mut drag_a = 0.0f32;
    let mut first = true;

    let mut queue: std::collections::VecDeque<Act> = acts.into();
    let mut cur: Option<(Act, f64, Pos2)> = None; // (動作, 開始時間, 起點)
    let mut pending_release = false;

    loop {
        let mut events = Vec::new();
        let mut hovered = Vec::new();
        let mut dropped = Vec::new();
        // ---- 推進指令碼 ----
        let mut done = false;
        loop {
            if cur.is_none() {
                match queue.pop_front() {
                    Some(a) => cur = Some((a, t, cursor)),
                    None => {
                        done = true;
                        break;
                    }
                }
            }
            let (a, t0, from) = cur.clone().unwrap();
            match a {
                Act::Move(to, d) => {
                    let k = if d <= 0.0 { 1.0 } else { ease((t - t0) / d) };
                    cursor = from + (to - from) * k as f32;
                    // 輕微弧線
                    let arc = (k * std::f64::consts::PI).sin() as f32 * (to - from).length() * 0.04;
                    cursor.y -= arc;
                    if k >= 1.0 {
                        cursor = to;
                        cur = None;
                        continue;
                    }
                }
                Act::Wait(d) => {
                    if t - t0 >= d {
                        cur = None;
                        continue;
                    }
                }
                Act::Click => {
                    events.push(Event::PointerButton { pos: cursor, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
                    press_t = t;
                    ripples.push((cursor, t));
                    pending_release = true;
                    cur = None;
                }
                Act::Type(s) => {
                    events.push(Event::Text(s.to_string()));
                    cur = None;
                    continue;
                }
                Act::Enter => {
                    events.push(Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
                    cur = None;
                    continue;
                }
                Act::DragStart(f) => {
                    dragging = Some(f);
                    cur = None;
                    continue;
                }
                Act::DragDrop => {
                    if let Some(f) = dragging.take() {
                        dropped.push(egui::DroppedFile { path: Some(f), ..Default::default() });
                    }
                    cur = None;
                    continue;
                }
            }
            break;
        }
        if done {
            break;
        }
        if pending_release && t - press_t > 0.09 {
            events.push(Event::PointerButton { pos: cursor, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
            pending_release = false;
        }
        drag_a = daozai::anim::approach(drag_a, if dragging.is_some() { 1.0 } else { 0.0 }, 14.0, 1.0 / 60.0);

        // 滑鼠只有在視窗的可互動區域裡才會被應用“看見”（與真視窗的區域裁剪一致）
        let inside = region.contains(cursor) && !first;
        if inside {
            events.insert(0, Event::PointerMoved(cursor));
        } else if was_inside {
            events.insert(0, Event::PointerGone);
        }
        if !inside {
            events.retain(|e| !matches!(e, Event::PointerButton { .. }));
        }
        was_inside = inside;
        if let Some(f) = &dragging {
            if inside {
                hovered.push(egui::HoveredFile { path: Some(f.clone()), ..Default::default() });
            }
        }
        if !inside {
            dropped.clear();
        }

        media.tick(t);
        let total = 14 * 3600 + 20 * 60 + t as u64;
        let env = Env {
            t,
            clock: format!("{:02}:{:02}", total / 3600, total / 60 % 60),
            today: "2026-10-28".into(),
            date: "10月28日 週三".into(),
            battery: Some(76),
            charging: true,
            network: None,
            cursor: Some(cursor),
            idle_seconds:None,
            screen_locked:false,
            fullscreen_hidden:false,
            browser_at_top:None,
            media: media.np.clone(),
            volume: Some((62, false)),
            bt: Vec::new(),
            bt_ready: true,
            screen: None,
            backdrop: None,
            glass_blur: false,
            opaque_surface: false,
        };

        let mut vp = egui::ViewportInfo::default();
        vp.native_pixels_per_point = Some(PPP);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(CW, CH))),
            time: Some(t),
            predicted_dt: 1.0 / 60.0,
            events,
            hovered_files: hovered,
            dropped_files: dropped,
            focused: true,
            viewports: std::iter::once((egui::ViewportId::ROOT, vp)).collect(),
            ..Default::default()
        };
        let dbg_click = raw.events.iter().any(|e| matches!(e, Event::PointerButton { .. }));
        let mut fo: Option<FrameOut> = None;
        let output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                fo = Some(app.frame(ui, win, &env));
            });
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Debug, egui::Id::new("demo-cursor")));
            ripples.retain(|(_, t0)| t - t0 < 0.45);
            for (at, t0) in &ripples {
                let k = ((t - t0) / 0.45) as f32;
                let r = 5.0 + 13.0 * daozai::anim::ease_out_cubic(k);
                painter.circle_stroke(*at, r, Stroke::new(1.6_f32, Color32::from_white_alpha((150.0 * (1.0 - k)) as u8)));
            }
            let press = (1.0 - ((t - press_t) / 0.18) as f32).clamp(0.0, 1.0);
            painter.extend(cursor_shapes(cursor, press, if drag_a > 0.01 { Some(drag_a) } else { None }));
        });
        let fo = fo.unwrap();
        if std::env::var("DAOZAI_DEBUG").is_ok() && frame_no % 60 == 0 {
            eprintln!("  t={t:.1} idle={:.1} p={:.2} cur={cursor:?}", app.idle_for(), app.progress());
        }
        if dbg_click && std::env::var("DAOZAI_DEBUG").is_ok() {
            eprintln!("t={t:.2} cur={cursor:?} page={} region={:?} hit={:?}", app.page(), fo.region, fo.hit);
        }
        region = fo.region.translate(Vec2::new(WX, 0.0)); // FrameOut 是視窗區域性座標
        for c in fo.cmds {
            if let Cmd::Media(m) = c {
                media.apply(m, t);
            }
        }
        first = false;

        if fast {
            frame_no += 1;
            t = frame_no as f64 / FPS;
            continue;
        }
        ras.update_textures(&output.textures_delta);
        let prims = ctx.tessellate(output.shapes, output.pixels_per_point);
        ras.buf.copy_from_slice(&wall);
        ras.draw(&prims, output.pixels_per_point);
        ras.free_textures(&output.textures_delta);
        ras.to_rgba8(&mut rgba);
        if pipe.write_all(&rgba).is_err() {
            break;
        }
        if let Ok(dir) = std::env::var("DAOZAI_DUMP") {
            if let Ok(every) = std::env::var("DAOZAI_DUMP_EVERY") {
                let n: u64 = every.parse().unwrap_or(30);
                if frame_no % n == 0 {
                    let _ = std::fs::write(format!("{dir}/f_{frame_no:05}.rgba"), &rgba);
                }
            }
        }
        frame_no += 1;
        t = frame_no as f64 / FPS;
    }
    drop(pipe);
    let _ = ff.wait();
    let _ = std::fs::remove_dir_all(root);
    eprintln!("{frame_no} 幀 → {out_path}");
}

/// 將實際渲染的六個頁面、休息狀態與提醒匯出，供迴歸檢查。
fn gallery() {
    let (root,_)=setup_data();
    let dest=std::env::args().nth(2).unwrap_or_else(||"preview".into());
    std::fs::create_dir_all(&dest).unwrap();
    for scene in 0..23 {
        let ctx=egui::Context::default();
        let mut app=App::new(&ctx,Options{fetch_lyrics:false,pet_idle:45.0});
        app.settings.pet=true;
        let mut ras=raster::Raster::new(1280,380);
        let mut rgba=vec![];
        for frame in 0..75 {
            let t=frame as f64/60.0;
            if scene<5 {app.show_page(scene);} if (11..14).contains(&scene){app.open_settings_category(scene-10);}
            if scene==9 && frame==0 {app.alert("喝水時間到了","休息一下，喝 250 ml 水吧",2,false);}
            if scene==14 && frame==0 {app.alert("專注計時","該休息了",1,false);}
            if scene==15 && frame==0 {app.alert("系統音量","音量 67%",5,false);}
            if scene==16 {app.settings.minimal=true;}
            if scene==17 && frame==0 {app.receive_notification("聊天應用".into(),"whatsapp.preview".into(),"你好，下午見".into());}
            if scene==18 && frame>0 {app.show_sit_view(42.0);}
            if scene==19 && frame==0 {app.alert(daozai::app::SIT_TITLE,"已連續坐了 60 分鐘",2,false);}
            if scene==20 && frame==1 {app.show_sit_view(42.0);app.open_settings_category(1);}
            if scene==21 && frame>0 {app.show_sit_view(65.0);}
            if scene==22 && frame==1 {app.open_settings_category(1);app.show_health_time();}
            let idle=if scene==7{60.0}else if scene==8{200.0}else{0.0};
            let env=Env{t,clock:"10:28:00".into(),today:"2026-10-28".into(),date:"10月28日 週三".into(),battery:Some(76),charging:true,cursor:if scene<6{Some(Pos2::new(320.0,60.0))}else{None},idle_seconds:Some(idle),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,190.0))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(23,28,45))).show(ctx,|ui|{let win=ui.max_rect();app.frame(ui,win,&env);});});
            ras.update_textures(&output.textures_delta);
            if frame==74 {
                ras.buf.fill([0.09,0.11,0.18,1.0]);
                ras.draw(&ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                ras.to_rgba8(&mut rgba);
            }
            ras.free_textures(&output.textures_delta);
        }
        let path=format!("{dest}/scene-{scene}.png");
        let mut ff=Command::new("ffmpeg").args(["-y","-loglevel","error","-f","rawvideo","-pix_fmt","rgba","-s","1280x380","-i","-","-frames:v","1",&path]).stdin(Stdio::piped()).spawn().unwrap();
        ff.stdin.take().unwrap().write_all(&rgba).unwrap();assert!(ff.wait().unwrap().success());
    }
    // 僅移除本次建立的獨立演示目錄。
    let _=std::fs::remove_dir_all(root);
}
