//! 毛玻璃皮膚的實時背景模糊：Windows 合成引擎（Windows.UI.Composition）。
//!
//! 早先用的 `SetWindowCompositionAttribute` 模糊總是鋪滿整個視窗矩形、不認視窗區域，
//! 而島仔視窗是一大塊透明畫布，於是桌面頂部出現一整塊深色方塊。
//!
//! 現在的做法：單獨建一個只承載合成視覺的「模糊底板」視窗（無重定向點陣圖、不接收滑鼠、
//! 不出現在工作列），與島仔視窗同位置同大小，並始終緊貼在它**正下方**。底板上的每塊視覺
//! 都用宿主背景畫刷（Windows 11 亞克力用的系統實時模糊），主體按圓角裁剪，頂部外翻的
//! 肩部按物理畫素逐行拼出，模糊因此嚴格貼合島體輪廓；島體的著色、高光與文字仍由 egui
//! 畫在上層的島仔視窗裡。（直接掛在島仔視窗上時，部分顯示卡會把視覺層疊到內容之上。）

use crate::app::BlurPiece;
use windows::core::Interface;
use windows::Foundation::Numerics::{Vector2, Vector3};
use windows::System::DispatcherQueueController;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{CompositionBrush, CompositionRoundedRectangleGeometry, Compositor, ContainerVisual, SpriteVisual, VisualCollection};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_HOSTBACKDROPBRUSH};
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::System::WinRT::{CreateDispatcherQueueController, DispatcherQueueOptions, RoInitialize, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, RO_INIT_SINGLETHREADED};

/// 底板視窗：滑鼠一律穿透、不啟用。
unsafe extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn create_backdrop_window() -> windows::core::Result<HWND> {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    static CLASS: std::sync::Once = std::sync::Once::new();
    let instance = unsafe { GetModuleHandleW(None)? };
    let class = windows::core::w!("DaozaiGlassBackdrop");
    CLASS.call_once(|| unsafe {
        let wc = WNDCLASSW { lpfnWndProc: Some(backdrop_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
    });
    const WS_EX_NOREDIRECTIONBITMAP: WINDOW_EX_STYLE = WINDOW_EX_STYLE(0x0020_0000);
    unsafe {
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class,
            windows::core::w!(""),
            WS_POPUP,
            0, 0, 1, 1,
            None, None, instance, None,
        )
    }
}

fn enable_host_backdrop(hwnd: HWND) -> windows::core::Result<()> {
    unsafe {
        let on = BOOL::from(true);
        DwmSetWindowAttribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, (&on as *const BOOL).cast(), std::mem::size_of::<BOOL>() as u32)
    }
}

/// 把底板視窗裁成模糊塊的並集。底板和島仔畫布一樣大，而 `HTTRANSPARENT`
/// 只把點選讓給同一執行緒的視窗：不裁剪時，畫布範圍內（含島右側和下方大片空白）
/// 其他程式的標題欄按鈕等都會點不動，直到退出島仔。
///
/// 裁剪只負責讓出點選，可見輪廓由各模糊塊自己的圓角裁剪決定，所以按 `CLIP_GRID` 畫素向外取整：
/// 動畫和歌詞寬度變化時不必每幀重設視窗區域。每次重設都會讓合成器重建這扇視窗，
/// 在部分核顯舊驅動上頻繁重設會拖垮合成器（整機閃屏後卡死）。
const CLIP_GRID: i32 = 8;

fn clip_rects(pieces: &[BlurPiece]) -> Vec<(i32, i32, i32, i32)> {
    let down = |v: f32| (v.floor() as i32).div_euclid(CLIP_GRID) * CLIP_GRID;
    let up = |v: f32| -((-(v.ceil() as i32)).div_euclid(CLIP_GRID) * CLIP_GRID);
    let mut rects: Vec<_> = pieces.iter().filter(|p| p.w > 0.0 && p.h > 0.0)
        .map(|p| (down(p.x), down(p.y), up(p.x + p.w), up(p.y + p.h))).collect();
    rects.sort_unstable();
    rects.dedup();
    rects
}

/// 區域沒變時不呼叫系統；返回是否真的重設了區域。
fn clip_to_pieces(hwnd: HWND, pieces: &[BlurPiece], applied: &mut Option<Vec<(i32, i32, i32, i32)>>) -> bool {
    use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, SetWindowRgn, RGN_OR};
    let rects = clip_rects(pieces);
    if applied.as_ref() == Some(&rects) { return false; }
    unsafe {
        let all = CreateRectRgn(0, 0, 0, 0);
        if all.0.is_null() { return false; }
        for &(l, t, r, b) in &rects {
            let part = CreateRectRgn(l, t, r, b);
            if part.0.is_null() { continue; }
            let _ = CombineRgn(all, all, part, RGN_OR);
            let _ = DeleteObject(part);
        }
        // 底板沒有重定向點陣圖，內容全由合成視覺提供，不需要系統重畫。成功後區域歸系統所有，不能再刪除。
        if SetWindowRgn(hwnd, all, false) == 0 { let _ = DeleteObject(all); return false; }
    }
    *applied = Some(rects);
    true
}

pub struct Backdrop {
    hwnd: HWND,
    placed: (RECT, bool),
    // 合成器依賴當前執行緒的 DispatcherQueue；控制器必須與合成器同生命週期。
    _queue: Option<DispatcherQueueController>,
    compositor: Compositor,
    _target: DesktopWindowTarget,
    root: ContainerVisual,
    children: VisualCollection,
    brush: CompositionBrush,
    pool: Vec<(SpriteVisual, CompositionRoundedRectangleGeometry)>,
    shown: Vec<BlurPiece>,
    clip: Option<Vec<(i32, i32, i32, i32)>>,
    visible: bool,
}

impl Backdrop {
    pub fn new() -> windows::core::Result<Self> {
        let hwnd = create_backdrop_window()?;
        // Win10 不支援這個 Win32 宿主背景屬性。失敗時不能繼續繪製純色畫刷，
        // 交給上層使用半透明底色，並釋放尚未顯示的底板視窗。
        if let Err(error) = enable_host_backdrop(hwnd) {
            unsafe { let _ = DestroyWindow(hwnd); }
            return Err(error);
        }
        unsafe {
            // 主執行緒已由視窗框架初始化為單執行緒套間時返回 S_FALSE，忽略即可。
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
        }
        let queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_NONE,
            })
        }
        .ok();
        let compositor = Compositor::new()?;
        let interop: ICompositorDesktopInterop = compositor.cast()?;
        // 底板視窗沒有自己的內容，視覺樹就是它的全部畫面。
        let target = unsafe { interop.CreateDesktopWindowTarget(hwnd, BOOL::from(true)) }?;
        let root = compositor.CreateContainerVisual()?;
        target.SetRoot(&root)?;
        let children = root.Children()?;
        let brush: CompositionBrush = compositor.CreateHostBackdropBrush()?.cast()?;
        root.SetIsVisible(false)?;
        Ok(Self { hwnd, placed: (RECT::default(), false), _queue: queue, compositor, _target: target, root, children, brush, pool: Vec::new(), shown: Vec::new(), clip: None, visible: false })
    }

    /// 讓底板與島仔視窗重合，並緊貼在它正下方。
    fn follow(&mut self, island: HWND) {
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(island, &mut rect).is_err() { return; }
            let below = GetWindow(island, GW_HWNDNEXT).ok() == Some(self.hwnd);
            if self.placed.0 != rect || !below || !self.placed.1 {
                let _ = SetWindowPos(self.hwnd, island, rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                self.placed = (rect, true);
            }
        }
    }

    /// 顯示並更新模糊塊（物理畫素，島仔視窗座標）。形狀不變時不提交任何改動。
    pub fn show(&mut self, island: HWND, pieces: &[BlurPiece]) -> windows::core::Result<()> {
        self.follow(island);
        if pieces != self.shown.as_slice() {
            while self.pool.len() < pieces.len() {
                let visual = self.compositor.CreateSpriteVisual()?;
                visual.SetBrush(&self.brush)?;
                let geometry = self.compositor.CreateRoundedRectangleGeometry()?;
                let clip = self.compositor.CreateGeometricClipWithGeometry(&geometry)?;
                visual.SetClip(&clip)?;
                self.children.InsertAtTop(&visual)?;
                self.pool.push((visual, geometry));
            }
            for (i, (visual, geometry)) in self.pool.iter().enumerate() {
                match pieces.get(i) {
                    Some(p) => {
                        let size = Vector2 { X: p.w, Y: p.h };
                        visual.SetOffset(Vector3 { X: p.x, Y: p.y, Z: 0.0 })?;
                        visual.SetSize(size)?;
                        geometry.SetSize(size)?;
                        geometry.SetCornerRadius(Vector2 { X: p.radius, Y: p.radius })?;
                        visual.SetIsVisible(true)?;
                    }
                    None => visual.SetIsVisible(false)?,
                }
            }
            clip_to_pieces(self.hwnd, pieces, &mut self.clip);
            self.shown = pieces.to_vec();
        }
        if !self.visible {
            self.root.SetIsVisible(true)?;
            self.visible = true;
        }
        Ok(())
    }

    pub fn hide(&mut self) {
        if self.visible {
            let _ = self.root.SetIsVisible(false);
            self.visible = false;
        }
        if self.placed.1 {
            unsafe { let _ = ShowWindow(self.hwnd, SW_HIDE); }
            self.placed.1 = false;
        }
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        unsafe { let _ = DestroyWindow(self.hwnd); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, GetWindowRgn, PtInRegion};

    #[test]
    fn regression_rejected_host_backdrop_is_not_reported_as_working_blur() {
        assert!(enable_host_backdrop(HWND::default()).is_err(),"DWM 拒絕宿主背景畫刷時必須啟用透明回退，不能把純色底板當成模糊成功");
    }

    #[test]
    fn backdrop_initialization_respects_system_support_and_starts_hidden() {
        let hwnd=create_backdrop_window().unwrap();
        let supported=enable_host_backdrop(hwnd).is_ok();
        unsafe { let _=DestroyWindow(hwnd); }
        match Backdrop::new() {
            Ok(backdrop)=>{
                assert!(supported);
                assert!(!unsafe{IsWindowVisible(backdrop.hwnd)}.as_bool(),"模糊塊準備好之前不能顯示矩形底板");
            }
            Err(error)=>assert!(!supported,"支援宿主背景畫刷時不應初始化失敗：{error}"),
        }
    }

    #[test]
    fn backdrop_only_covers_the_blur_pieces_so_other_apps_stay_clickable() {
        let hwnd = create_backdrop_window().unwrap();
        unsafe {
            let _ = SetWindowPos(hwnd, None, 0, 0, 960, 270, SWP_NOACTIVATE | SWP_NOZORDER);
            let mut applied = None;
            assert!(clip_to_pieces(hwnd, &[BlurPiece { x: 230.0, y: 0.0, w: 180.0, h: 34.0, radius: 12.0 }], &mut applied));
            let region = CreateRectRgn(0, 0, 0, 0);
            assert_ne!(GetWindowRgn(hwnd, region).0, 0, "底板必須帶視窗區域");
            assert!(PtInRegion(region, 320, 17).as_bool(), "島體下方仍要顯示模糊");
            assert!(!PtInRegion(region, 800, 150).as_bool(), "畫布右側空白處必須讓給下層程式");
            assert!(!PtInRegion(region, 320, 200).as_bool(), "島下方空白處必須讓給下層程式");
            assert!(clip_to_pieces(hwnd, &[], &mut applied));
            GetWindowRgn(hwnd, region);
            assert!(!PtInRegion(region, 320, 17).as_bool(), "沒有模糊塊時不能佔任何區域");
            let _ = DeleteObject(region);
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn animation_frames_do_not_reset_the_backdrop_region_every_frame() {
        // 展開 / 歌詞寬度變化時輪廓每幀只差零點幾到一兩個畫素：大多數幀不應重設視窗區域。
        let hwnd = create_backdrop_window().unwrap();
        let mut applied = None;
        let mut resets = 0;
        for i in 0..60 {
            let w = 180.0 + i as f32 * 0.5;
            if clip_to_pieces(hwnd, &[BlurPiece { x: 320.0 - w / 2.0, y: 0.0, w, h: 34.0, radius: 12.0 }], &mut applied) { resets += 1; }
        }
        assert!(resets <= 10, "60 幀細微變化重設了 {resets} 次視窗區域");
        let rects = applied.unwrap();
        assert!(rects.iter().all(|r| r.0 % CLIP_GRID == 0 && r.2 % CLIP_GRID == 0), "區域應按網格取整");
        unsafe { let _ = DestroyWindow(hwnd); }
    }
}
