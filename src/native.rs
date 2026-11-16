//! Windows 的非啟用視窗、形狀區域與命中測試。
//!
//! 對齊原版實測（`BUILD-ORDER-v1.md` §1/§5）：
//! - `WS_POPUP` + `WS_EX_TOPMOST|TOOLWINDOW|NOREDIRECTIONBITMAP|NOACTIVATE`；
//! - `SetWindowRgn` 用三種矩形：收起 179×34@(230,0)、展開 512×186@(64,0)、
//!   極簡收起 112×14@(264,0)，座標是 640×190 邏輯點；
//! - 命中區與 region 分開：收起 143×22@(253,5)、展開 416×132@(110,5)，
//!   命中區外 `HTTRANSPARENT`，真實點選落給下層視窗；
//! - 前臺視窗切走（`EVENT_SYSTEM_FOREGROUND`，且不是本程序）時通知主執行緒收起；
//! - 原版沒有可用的 Esc 收起入口（§2.5），這裡也不註冊鍵盤鉤子。

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use crate::poll_clock::PollClock;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CombineRgn, CreatePolygonRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HRGN, RGN_OR, WINDING};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState,VK_LBUTTON,VK_RBUTTON};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

const SUBCLASS_ID: usize = 0x4E55_5341; // "DAOZAI"
#[allow(dead_code)]
const WS_EX_NOREDIRECTIONBITMAP: isize = 0x0020_0000;

/// 命中區（物理畫素，視窗座標，left/top/right/bottom）。
static HIT: OnceLock<Mutex<Vec<(i32, i32, i32, i32)>>> = OnceLock::new();
/// 前臺視窗切到別的程序時置位，由主執行緒消費。
static FG_CHANGED: AtomicBool = AtomicBool::new(false);

// `WS_EX_NOREDIRECTIONBITMAP` 只能在 CreateWindowEx 時設定（執行時 SetWindowLongPtrW
// 會回 ERROR_INVALID_PARAMETER），本專案用 GL 合成視窗，加上這一位後畫面上什麼都
// 看不到，因此不使用；具體見 `apply_style` 註釋與交付說明。

fn hit() -> &'static Mutex<Vec<(i32, i32, i32, i32)>> {
    HIT.get_or_init(|| Mutex::new(Vec::new()))
}

fn inside_hit(x: i32, y: i32) -> bool {
    hit().lock().unwrap_or_else(|e|e.into_inner()).iter().any(|&(l,t,r,b)|x>=l&&x<r&&y>=t&&y<b)
}

fn file_drag_region(region:(i32,i32,i32,i32),zone:Option<(i32,i32,i32,i32)>,capture:bool)->(i32,i32,i32,i32){
    match zone.filter(|_|capture) {
        Some(zone)=>(region.0.min(zone.0),region.1.min(zone.1),region.2.max(zone.2),region.3.max(zone.3)),
        None=>region,
    }
}

#[derive(Default)]
struct DragCapture {
    origin: Option<(i32,i32)>,
    moved: bool,
}

impl DragCapture {
    fn update(&mut self,down:bool,cursor:Option<(i32,i32)>,zone:Option<(i32,i32,i32,i32)>,threshold:(i32,i32))->bool {
        let Some(cursor)=cursor.filter(|_|down && zone.is_some()) else {
            *self=Self::default();
            return false;
        };
        let inside=|(x,y):(i32,i32)|zone.is_some_and(|(l,t,r,b)|x>=l && x<r && y>=t && y<b);
        let origin=*self.origin.get_or_insert(cursor);
        self.moved|=(cursor.0-origin.0).abs()>threshold.0 || (cursor.1-origin.1).abs()>threshold.1;
        // 只接收從感應區外拖進來的檔案：在島下方的標籤欄裡按下（含連續點選、拖標籤、選文字）
        // 不能擴大視窗區域，否則 HTTRANSPARENT 攔不住跨程序點選，下層視窗會點不動。
        self.moved && !inside(origin) && inside(cursor)
    }
}

/// 拖檔案進島：拖放感應區只在每幀的 `update` 裡檢查，而島靜止時（沒有吉祥物動畫、
/// 沒有播放音樂，例如側邊停靠）一秒才畫一幀，拖到島上往往來不及擴大接收區域。
/// 這裡用一個幾乎不佔資源的小執行緒：每 80 ms 看一次滑鼠鍵，只有按住且游標在島的畫布內時
/// 才請求重繪，讓感應區及時生效；平時不觸發任何繪製。
fn watch_drag(hwnd:HWND,ctx:eframe::egui::Context){
    let raw=hwnd.0 as isize;
    let mut was_down=false;
    let _=std::thread::Builder::new().name("daozai-drag-watch".into()).spawn(move||loop{
        std::thread::sleep(std::time::Duration::from_millis(80));
        unsafe {
            let hwnd=HWND(raw as *mut _);
            if !IsWindow(hwnd).as_bool(){return;}
            let down=GetAsyncKeyState(VK_LBUTTON.0 as i32)<0 || GetAsyncKeyState(VK_RBUTTON.0 as i32)<0;
            // 鬆開後補一幀，讓拖放狀態及時復位；否則靜止時一秒才畫一幀，連續點選會沿用上次的按下位置。
            if was_down && !down {ctx.request_repaint();}
            was_down=down;
            if !down {continue;}
            let mut cursor=POINT::default();let mut rect=RECT::default();
            if GetCursorPos(&mut cursor).is_err() || GetWindowRect(hwnd,&mut rect).is_err(){continue;}
            if cursor.x>=rect.left && cursor.x<rect.right && cursor.y>=rect.top && cursor.y<rect.bottom {ctx.request_repaint();}
        }
    });
}

// 圖形上下文和原生樣式就緒後重新啟用逐畫素合成；不能只依賴建立 HWND 時的請求。
fn enable_alpha_composition(hwnd:HWND)->windows::core::Result<()> {
    use windows::Win32::Graphics::Dwm::{DwmEnableBlurBehindWindow,DWM_BLURBEHIND,DWM_BB_ENABLE,DWM_BB_BLURREGION};
    unsafe {
        let region=CreateRectRgn(0,0,-1,-1);
        if region.0.is_null(){return Err(windows::core::Error::from_win32());}
        let blur=DWM_BLURBEHIND{dwFlags:DWM_BB_ENABLE|DWM_BB_BLURREGION,fEnable:true.into(),hRgnBlur:region,..Default::default()};
        let result=DwmEnableBlurBehindWindow(hwnd,&blur);
        let _=DeleteObject(region);
        result
    }
}

// ---------- 毛玻璃：視窗區域 ----------
//
// 相容後端（無逐畫素透明）按輪廓裁剪視窗區域；毛玻璃的實時模糊見 `composition.rs`。

/// Windows「透明效果」開關；讀不到時返回 None。
fn system_transparency() -> Option<bool> {
    use windows::{core::w, Win32::System::Registry::*};
    let mut value = 1u32; let mut size = 4u32;
    let ok = unsafe { RegGetValueW(HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"), w!("EnableTransparency"), RRF_RT_REG_DWORD, None, Some((&mut value as *mut u32).cast()), Some(&mut size)) }.is_ok();
    ok.then_some(value != 0)
}

/// 是否使用合成引擎的實時模糊。系統「透明效果」關閉時宿主背景畫刷只會畫出一塊純色，
/// 卻仍要合成器為這扇視窗持續工作；部分核顯舊驅動在這種狀態下會整機閃屏卡死，因此直接不用，
/// 由島體自己畫半透明底色。登錄檔每 2 秒最多讀一次；狀態變化時寫一條診斷。
fn compositor_blur_allowed() -> bool {
    use std::cell::Cell;
    thread_local! { static CACHE: Cell<(Option<std::time::Instant>, Option<bool>)> = const { Cell::new((None, None)) }; }
    CACHE.with(|cache| {
        let (at, last) = cache.get();
        if let (Some(at), Some(last)) = (at, last) { if at.elapsed() < Duration::from_secs(2) { return last; } }
        let allowed = system_transparency() != Some(false);
        if last.is_some_and(|last| last != allowed) || last.is_none() && !allowed {
            glass_diagnostic(if allowed { "系統透明效果已開啟，恢復合成引擎模糊" } else { "系統透明效果關閉，不使用合成引擎模糊" });
        }
        cache.set((Some(std::time::Instant::now()), Some(allowed)));
        allowed
    })
}

/// 毛玻璃診斷：寫到資料目錄的 glass-diagnostics.txt（只記錄狀態變化，不會持續增長）。
fn glass_diagnostic(message: &str) {
    use std::io::Write;
    let path = crate::model::config_dir().join("glass-diagnostics.txt");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) { let _ = std::fs::write(&path, b""); }
    let transparency = {
        match system_transparency() { Some(true) => "開", Some(false) => "關", None => "未知" }
    };
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{} · {} · 系統透明效果：{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), message, transparency);
    }
}

/// 把邏輯座標的輪廓換算成物理畫素，作為去重的鍵和區域的輸入。
fn glass_key(shapes: &[crate::app::GlassShape], ppp: f32) -> Vec<i32> {
    use crate::app::GlassShape;
    let px = |v: f32| (v * ppp).round() as i32;
    let mut key = Vec::new();
    for shape in shapes {
        match shape {
            GlassShape::Outline(_) | GlassShape::Notch { .. } | GlassShape::Docked { .. } => {
                let points = shape.outline();
                key.push(-1);
                key.push(points.len() as i32);
                for p in &points { key.push(px(p.x)); key.push(px(p.y)); }
            }
            GlassShape::Round(r, radius) => {
                key.extend([-2, px(r.left()), px(r.top()), px(r.right()), px(r.bottom()), px(*radius * 2.0)]);
            }
        }
    }
    key
}

/// 按 `glass_key` 的結果合成視窗區域（多塊輪廓取並集）。
unsafe fn glass_region(key: &[i32]) -> Option<HRGN> {
    let mut out: Option<HRGN> = None;
    let mut i = 0;
    while i < key.len() {
        let part = match key[i] {
            -1 => {
                let n = key[i + 1] as usize;
                let points: Vec<POINT> = (0..n).map(|j| POINT { x: key[i + 2 + j * 2], y: key[i + 3 + j * 2] }).collect();
                i += 2 + n * 2;
                if points.len() < 3 { continue; }
                CreatePolygonRgn(&points, WINDING)
            }
            _ => {
                let (l, t, r, b, d) = (key[i + 1], key[i + 2], key[i + 3], key[i + 4], key[i + 5]);
                i += 6;
                CreateRoundRectRgn(l, t, r + 1, b + 1, d, d)
            }
        };
        if part.0.is_null() { continue; }
        match out {
            None => out = Some(part),
            Some(all) => {
                let _ = CombineRgn(all, all, part, RGN_OR);
                let _ = DeleteObject(part);
            }
        }
    }
    out
}

/// 命中區外 `HTTRANSPARENT`（滑鼠投給下層視窗），命中區內 `HTCLIENT`。
unsafe extern "system" fn subclass(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if matches!(msg,WM_DWMCOMPOSITIONCHANGED|WM_DISPLAYCHANGE|WM_DWMCOLORIZATIONCOLORCHANGED)
        || (msg==WM_SHOWWINDOW && wp.0!=0) {
        // 隱藏建立到首次顯示、隱藏後恢復時也要恢復透明；等框架處理完視窗狀態再應用。
        let result=DefSubclassProc(hwnd,msg,wp,lp);
        let _=enable_alpha_composition(hwnd);
        return result;
    }
    // 保留框架的焦點通知；lParam=-1 禁止系統在啟用時重畫透明視窗的頂邊。
    if msg==WM_NCACTIVATE {return DefSubclassProc(hwnd,msg,wp,LPARAM(-1));}
    if msg==WM_NCPAINT {return LRESULT(0);}
    if msg == WM_MOUSEACTIVATE
        && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_NOACTIVATE.0 as isize != 0
    {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if msg == WM_NCHITTEST {
        let x = (lp.0 & 0xffff) as u16 as i16 as i32;
        let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as i32;
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            let px = x - rect.left;
            let py = y - rect.top;
            return if inside_hit(px, py) {
                LRESULT(HTCLIENT as isize)
            } else {
                LRESULT(HTTRANSPARENT as isize)
            };
        }
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if event != EVENT_SYSTEM_FOREGROUND || hwnd.0.is_null() {
        return;
    }
    let mut pid = 0u32;
    let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    // 自己（含系統匣視窗）拿到前臺不算「切走」
    if pid != 0 && pid != GetCurrentProcessId() {
        FG_CHANGED.store(true, Ordering::Relaxed);
    }
}

/// 主執行緒每幀呼叫一次：這一幀是否發生了「前臺切到別的程式」。
pub fn take_foreground_change() -> bool {
    FG_CHANGED.swap(false, Ordering::Relaxed)
}

/// winit 會在顯示/改屬性時重寫視窗樣式，所以每幀按需兜一次底。
unsafe fn apply_style(hwnd: HWND, editing: bool, raise: bool) {
    let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    let want = (style | WS_POPUP.0 as isize)
        & !((WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0)
            as isize);
    let mut frame_changed = false;
    if want != style {
        SetWindowLongPtrW(hwnd, GWL_STYLE, want);
        frame_changed = true;
    }
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    // 凍結單要求 0x08200088（含 WS_EX_NOREDIRECTIONBITMAP）。該位只能在
    // CreateWindowEx 時設定，執行時 SetWindowLongPtrW 會回 ERROR_INVALID_PARAMETER；
    // 而給 GL/視窗渲染的 HWND 加上它之後畫面完全不被合成（實測可見畫素為 0）。
    // 因此這裡只保留可用的 TOPMOST|TOOLWINDOW|NOACTIVATE，去掉會露在工作列的
    // APPWINDOW；ACCEPTFILES（winit 預設）也去掉，拖放走的是 OLE RegisterDragDrop，
    // 不依賴 WM_DROPFILES，這樣 ex style 就是 0x08000088。
    // 置頂由 SetWindowPos 管理，避免樣式標記與實際視窗層級脫節。
    let mut want_ex = (ex | WS_EX_TOOLWINDOW.0 as isize)
        & !(WS_EX_APPWINDOW.0 as isize)
        & !(WS_EX_ACCEPTFILES.0 as isize);
    if editing {
        want_ex &= !(WS_EX_NOACTIVATE.0 as isize);
    } else {
        want_ex |= WS_EX_NOACTIVATE.0 as isize;
    }
    if want_ex != ex {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want_ex);
    }
    let topmost = ex & WS_EX_TOPMOST.0 as isize != 0;
    if frame_changed || !topmost || raise {
        // 只有真的 SetWindowPos(HWND_TOPMOST) 才會讓 TOPMOST 位生效
        // 每次展開重新抬高實際層級；標記存在時仍可能被別的視窗蓋住。
        let mut flags=SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        // 輸入狀態只切換啟用許可權，避免重畫無邊框視窗的非客戶區。
        if frame_changed {flags|=SWP_FRAMECHANGED;}
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            flags,
        );
    }
}

/// 這些渲染路徑交給系統的畫面可能沒有可用的透明通道，透明畫布會顯示成黑框（#72：AMD RX 6750 GRE 等）。
/// 但同為 AMD，也有不少機器逐畫素透明完全正常（#152 RX 5600、#114 AMD 核顯）。所以這裡只決定
/// 「不直接信任系統合成」：畫面改由分層視窗逐畫素顯示（見 `present_frame`），兩類機器都不會有黑框，
/// 邊緣也保持抗鋸齒；不再像 v0.6.63–0.6.64 那樣直接用二值的視窗區域硬裁剪（圓角、肩部全是臺階）。
fn surface_needs_outline_clip(backend:Option<eframe::wgpu::Backend>,vendor:&str,renderer:&str)->bool {
    use eframe::wgpu::Backend;
    if matches!(backend,Some(Backend::Dx12|Backend::Vulkan)){return true;}
    if renderer.to_ascii_lowercase().contains("radeon"){return true;}
    [vendor,renderer].into_iter().any(|name|{
        let name=name.to_ascii_lowercase();
        name.contains("advanced micro devices") || name.split(|c:char|!c.is_ascii_alphanumeric()).any(|word|matches!(word,"amd"|"ati"))
    })
}

/// 只識別同屏頂部的寬條視窗，普通彈窗、全屏視窗和其他螢幕不觸發抬高。
unsafe fn top_bar_above(hwnd:HWND,bounds:crate::display_layout::Bounds)->bool {
    let mut island=RECT::default();
    if !IsWindowVisible(hwnd).as_bool() || GetWindowRect(hwnd,&mut island).is_err(){return false;}
    let mut current=hwnd;
    while let Ok(previous)=GetWindow(current,GW_HWNDPREV) {
        current=previous;
        if !IsWindowVisible(current).as_bool() || GetWindowLongPtrW(current,GWL_EXSTYLE)&WS_EX_TOPMOST.0 as isize==0 {continue;}
        let mut bar=RECT::default();
        if GetWindowRect(current,&mut bar).is_ok()
            && bar.top<=bounds.top && bar.bottom>bounds.top
            && bar.bottom-bar.top<=(bounds.bottom-bounds.top)/8
            && bar.right-bar.left>=(bounds.right-bounds.left)/2
            && bar.left<island.right && bar.right>island.left
            && bar.bottom>island.top {
            return true;
        }
    }
    false
}

/// 相容後端的分層視窗狀態：先按輪廓裁剪，讀回到帶透明的畫面後再切到分層視窗。
#[derive(Default)]
enum Layering {
    /// 不需要（透明合成正常）或還沒收到第一幀。
    #[default]
    Pending,
    Active(crate::layered::Layered),
    /// 讀不到 alpha 或系統拒絕：一直用輪廓裁剪。
    Unavailable,
}

/// 本機驗證分層顯示的開關：None 不啟用；Some(false) 按需要分層的顯示卡處理；Some(true) 另外假裝讀回的畫面沒有透明通道。
fn outline_test()->Option<bool> {
    static VALUE:std::sync::OnceLock<Option<bool>>=std::sync::OnceLock::new();
    *VALUE.get_or_init(||match std::env::var("DAOZAI_OUTLINE_TEST").ok()?.trim().to_ascii_lowercase().as_str() {
        "alpha"=>Some(false),
        "opaque"=>Some(true),
        _=>None,
    })
}

pub struct Window {
    hwnd: HWND,
    layering: Layering,
    region: (i32, i32, i32, i32),
    expanded: bool,
    hook: HWINEVENTHOOK,
    browser_state: (Option<bool>,PollClock),
    drag_capture: DragCapture,
    /// 當前生效的貼合輪廓（物理畫素鍵）、毛玻璃的合成模糊層與其狀態。
    glass: Option<Vec<i32>>,
    backdrop: Option<crate::composition::Backdrop>,
    blur: bool,
    blur_unavailable: bool,
    opaque_surface: bool,
    /// 讀回畫面沒有透明通道時，用來補算邊緣 alpha 的島體輪廓（物理畫素）。
    outline: ShapeAlpha,
}

/// 按島體輪廓補出來的 alpha：輪廓每幀隨 `update` 更新，蒙版只在輪廓或畫面尺寸變化時重算。
#[derive(Default)]
struct ShapeAlpha {
    /// 讀回的畫面確實沒有透明通道，正在用輪廓補 alpha。
    active: bool,
    key: Vec<i32>,
    polygons: Vec<Vec<(f32, f32)>>,
    mask: Option<([usize; 2], Vec<i32>, Vec<u8>)>,
    frame: Vec<u8>,
}

impl ShapeAlpha {
    fn set_shapes(&mut self, shapes: &[crate::app::GlassShape], ppp: f32) {
        let key = glass_key(shapes, ppp);
        if key == self.key { return; }
        self.polygons = shapes.iter().map(|shape| shape.outline().iter().map(|p| (p.x * ppp, p.y * ppp)).collect()).collect();
        self.key = key;
    }

    /// 把讀回的畫面套上輪廓 alpha，返回交給分層視窗的畫面。
    fn apply(&mut self, size: [usize; 2], rgba: &[u8]) -> &[u8] {
        if !self.mask.as_ref().is_some_and(|(s, k, _)| *s == size && *k == self.key) {
            self.mask = Some((size, self.key.clone(), crate::layered::coverage_mask(size, &self.polygons)));
        }
        let (_, _, mask) = self.mask.as_ref().unwrap();
        crate::layered::apply_mask(rgba, mask, &mut self.frame);
        &self.frame
    }
}

impl Window {
    pub fn hwnd(&self)->HWND {self.hwnd}

    /// 頂欄重新置頂後恢復島體層級；沒有遮擋時不修改視窗順序，也不啟用視窗。
    pub fn keep_above_top_bar(&mut self,bounds:crate::display_layout::Bounds) {
        let display=match &self.layering {Layering::Active(layered)=>layered.display(),_=>self.hwnd};
        if unsafe{top_bar_above(display,bounds)} {
            unsafe{let _=SetWindowPos(self.hwnd,HWND_TOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);}
            if let Layering::Active(layered)=&mut self.layering {layered.sync(self.hwnd,true);}
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        let RawWindowHandle::Win32(handle) = cc.window_handle().ok()?.as_raw() else { return None };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        let hook = unsafe {
            apply_style(hwnd, false, false);
            let _=enable_alpha_composition(hwnd);
            let _ = SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, 0);
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        // 區域用哨兵值：首幀（包括啟動即隱藏的空區域）也必須真正呼叫 SetWindowRgn，
        // 否則整塊透明畫布會擋住下層程式的點選。
        let backend=cc.wgpu_render_state.as_ref().map(|state|state.adapter.get_info().backend);
        let (vendor,renderer)=cc.gl.as_ref().map(|gl|{
            use eframe::glow::HasContext;
            unsafe{(gl.get_parameter_string(eframe::glow::VENDOR),gl.get_parameter_string(eframe::glow::RENDERER))}
        }).unwrap_or_default();
        // 本機驗證用：DAOZAI_OUTLINE_TEST=alpha 按 AMD 等路徑處理；=opaque 再假裝讀回的畫面沒有透明通道。
        let opaque_surface=surface_needs_outline_clip(backend,&vendor,&renderer) || outline_test().is_some();
        if opaque_surface{glass_diagnostic(&format!("渲染相容：{vendor} {renderer} {backend:?}，不直接信任系統透明合成，首幀前先按輪廓裁剪"));}
        watch_drag(hwnd,cc.egui_ctx.clone());
        Some(Self { hwnd, layering:Layering::Pending, region: (i32::MIN, 0, 0, 0), expanded: false, hook, browser_state:(None,PollClock::default()), drag_capture:DragCapture::default(), glass:None, backdrop:None, blur:false, blur_unavailable:false, opaque_surface, outline:ShapeAlpha::default() })
    }

    /// 使用目標螢幕的物理座標定位，並在縮放、島體大小、水平位置或螢幕連線變化後重新校正。
    /// `scale` 是主視窗的實際畫素比（系統 DPI × 島體大小），畫布按它同步調整物理尺寸。
    /// `shift` 是 OLED 畫素偏移的位移（物理畫素），按展開面板的螢幕餘量收窄後疊加在不貼邊的一軸上。
    pub fn place_on_display(&self, scale:f32, bounds:crate::display_layout::Bounds, offset:f32, dock:crate::model::Dock, shift:(i32,i32))->bool {
        unsafe {
            // 頂部：水平居中 + 水平偏移；左右：貼邊 + 豎直偏移，畫布換成豎向的大小。
            let origin=if dock.is_side() {
                crate::display_layout::side_origin(bounds,dock,(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H),crate::side_dock::SIDE_HALF_EXTENT,scale,offset)
            } else {crate::display_layout::island_origin_offset(bounds,crate::app::WIN_W,scale,offset)};
            let Some((x,y))=origin else{return false;};
            // OLED 畫素偏移：只沿不貼邊的一軸讓開，並按展開面板的餘量收窄——位置拉滿或螢幕太窄時少挪、不挪，
            // 貼邊的一側也保持嚴絲合縫，劉海不會和螢幕邊緣裂開。
            let (back,forth)=crate::display_layout::shift_slack(bounds,scale,if dock.is_side() {crate::side_dock::SIDE_HALF_EXTENT} else {crate::display_layout::ISLAND_HALF_EXTENT},offset,dock);
            let (dx,dy)=(shift.0.clamp(-back,forth),shift.1.clamp(-back,forth));
            let (x,y)=if dock.is_side() {(x,y+dy)} else {(x+dx,y)};
            let (canvas_w,canvas_h)=if dock.is_side() {(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)} else {(crate::app::CANVAS_W,crate::app::WIN_H)};
            let (w,h)=((canvas_w*scale).round() as i32,(canvas_h*scale).round() as i32);
            let mut current=RECT::default();
            if GetWindowRect(self.hwnd,&mut current).is_err(){return false;}
            let moved=(current.left,current.top)!=(x,y);
            let resized=(current.right-current.left,current.bottom-current.top)!=(w,h);
            if moved||resized {
                let flags=SWP_NOACTIVATE|SWP_NOZORDER|if resized{SET_WINDOW_POS_FLAGS(0)}else{SWP_NOSIZE};
                if SetWindowPos(self.hwnd,None,x,y,w,h,flags).is_err(){return false;}
            }
            true
        }
    }

    /// 系統背景模糊是否正在生效（毛玻璃皮膚據此選擇著色深淺）。
    pub fn blur_active(&self)->bool {self.blur}
    pub fn opaque_surface(&self)->bool {self.opaque_surface}

    /// 是否需要每幀讀回畫面交給分層視窗（相容後端，且分層視窗仍可用）。
    pub fn wants_frames(&self)->bool {self.opaque_surface && !matches!(self.layering,Layering::Unavailable)}

    /// 交一幀讀回的畫面（RGBA，已預乘）。返回畫面是否變化。
    /// 首幀看讀回的畫面有沒有真正的透明畫素：有就原樣交給分層視窗；沒有（部分 AMD 顯示卡的 OpenGL）
    /// 就按島體輪廓補算邊緣 alpha——普通模式也不再退回逐畫素「有或沒有」的輪廓裁剪。
    pub fn present_frame(&mut self,size:[usize;2],rgba:&[u8])->bool {
        if !self.wants_frames(){return false;}
        if matches!(self.layering,Layering::Pending) {
            let real=crate::layered::has_real_alpha(size,rgba) && outline_test()!=Some(true);
            // 還沒收到過島體輪廓時無從補 alpha，先保持輪廓裁剪，等下一幀。
            if !real && self.outline.polygons.is_empty() {return false;}
            match crate::layered::Layered::new() {
                Ok(layered)=>{
                    self.outline.active=!real;
                    glass_diagnostic(if real {"渲染相容：已切換為分層視窗逐畫素透明（邊緣抗鋸齒）"}
                        else {"渲染相容：讀回的畫面沒有透明通道，按島體輪廓補算邊緣透明度（分層視窗，邊緣抗鋸齒）"});
                    self.layering=Layering::Active(layered);
                }
                Err(error)=>{
                    glass_diagnostic(&format!("渲染相容：分層顯示視窗建立失敗：{error:?}"));
                    self.layering=Layering::Unavailable;return false;
                }
            }
        }
        let Layering::Active(layered)=&mut self.layering else {return false};
        let frame=if self.outline.active {self.outline.apply(size,rgba)} else {rgba};
        match layered.present(self.hwnd,size,frame) {
            // 切到分層視窗後，下一次 update 換回矩形區域（島仔視窗本身已幾乎不可見，只負責命中）。
            Ok(changed)=>changed,
            Err(error)=>{
                glass_diagnostic(&format!("渲染相容：分層視窗不可用，回到輪廓裁剪：{error:?}"));
                layered.detach(self.hwnd);
                self.layering=Layering::Unavailable;
                self.glass=None;self.region=(i32::MIN,0,0,0);
                false
            }
        }
    }

    pub fn logical_origin(&self,scale:f32)->Option<eframe::egui::Pos2>{
        let mut r=RECT::default();
        unsafe { GetWindowRect(self.hwnd,&mut r).ok()?; }
        Some(eframe::egui::pos2(r.left as f32/scale,r.top as f32/scale))
    }

    pub fn browser_at_top(&mut self,settings:&crate::model::Settings)->Option<bool> {
        if self.browser_state.1.ready(Duration::from_millis(100)) {
            self.browser_state=(crate::system::browser_at_top(self.hwnd,settings),PollClock::now());
        }
        self.browser_state.0
    }

    /// 區域引數均為視窗內的邏輯座標 (x, y, 寬, 高)。
    pub fn update(
        &mut self,
        region: (f32, f32, f32, f32),
        hit_rect: (f32, f32, f32, f32),
        notice_hit: Option<(f32,f32,f32,f32)>,
        file_drop_zone: Option<(f32,f32,f32,f32)>,
        glass: Option<&[crate::app::GlassShape]>,
        ppp: f32,
        editing: bool,
        expanded: bool,
    ) {
        let px = |v: f32| (v * ppp).round() as i32;
        let r = (px(region.0), px(region.1), px(region.0 + region.2), px(region.1 + region.3));
        // 僅在拖動進入感應區後擴大 OLE 接收區域，避免普通點選改變透明視窗裁剪。
        let button_down=unsafe{GetAsyncKeyState(VK_LBUTTON.0 as i32)<0 || GetAsyncKeyState(VK_RBUTTON.0 as i32)<0};
        let zone=file_drop_zone.map(|(x,y,w,h)|(px(x),px(y),px(x+w),px(y+h)));
        let mut cursor=POINT::default();let mut window=RECT::default();
        let cursor=unsafe{(GetCursorPos(&mut cursor).is_ok() && GetWindowRect(self.hwnd,&mut window).is_ok()).then_some((cursor.x-window.left,cursor.y-window.top))};
        let threshold=unsafe{((GetSystemMetrics(SM_CXDRAG)/2).max(1),(GetSystemMetrics(SM_CYDRAG)/2).max(1))};
        let capture=self.drag_capture.update(button_down,cursor,zone,threshold);
        let r=file_drag_region(r,zone,capture);
        let h = (
            px(hit_rect.0),
            px(hit_rect.1),
            px(hit_rect.0 + hit_rect.2),
            px(hit_rect.1 + hit_rect.3),
        );
        let mut hits=vec![h];
        if let Some((x,y,w,h))=notice_hit {hits.push((px(x),px(y),px(x+w),px(y+h)));}
        *hit().lock().unwrap_or_else(|e| e.into_inner()) = hits;
        // 毛玻璃：合成引擎的模糊塊貼合島體輪廓，視窗區域仍用矩形（含陰影與拖放感應區）。
        let _ = capture;
        // 分層視窗顯示的是帶真實透明通道的畫面時，系統模糊同樣能透出來（模糊底板緊貼在島仔視窗下方，
        // 分層顯示視窗在上方）；只有輪廓裁剪或按輪廓補 alpha（內部不透明）時才關掉模糊。
        let layered_alpha=matches!(self.layering,Layering::Active(_)) && !self.outline.active;
        match glass.filter(|g|!g.is_empty() && !self.blur_unavailable && (!self.opaque_surface || layered_alpha) && compositor_blur_allowed()) {
            Some(shapes) => {
                if self.backdrop.is_none() {
                    match crate::composition::Backdrop::new() {
                        Ok(backdrop) => {
                            glass_diagnostic("合成引擎模糊底板已建立");
                            self.backdrop = Some(backdrop);
                        }
                        Err(error) => {
                            glass_diagnostic(&format!("合成引擎模糊層建立失敗：{error:?}"));
                            self.blur_unavailable = true;
                        }
                    }
                }
                if let Some(backdrop) = &mut self.backdrop {
                    match backdrop.show(self.hwnd, &crate::app::blur_pieces(shapes, ppp)) {
                        Ok(()) => self.blur = true,
                        Err(error) => {
                            glass_diagnostic(&format!("模糊層更新失敗：{error:?}"));
                            backdrop.hide();
                            self.blur = false;
                            self.blur_unavailable = true;
                        }
                    }
                }
            }
            None => {
                if let Some(backdrop) = &mut self.backdrop { backdrop.hide(); }
                self.blur = false;
            }
        }
        // 記下島體輪廓：讀回畫面沒有透明通道時，分層視窗按它補算邊緣 alpha。
        if self.opaque_surface {
            if let Some(shapes)=glass.filter(|g|!g.is_empty()) {self.outline.set_shapes(shapes,ppp);}
        }
        // 不可靠的透明合成路徑按真實輪廓裁剪，避免圓角外側和透明畫布留下黑底。
        let outline_clip=self.opaque_surface && !matches!(self.layering,Layering::Active(_));
        match glass.filter(|_|outline_clip) {
            Some(shapes) => {
                let key=glass_key(shapes,ppp);
                if self.glass.as_ref()!=Some(&key) {
                    unsafe {
                        if let Some(rgn)=glass_region(&key) {
                            if SetWindowRgn(self.hwnd, rgn, true) != 0 {
                                self.glass=Some(key);
                                // 回到矩形區域時必須重新設定。
                                self.region=(i32::MIN,0,0,0);
                            } else {
                                let _ = DeleteObject(rgn);
                            }
                        }
                    }
                }
            }
            None if self.region != r || self.glass.is_some() => {
                unsafe {
                    let rgn = CreateRectRgn(r.0, r.1, r.2, r.3);
                    if SetWindowRgn(self.hwnd, rgn, true) != 0 {
                        self.region = r;
                        self.glass=None;
                    } else {
                        let _ = DeleteObject(rgn);
                    }
                }
            }
            None => {}
        }
        let raise=expanded && !self.expanded;
        unsafe { apply_style(self.hwnd, editing, raise) };
        if let Layering::Active(layered)=&mut self.layering {layered.sync(self.hwnd,raise);}
        self.expanded = expanded;
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(subclass), SUBCLASS_ID);
            if !self.hook.is_invalid() {
                let _ = UnhookWinEvent(self.hook);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static NATIVE_STATE_LOCK:Mutex<()>=Mutex::new(());

    #[test]
    fn opaque_surface_clips_black_background_even_without_system_blur(){
        let _guard=NATIVE_STATE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
        use windows::Win32::Graphics::Gdi::{GetWindowRgn,PtInRegion};
        for (backend,vendor,renderer) in [
            (Some(eframe::wgpu::Backend::Dx12),"",""),
            (Some(eframe::wgpu::Backend::Vulkan),"",""),
            (None,"ATI Technologies Inc.","AMD Radeon RX 6750 GRE 12GB"),
            (None,"Advanced Micro Devices, Inc.","Radeon RX 7900 XT"),
        ] {for blur_unavailable in [true,false] {for ppp in [1.0,1.25,1.5,2.0] {unsafe {
            let px=|v:f32|(v*ppp).round() as i32;
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,windows::core::w!("STATIC"),windows::core::w!(""),WS_POPUP,-10000,-10000,px(400.0),px(200.0),None,None,None,None).unwrap();
            let saved=hit().lock().unwrap_or_else(|e|e.into_inner()).clone();
            {
                let mut native=Window{hwnd,layering:Layering::Pending,region:(0,0,0,0),expanded:false,hook:HWINEVENTHOOK::default(),browser_state:(None,PollClock::now()),drag_capture:DragCapture::default(),glass:None,backdrop:None,blur:false,blur_unavailable,opaque_surface:surface_needs_outline_clip(backend,vendor,renderer),outline:ShapeAlpha::default()};
                let shapes=[crate::app::GlassShape::Round(eframe::egui::Rect::from_min_max(eframe::egui::pos2(80.0,60.0),eframe::egui::pos2(320.0,160.0)),25.0)];
                native.update((0.0,0.0,400.0,200.0),(80.0,60.0,240.0,100.0),None,None,Some(&shapes),ppp,false,false);
                let region=CreateRectRgn(0,0,0,0);
                GetWindowRgn(hwnd,region);
                assert!(PtInRegion(region,px(200.0),px(100.0)).as_bool());
                assert!(!PtInRegion(region,px(10.0),px(10.0)).as_bool(),"相容後端不能保留畫布上的黑色背景：{backend:?} / {vendor} / {renderer}");
                assert!(!PtInRegion(region,px(81.0),px(61.0)).as_bool(),"圓角外側必須透出桌面");
                assert!(!native.blur);
                assert!(native.backdrop.is_none(),"輪廓相容模式不能再啟用獨立模糊底板");
                native.update((0.0,0.0,0.0,0.0),(0.0,0.0,0.0,0.0),None,None,None,ppp,false,false);
                GetWindowRgn(hwnd,region);
                assert!(!PtInRegion(region,px(200.0),px(100.0)).as_bool(),"隱藏時不能留下上一幀輪廓");
                let _=DeleteObject(region);
            }
            *hit().lock().unwrap_or_else(|e|e.into_inner())=saved;
            let _=DestroyWindow(hwnd);
        }}}}
    }

    /// 部分 AMD 顯示卡的 OpenGL：讀回的畫面沒有透明通道。過去這裡直接退回輪廓裁剪（圓角、肩部全是鋸齒），
    /// 現在要切到分層視窗，並按島體輪廓補出帶過渡的邊緣 alpha。
    #[test]
    fn opaque_readback_uses_outline_alpha_instead_of_jagged_clip(){
        use crate::app::GlassShape;
        use eframe::egui::{pos2,Rect};
        use windows::Win32::Graphics::Gdi::{CreateRectRgn,DeleteObject,GetWindowRgn,PtInRegion};
        unsafe {
            let (w,h)=(400usize,200usize);
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,windows::core::w!("BUTTON"),windows::core::w!(""),WS_POPUP|WS_VISIBLE,-10000,-10000,w as i32,h as i32,None,None,None,None).unwrap();
            let saved=hit().lock().unwrap_or_else(|e|e.into_inner()).clone();
            {
                let mut native=Window{hwnd,layering:Layering::Pending,region:(0,0,0,0),expanded:false,hook:HWINEVENTHOOK::default(),browser_state:(None,PollClock::now()),drag_capture:DragCapture::default(),glass:None,backdrop:None,blur:false,blur_unavailable:false,opaque_surface:true,outline:ShapeAlpha::default()};
                let island=crate::shape::outline(Rect::from_min_max(pos2(80.0,0.0),pos2(320.0,60.0)),12.0,10.0,20.0);
                let shapes=[GlassShape::Outline(island)];
                // 讀回的畫面：島體黑色、畫布也是黑色，alpha 全部 255——和 AMD OpenGL 讀回的一樣。
                let opaque=[0u8,0,0,255].repeat(w*h);
                assert!(!native.present_frame([w,h],&opaque),"還沒收到輪廓時先保持輪廓裁剪");
                assert!(matches!(native.layering,Layering::Pending));
                native.update((0.0,0.0,w as f32,h as f32),(80.0,0.0,240.0,60.0),None,None,Some(&shapes),1.0,false,false);
                assert!(native.wants_frames());
                assert!(native.present_frame([w,h],&opaque),"讀不到透明通道也要切到分層視窗");
                assert!(matches!(native.layering,Layering::Active(_)) && native.outline.active);
                let frame=&native.outline.frame;
                let alpha=|x:usize,y:usize|frame[(y*w+x)*4+3];
                assert_eq!(alpha(200,30),255,"島體內部不透明");
                assert_eq!(alpha(10,150),0,"畫布空白處透明，不能留黑底");
                let soft=(0..h).flat_map(|y|(0..w).map(move|x|(x,y))).filter(|&(x,y)|(1..255).contains(&alpha(x,y))).count();
                assert!(soft>=80,"圓角和肩部要有抗鋸齒過渡畫素，實際 {soft}");
                assert!(frame.chunks_exact(4).all(|p|p[0]<=p[3]&&p[1]<=p[3]&&p[2]<=p[3]),"必須是合法的預乘顏色");
                // 切到分層視窗後不再用輪廓裁剪島仔視窗本身。
                native.update((0.0,0.0,w as f32,h as f32),(80.0,0.0,240.0,60.0),None,None,Some(&shapes),1.0,false,false);
                let region=CreateRectRgn(0,0,0,0);
                if GetWindowRgn(hwnd,region).0!=0 {assert!(PtInRegion(region,10,150).as_bool(),"分層顯示後視窗區域應回到矩形");}
                let _=DeleteObject(region);
                assert!(!native.present_frame([w,h],&opaque),"畫面沒變時不重複提交");
            }
            *hit().lock().unwrap_or_else(|e|e.into_inner())=saved;
            let _=DestroyWindow(hwnd);
        }
    }

    #[test]
    fn outline_clip_preserves_other_opengl_drivers_and_recognizes_amd_variants(){
        for (vendor,renderer) in [("NVIDIA Corporation","NVIDIA GeForce RTX 4060"),("Intel","Intel(R) UHD Graphics"),("Mesa","llvmpipe (LLVM 20.1.1, 256 bits)"),("Microsoft Corporation","GDI Generic")]{
            assert!(!surface_needs_outline_clip(None,vendor,renderer),"正常驅動不能被錯誤識別為 AMD：{vendor}");
        }
        for (vendor,renderer) in [("AMD","Radeon RX 6750 GRE"),("ati technologies inc.","Radeon"),("Mesa","AMD Radeon RX 6800 (radeonsi)"),("Mesa","Radeon RX 6750 GRE (radeonsi)")]{
            assert!(surface_needs_outline_clip(None,vendor,renderer),"需要識別 AMD 廠商與渲染器：{vendor} / {renderer}");
        }
    }

    #[test]
    fn glass_region_follows_the_island_outline_instead_of_its_bounding_box(){
        use crate::app::GlassShape;
        use eframe::egui::{pos2,Rect};
        use windows::Win32::Graphics::Gdi::PtInRegion;
        let outline=crate::shape::outline(Rect::from_min_max(pos2(100.0,0.0),pos2(300.0,60.0)),5.0,8.0,20.0);
        let shapes=[GlassShape::Outline(outline),GlassShape::Round(Rect::from_min_max(pos2(320.0,0.0),pos2(360.0,30.0)),10.0)];
        let key=glass_key(&shapes,1.5);
        assert_eq!(key,glass_key(&shapes,1.5),"同一幀輪廓的鍵必須穩定，避免每幀重設區域");
        assert_ne!(key,glass_key(&shapes,1.25));
        unsafe {
            let rgn=glass_region(&key).expect("輪廓應能生成區域");
            assert!(PtInRegion(rgn,300,45).as_bool(),"島體中央在區域內");
            assert!(!PtInRegion(rgn,152,88).as_bool(),"底部圓角外側不能出現模糊");
            assert!(PtInRegion(rgn,510,20).as_bool(),"通知詳情等圓角塊也在區域內");
            assert!(!PtInRegion(rgn,470,20).as_bool(),"兩塊之間保持透明");
            let _=DeleteObject(rgn);
        }
    }

    #[test]
    fn drag_capture_ignores_clicks_and_pointer_jitter() {
        let mut capture=DragCapture::default();let zone=Some((110,0,530,120));
        assert!(!capture.update(true,Some((300,60)),zone,(4,4)),"感應區內按下不能直接擴大視窗");
        assert!(!capture.update(true,Some((302,61)),zone,(4,4)),"點選時的小幅抖動不算拖動");
        assert!(!capture.update(false,Some((302,61)),zone,(4,4)));
    }

    #[test]
    fn drag_capture_ignores_repeated_clicks_below_the_island_without_an_observed_release() {
        // 極簡模式一秒才畫一幀：兩次點選之間可能沒有一幀看到鬆開。
        let mut capture=DragCapture::default();let zone=Some((110,0,530,120));
        assert!(!capture.update(true,Some((200,60)),zone,(4,4)));
        assert!(!capture.update(true,Some((380,60)),zone,(4,4)),"在島下方連續點選不同標籤不能被當成拖檔案");
        assert!(!capture.update(true,Some((300,30)),zone,(4,4)),"在感應區內拖動標籤或選文字也不能擴大視窗");
    }

    #[test]
    fn drag_capture_requires_movement_and_entry_and_resets_after_release() {
        let mut capture=DragCapture::default();let zone=Some((110,0,530,120));
        assert!(!capture.update(true,Some((700,400)),zone,(4,4)));
        assert!(!capture.update(true,Some((650,300)),zone,(4,4)),"遠處拖動不能改變島的區域");
        assert!(capture.update(true,Some((300,60)),zone,(4,4)),"拖到感應區後允許接收檔案");
        assert!(!capture.update(true,Some((700,400)),zone,(4,4)),"離開感應區應恢復區域");
        assert!(capture.update(true,Some((300,60)),zone,(4,4)));
        assert!(!capture.update(false,Some((300,60)),zone,(4,4)));
        assert!(!capture.update(true,Some((300,60)),zone,(4,4)),"下一次點選不能沿用上次拖動狀態");
        assert!(!capture.update(true,None,zone,(4,4)));
        assert!(!capture.update(true,Some((300,60)),None,(4,4)));
    }

    #[test]
    fn top_bar_mode_restores_z_order_without_raising_over_unrelated_windows(){
        let _guard=NATIVE_STATE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
        struct TestWindow(HWND);
        impl Drop for TestWindow {fn drop(&mut self){unsafe{let _=DestroyWindow(self.0);}}}
        unsafe fn above(first:HWND,second:HWND)->bool {
            let mut current=first;
            while let Ok(next)=GetWindow(current,GW_HWNDNEXT){if next==second{return true;}current=next;}
            false
        }
        unsafe {
            let bounds=crate::display_layout::Bounds{left:-10480,top:-10000,right:-8560,bottom:-8920};
            let flags=SWP_NOACTIVATE|SWP_SHOWWINDOW;
            let create=||TestWindow(CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,windows::core::w!("STATIC"),windows::core::w!(""),WS_POPUP|WS_VISIBLE,-10000,-10000,416,140,None,None,None,None).unwrap());
            for layered in [false,true] {
                let island=create();let cover=create();
                let mut native=Window{hwnd:island.0,layering:Layering::Pending,region:(0,0,0,0),expanded:false,hook:HWINEVENTHOOK::default(),browser_state:(None,PollClock::now()),drag_capture:DragCapture::default(),glass:None,backdrop:None,blur:false,blur_unavailable:false,opaque_surface:false,outline:ShapeAlpha::default()};
                if layered {
                    let mut display=crate::layered::Layered::new().unwrap();
                    display.present(island.0,[416,140],&vec![0;416*140*4]).unwrap();
                    native.layering=Layering::Active(display);
                }
                let display=match &native.layering {Layering::Active(layered)=>layered.display(),_=>island.0};
                let foreground=GetForegroundWindow();
                SetWindowPos(cover.0,HWND_TOPMOST,bounds.left,bounds.top,1920,32,flags).unwrap();
                assert!(top_bar_above(display,bounds),"復現頂欄遮擋，相容渲染={layered}");
                native.keep_above_top_bar(bounds);
                assert!(!top_bar_above(display,bounds),"收起時也應恢復實際層級，相容渲染={layered}");
                assert_eq!(GetForegroundWindow(),foreground,"置頂不能搶走輸入焦點");
                for (x,y,w,h) in [(-10000,-10000,400,140),(-10480,-10000,1920,300),(-10480,-10000,1920,1080),(-10480,-9600,1920,32),(-12400,-10000,1920,32)] {
                    SetWindowPos(cover.0,HWND_TOPMOST,x,y,w,h,flags).unwrap();
                    assert!(!top_bar_above(display,bounds),"普通彈窗、全屏視窗和其他螢幕不觸發頂欄適配");
                    native.keep_above_top_bar(bounds);
                    assert!(above(cover.0,display),"不應越過無關彈窗爭搶置頂");
                }
                SetWindowPos(cover.0,HWND_TOPMOST,bounds.left,bounds.top,1920,32,flags).unwrap();
                let _=ShowWindow(cover.0,SW_HIDE);
                assert!(!top_bar_above(display,bounds),"隱藏的頂欄不觸發抬高");
            }
        }
    }

    #[test]
    fn expanding_raises_the_island_without_activating_it(){
        let _guard=NATIVE_STATE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
        struct TestWindow(HWND);
        impl Drop for TestWindow {fn drop(&mut self){unsafe{let _=DestroyWindow(self.0);}}}
        unsafe fn above(first:HWND,second:HWND)->bool {
            let mut current=first;
            while let Ok(next)=GetWindow(current,GW_HWNDNEXT){
                if next==second{return true;}
                current=next;
            }
            false
        }
        unsafe {
            // 視窗在螢幕外可見，才能驗證實際桌面層級；不啟用，也不遮擋使用者桌面。
            let create=||TestWindow(CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,windows::core::w!("STATIC"),windows::core::w!(""),WS_POPUP|WS_VISIBLE,-10000,-10000,416,140,None,None,None,None).unwrap());
            let island=create();let cover=create();
            let flags=SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE;
            apply_style(island.0,false,false);
            SetWindowPos(cover.0,HWND_TOPMOST,0,0,0,0,flags).unwrap();
            assert!(above(cover.0,island.0),"先復現其他視窗蓋住島體");
            let foreground=GetForegroundWindow();
            let saved=hit().lock().unwrap_or_else(|e|e.into_inner()).clone();
            let mut native=Window{hwnd:island.0,layering:Layering::Pending,region:(0,0,0,0),expanded:false,hook:HWINEVENTHOOK::default(),browser_state:(None,PollClock::now()),drag_capture:DragCapture::default(),glass:None,backdrop:None,blur:false,blur_unavailable:false,opaque_surface:false,outline:ShapeAlpha::default()};
            let update=|native:&mut Window,expanded|native.update((0.0,0.0,416.0,140.0),(0.0,0.0,416.0,140.0),None,None,None,1.0,false,expanded);
            update(&mut native,true);
            assert!(above(island.0,cover.0),"展開時必須重新抬高實際視窗層級，不能只檢查置頂標記");
            assert_eq!(GetForegroundWindow(),foreground,"懸停展開不能搶走當前應用的輸入焦點");
            SetWindowPos(cover.0,HWND_TOPMOST,0,0,0,0,flags).unwrap();
            update(&mut native,true);
            assert!(above(cover.0,island.0),"展開後不能每幀與其他彈窗爭搶層級");
            update(&mut native,false);
            assert!(above(cover.0,island.0),"收起時不抬高層級");
            // 模擬只剩置頂標記、實際順序落在普通視窗後面的情況。
            SetWindowPos(island.0,HWND_NOTOPMOST,0,0,0,0,flags).unwrap();
            SetWindowPos(cover.0,HWND_NOTOPMOST,0,0,0,0,flags).unwrap();
            SetWindowLongPtrW(island.0,GWL_EXSTYLE,GetWindowLongPtrW(island.0,GWL_EXSTYLE)|WS_EX_TOPMOST.0 as isize);
            assert!(above(cover.0,island.0));
            update(&mut native,true);
            assert!(above(island.0,cover.0),"再次展開也必須恢復實際置頂順序");
            assert_eq!(GetForegroundWindow(),foreground);
            *hit().lock().unwrap_or_else(|e|e.into_inner())=saved;
        }
    }

    #[test]
    fn file_drag_expands_region_only_when_capture_is_active(){
        let island=(230,0,410,42);let zone=(110,0,530,120);
        assert_eq!(file_drag_region(island,Some(zone),false),island,"普通懸停不擴大原生區域");
        assert_eq!(file_drag_region(island,Some(zone),true),zone,"拖入感應區應覆蓋島下方");
        assert_eq!(file_drag_region(island,None,true),island,"關閉置物架或隱藏島時不接收外圍拖放");
        let expanded=(64,0,576,220);
        assert_eq!(file_drag_region(expanded,Some(zone),true),expanded,"展開後的檔案區不應被感應區裁掉");
    }

    #[test]
    fn file_capture_below_the_island_still_returns_click_through(){
        let _guard=NATIVE_STATE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
        unsafe {
            let saved=std::mem::replace(&mut *hit().lock().unwrap_or_else(|e|e.into_inner()),vec![(230,0,410,42)]);
            let hwnd=CreateWindowExW(WS_EX_NOACTIVATE,windows::core::w!("STATIC"),windows::core::w!(""),WS_POPUP,0,0,640,270,None,None,None,None).unwrap();
            SetWindowSubclass(hwnd,Some(subclass),SUBCLASS_ID,0).unwrap();
            let region=file_drag_region((230,0,410,42),Some((110,0,530,120)),true);
            let rgn=CreateRectRgn(region.0,region.1,region.2,region.3);assert_ne!(SetWindowRgn(hwnd,rgn,false),0);
            let mut rect=RECT::default();GetWindowRect(hwnd,&mut rect).unwrap();
            let point=|x:i32,y:i32|LPARAM((((rect.top+y) as u32)<<16|((rect.left+x) as u16 as u32)) as isize);
            assert_eq!(SendMessageW(hwnd,WM_NCHITTEST,WPARAM(0),point(500,80)).0,HTTRANSPARENT as isize,"感應區中的普通點選仍須穿透");
            assert_eq!(SendMessageW(hwnd,WM_NCHITTEST,WPARAM(0),point(320,15)).0,HTCLIENT as isize,"島體本身仍可互動");
            let _=DestroyWindow(hwnd);*hit().lock().unwrap_or_else(|e|e.into_inner())=saved;
        }
    }

    #[test]
    fn right_notice_keeps_empty_space_below_the_main_bar_click_through(){
        let _guard=NATIVE_STATE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
        let saved=std::mem::replace(&mut *hit().lock().unwrap_or_else(|e|e.into_inner()),vec![(100,0,400,30),(372,0,760,96)]);
        assert!(inside_hit(600,70));assert!(inside_hit(200,15));assert!(!inside_hit(200,70));
        *hit().lock().unwrap_or_else(|e|e.into_inner())=saved;
    }

    #[test]
    fn activation_keeps_focus_messages_but_suppresses_nonclient_paint(){
        #[derive(Default)]
        struct Capture{activation:Vec<(usize,isize)>,paint:u32}
        unsafe extern "system" fn observe(hwnd:HWND,msg:u32,wp:WPARAM,lp:LPARAM,_id:usize,data:usize)->LRESULT{
            let capture=&mut *(data as *mut Capture);
            if msg==WM_NCACTIVATE{capture.activation.push((wp.0,lp.0));}
            if msg==WM_NCPAINT{capture.paint+=1;return LRESULT(0);}
            DefSubclassProc(hwnd,msg,wp,lp)
        }
        unsafe{
            let mut capture=Capture::default();
            let hwnd=CreateWindowExW(WS_EX_NOACTIVATE,windows::core::w!("STATIC"),windows::core::w!(""),WS_POPUP,0,0,100,50,None,None,None,None).unwrap();
            SetWindowSubclass(hwnd,Some(observe),SUBCLASS_ID+2,&mut capture as *mut Capture as usize).unwrap();
            SetWindowSubclass(hwnd,Some(subclass),SUBCLASS_ID,0).unwrap();
            for active in [1,0]{let result=SendMessageW(hwnd,WM_NCACTIVATE,WPARAM(active),LPARAM(0));if active==0{assert_ne!(result.0,0,"仍須允許切換到其他視窗");}}
            SendMessageW(hwnd,WM_NCPAINT,WPARAM(1),LPARAM(0));
            let _=DestroyWindow(hwnd);
            assert_eq!(capture.activation,[(1,-1),(0,-1)],"仍需通知視窗框架焦點變化，同時阻止系統重畫頂邊");
            assert_eq!(capture.paint,0,"透明島體不能交給系統繪製非客戶區");
        }
    }

    #[test]
    fn editing_mode_does_not_recalculate_the_native_window_frame(){
        unsafe extern "system" fn count_frame(hwnd:HWND,msg:u32,wp:WPARAM,lp:LPARAM,_id:usize,data:usize)->LRESULT {
            if msg==WM_NCCALCSIZE {*(data as *mut u32)+=1;}
            DefSubclassProc(hwnd,msg,wp,lp)
        }
        struct TestWindow(HWND);
        impl Drop for TestWindow {fn drop(&mut self){unsafe{let _=DestroyWindow(self.0);}}}
        unsafe {
            let mut count=0u32;
            // 與實際島體一樣，從已置頂視窗驗證輸入狀態切換。
            let window=TestWindow(CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOPMOST,windows::core::w!("STATIC"),windows::core::w!(""),WS_POPUP,0,0,100,50,None,None,None,None).unwrap());
            SetWindowSubclass(window.0,Some(count_frame),SUBCLASS_ID+1,&mut count as *mut u32 as usize).unwrap();
            apply_style(window.0,false,false);count=0;
            for editing in [true,false,true,false] {apply_style(window.0,editing,false);}
            apply_style(window.0,false,true);
            assert_eq!(count,0,"新增和退出輸入時不應觸發原生邊框重算，避免頂端白條閃現");
            assert_ne!(GetWindowLongPtrW(window.0,GWL_EXSTYLE)&WS_EX_TOPMOST.0 as isize,0,"仍應保持島體置頂");
        }
    }

    #[test]
    fn mouse_click_keeps_overlay_inactive_except_when_editing() {
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) { unsafe { let _ = DestroyWindow(self.0); } }
        }
        unsafe {
            let window = TestWindow(CreateWindowExW(
                WS_EX_NOACTIVATE, windows::core::w!("STATIC"), windows::core::w!(""),
                WS_POPUP, 0, 0, 100, 50, None, None, None, None,
            ).unwrap());
            SetWindowSubclass(window.0, Some(subclass), SUBCLASS_ID, 0).unwrap();
            let click = LPARAM(((WM_LBUTTONDOWN << 16) | HTCLIENT) as isize);
            assert_eq!(SendMessageW(window.0, WM_MOUSEACTIVATE, WPARAM(0), click).0, MA_NOACTIVATE as isize);
            let style = GetWindowLongPtrW(window.0, GWL_EXSTYLE);
            SetWindowLongPtrW(window.0, GWL_EXSTYLE, style & !(WS_EX_NOACTIVATE.0 as isize));
            assert_eq!(SendMessageW(window.0, WM_MOUSEACTIVATE, WPARAM(0), click).0, MA_ACTIVATE as isize);
        }
    }
}
