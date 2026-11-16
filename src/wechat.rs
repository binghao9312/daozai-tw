//! 工作列提醒共用主視窗訊息迴圈；按帳號未讀狀態合併，不讀取聊天內容。
use std::{cell::RefCell, path::PathBuf, time::{Duration, Instant}};
use windows::{core::{w, PCWSTR}, Win32::{
    Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM},
    System::Registry::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER},
    UI::{Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass}, WindowsAndMessaging::*},
}};

use crate::messenger::{OpenError,Unread};
use crate::messenger::windows::*;

pub(crate) const ADAPTER:NativeAdapter=NativeAdapter{
    paths:installed_paths,main_frame:wechat_main_frame,accounts:None,ready:|_|true,strict_window:false,open:open_wechat,taskbar:Some(start_taskbar),
};
fn wechat_main_frame(class:&str,style:u32,extended:u32)->bool{
    (class=="WeChatMainWndForPC" || class.starts_with("Qt") && class.ends_with("QWindowIcon")) && frame_style(class,style,extended)
}
fn start_taskbar(ctx:eframe::egui::Context)->Result<Box<dyn TaskbarSource>,String>{Ok(Box::new(Listener::new(ctx)?))}
impl TaskbarSource for Listener {
    fn registered(&self)->bool{Listener::registered(self)}
    fn take(&self)->Vec<(u32,isize)>{Listener::take(self)}
    fn forget(&self,pid:u32){Listener::forget(self,pid);}
}
fn open_wechat(unread:Unread,paths:&[String],allow_tray:bool)->Result<(),OpenError>{
    let window=HWND(unread.window as *mut _);
    unsafe{
        let target=if unread.pid==0{None}
            else if window_pid(window)==unread.pid && usable_main_frame(window,unread.source) && matches(window,paths){Some(window)}
            else{account_main_window(unread.pid,unread.source,paths)};
        if let Some(target)=target{
            wake_step(format!("切前台 {}",describe_window(target)));
            if IsIconic(target).as_bool(){ShowWindowAsync(target,SW_RESTORE).ok().map_err(|_|{wake_step("还原最小化窗口失败");OpenError::Failed("请从任务栏打开微信".into())})?;}
            if SetForegroundWindow(target).as_bool(){wake_step("系统已接受");return Ok(());}
            wake_step("系统拒绝切前台");
        }else{wake_step("没有该账号可见的微信主窗口");}
    }
    if !allow_tray{wake_step("未开启从托盘唤起");return Err(OpenError::Failed("请从任务栏或托盘打开微信".into()));}
    // 系統匣喚起的每一步記在 wechat-tray.log（失敗時寫入，失敗程式碼 T0–T5 同時出現在喚起結果裡）。
    wake_step("改从托盘唤起");
    wechat_in_tray(unread).map_err(OpenError::Failed)?;
    open_wechat_from_tray(unread).map_err(OpenError::Failed)
}

const SUBCLASS_ID: usize = 0x5745_4348;
const FLASH: usize = 0x8006;

/// 合併同一視窗的短間隔事件；是否重彈由介面的帳號未讀狀態決定。
#[derive(Default)]
struct Burst {last:Option<Instant>,window:isize}
impl Burst {
    fn attention(&mut self,now:Instant,window:isize)->bool{
        let changed=self.window!=window;self.window=window;
        self.last.replace(now).is_none_or(|last|now.duration_since(last)>=Duration::from_secs(5)) || changed
    }
}

// 只提取協議處理程式的路徑，絕不執行登錄檔裡的命令或引數。
fn protocol_executable(command:&str)->Option<PathBuf>{
    let command=command.trim();
    let exe=if let Some(quoted)=command.strip_prefix('"'){quoted.split_once('"')?.0}else{
        command.strip_suffix(" \"%1\"").or_else(||command.strip_suffix(" %1")).unwrap_or(command)
    };
    let path=PathBuf::from(exe);
    let name=path.file_name()?.to_str()?;
    (path.is_absolute() && (name.eq_ignore_ascii_case("Weixin.exe") || name.eq_ignore_ascii_case("WeChat.exe"))).then_some(path)
}

fn installed_paths() -> Vec<String> {
    let mut paths=Vec::new();
    for (key,exe) in [(w!("Software\\Tencent\\Weixin"),"Weixin.exe"),(w!("Software\\Tencent\\WeChat"),"WeChat.exe")] {
        if let Some(folder)=registry_string(HKEY_CURRENT_USER,key,w!("InstallPath")){paths.push(PathBuf::from(folder).join(exe));}
    }
    if let Some(path)=registry_string(HKEY_CLASSES_ROOT,w!("weixin\\shell\\open\\command"),PCWSTR::null()).and_then(|cmd|protocol_executable(&cmd)){paths.push(path);}
    let mut paths:Vec<String>=paths.into_iter().filter(|p|p.is_absolute() && p.is_file()).filter_map(|p|p.canonicalize().ok()).map(|p|normalized(&p.to_string_lossy())).collect();
    // 登錄檔路徑可能過期（升級、換目錄、複製一份雙開、綠色版）：再認檔名相同、且由騰訊釋出的程式。
    paths.extend(["*weixin.exe".to_string(),"*wechat.exe".to_string()]);
    paths
}

/// 閃動與前臺檢視使用同一帳號程序口徑，相容主視窗與系統匣宿主位於不同程序。
pub(crate) fn account_pid(pid:u32,paths:&[String])->u32{
    let owners:std::collections::HashSet<_>=unsafe{tray_hosts(paths)}.into_iter().map(|(pid,_)|pid).collect();
    if owners.is_empty() || owners.contains(&pid){return pid;}
    account_tray_pid(pid,&owners.into_iter().collect::<Vec<_>>(),&parent_processes()).unwrap_or(pid)
}

/// 系統匣喚起的診斷：只記步驟、視窗類名、位置和系統版本，不記視窗標題、提示文字或聊天內容。
struct TrayLog(Vec<String>);
impl TrayLog {
    fn new(unread:crate::app::Unread)->Self{
        use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
        let key=w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
        let build=registry_string(HKEY_LOCAL_MACHINE,key,w!("CurrentBuild")).unwrap_or_default();
        let display=registry_string(HKEY_LOCAL_MACHINE,key,w!("DisplayVersion")).unwrap_or_default();
        Self(vec![format!("岛仔 {} · Windows 内部版本 {build} ({display}) · 账号进程 {} · 闪动窗口 {:#x}",env!("CARGO_PKG_VERSION"),unread.pid,unread.window)])
    }
    fn step(&mut self,line:impl Into<String>){self.0.push(line.into());}
    /// 寫入日誌並返回給使用者看的提示（帶錯誤碼，便於反饋）。
    fn fail(&mut self,code:&str,reason:&str)->String{
        self.step(format!("失败 {code}：{reason}"));
        let directory=crate::model::config_dir().join("diagnostics");
        let saved=std::fs::create_dir_all(&directory).and_then(|_|std::fs::write(directory.join("wechat-tray.log"),self.0.join("\n")+"\n")).is_ok();
        if saved {format!("未能唤起微信（{code}），请从托盘打开；诊断已记录")} else {format!("未能唤起微信（{code}），请从托盘打开")}
    }
}

unsafe fn tray_hosts(paths:&[String])->Vec<(u32,HWND)>{
    use windows::Win32::Foundation::BOOL;
    struct Find<'a>{paths:&'a [String],hosts:Vec<(u32,HWND)>}
    unsafe extern "system" fn visit(hwnd:HWND,data:LPARAM)->BOOL{
        let find=&mut *(data.0 as *mut Find);
        // 微信 4.x 為 Qt…WxTrayIconMessageWindowClass；只認類名裡帶 TrayIcon 且屬於微信安裝路徑的視窗。
        if class_name(hwnd).contains("TrayIcon") && matches(hwnd,find.paths){find.hosts.push((window_pid(hwnd),hwnd));}
        true.into()
    }
    let mut find=Find{paths,hosts:Vec::new()};
    let _=EnumWindows(Some(visit),LPARAM((&mut find as *mut Find) as isize));
    find.hosts
}

struct TrayAccount {
    pid:u32,
    hosts:Vec<(u32,HWND)>,
    icon:Option<(HWND,u32)>,
}

unsafe fn tray_account(pid:u32,paths:&[String],log:&mut TrayLog)->Option<TrayAccount>{
    let hosts=tray_hosts(paths);
    let mut owners:Vec<_>=hosts.iter().map(|&(p,_)|p).collect();
    owners.sort_unstable();owners.dedup();
    let mine=account_tray_pid(pid,&owners,&parent_processes())?;
    let icons=host_icons(&hosts,mine);
    log.step(format!("闪动进程 {pid} 对应托盘进程 {mine}，共 {} 个托盘进程，目标图标 {} 个",owners.len(),icons.len()));
    let icon=match icons.as_slice(){[]=>None,[one]=>Some(*one),_=>return None};
    Some(TrayAccount{pid:mine,hosts,icon})
}

/// 系統匣按鈕的名字是不是微信（排除「微信輸入法」等）。
fn wechat_tray_name(name:&str)->bool{
    let first=name.lines().next().unwrap_or("").trim();
    let lower=first.to_ascii_lowercase();
    (first.starts_with("微信") && !first.starts_with("微信输入法")) || lower.starts_with("wechat") || lower.starts_with("weixin")
}

/// 只有其他每個帳號都唯一對應到不同按鈕，且僅剩目標按鈕時，才允許排除法。
fn remaining_tray_button(buttons:&[windows::Win32::Foundation::RECT],others:&[Option<windows::Win32::Foundation::RECT>])->Option<usize>{
    if buttons.len()!=others.len()+1{return None;}
    let mut assigned=vec![false;buttons.len()];
    for icon in others {
        let icon=(*icon)?;
        let mut hits=buttons.iter().enumerate().filter(|(_,r)|rect_center_inside(icon,**r));
        let (index,_)=hits.next()?;
        if hits.next().is_some() || assigned[index]{return None;}
        assigned[index]=true;
    }
    assigned.iter().position(|used|!*used)
}

/// 該帳號能切換過去的主視窗：先找閃動程序自己，再找它的微信祖先程序。
unsafe fn account_main_window(pid:u32,source:crate::app::Messenger,paths:&[String])->Option<HWND>{
    if pid==0 {return None;}
    if let Some(window)=visible_main_window_of(pid,source,paths){return Some(window);}
    let owners:Vec<_>=tray_hosts(paths).into_iter().map(|(p,_)|p).collect::<std::collections::HashSet<_>>().into_iter().collect();
    let owner=account_tray_pid(pid,&owners,&parent_processes())?;
    visible_main_window_of(owner,source,paths)
}

/// 該帳號縮在系統匣（沒有可見主視窗，但有系統匣圖示）時返回 Ok；否則返回給使用者的提示，並記錄診斷。
pub fn wechat_in_tray(unread:crate::app::Unread)->Result<(),String>{
    if unread.source!=crate::app::Messenger::Wechat {return Err("请从任务栏或托盘打开微信".into());}
    let paths=client_paths(unread.source);
    let mut log=TrayLog::new(unread);
    unsafe{
        // 雙開時可能查不到本帳號圖示的位置，此時由喚起流程用排除法定位，這裡只要求找到它的系統匣視窗。
        let Some(account)=tray_account(unread.pid,&paths,&mut log) else {return Err(log.fail("T1","未找到该账号的微信托盘窗口"));};
        let pid=account.pid;
        if let Some(main)=visible_main_window_of(pid,unread.source,&paths).or_else(||account_main_window(unread.pid,unread.source,&paths)){
            return Err(log.fail("T0",&format!("微信主窗口仍可见（{} {:#x}），但未能切到前台",class_name(main),main.0 as usize)));
        }
    }
    Ok(())
}

fn rect_center_inside(inner:windows::Win32::Foundation::RECT,outer:windows::Win32::Foundation::RECT)->bool{
    let (x,y)=((inner.left+inner.right)/2,(inner.top+inner.bottom)/2);
    outer.right>outer.left && outer.bottom>outer.top && x>=outer.left && x<outer.right && y>=outer.top && y<outer.bottom
}

/// 覆蓋圖示中心、大小與系統匣按鈕相當的控制元件（排除整條工作列之類的容器）。
fn button_sized(icon:windows::Win32::Foundation::RECT,button:windows::Win32::Foundation::RECT)->bool{
    let (iw,ih)=((icon.right-icon.left).max(16),(icon.bottom-icon.top).max(16));
    rect_center_inside(icon,button) && button.right-button.left<=iw*3 && button.bottom-button.top<=ih*3
}

fn show_rect(r:windows::Win32::Foundation::RECT)->String{format!("({},{})-({},{})",r.left,r.top,r.right,r.bottom)}

fn window_rect(hwnd:HWND)->windows::Win32::Foundation::RECT{
    let mut r=windows::Win32::Foundation::RECT::default();
    unsafe{let _=GetWindowRect(hwnd,&mut r);}
    r
}

/// 系統設定了「自動隱藏工作列」。
fn taskbar_autohide()->bool{
    use windows::Win32::UI::Shell::{SHAppBarMessage,ABM_GETSTATE,APPBARDATA};
    const ABS_AUTOHIDE:usize=1;
    let mut data=APPBARDATA{cbSize:std::mem::size_of::<APPBARDATA>() as u32,..Default::default()};
    let state=unsafe{SHAppBarMessage(ABM_GETSTATE,&mut data)};
    state & ABS_AUTOHIDE!=0
}

/// 工作列是否完整落在它所在的顯示器內（自動隱藏縮回時只露出一兩個畫素）。
fn taskbar_fully_shown(taskbar:HWND)->bool{
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW,MonitorFromWindow,MONITORINFO,MONITOR_DEFAULTTONEAREST};
    let r=window_rect(taskbar);
    let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>() as u32,..Default::default()};
    if !unsafe{GetMonitorInfoW(MonitorFromWindow(taskbar,MONITOR_DEFAULTTONEAREST),&mut info)}.as_bool(){return false;}
    let m=info.rcMonitor;
    r.right>r.left && r.bottom>r.top && r.left>=m.left && r.top>=m.top && r.right<=m.right && r.bottom<=m.bottom
}

/// 讓自動隱藏的工作列彈出來並等它滑完（最多約 1 秒）。先按系統方式切到工作列，不行再由無障礙介面設定焦點。
unsafe fn reveal_taskbar(ui:&windows::Win32::UI::Accessibility::IUIAutomation,taskbar:HWND)->bool{
    if taskbar_fully_shown(taskbar){return true;}
    let wait=||{for _ in 0..20 {if taskbar_fully_shown(taskbar){std::thread::sleep(Duration::from_millis(120));return true;} std::thread::sleep(Duration::from_millis(50));} false};
    if SetForegroundWindow(taskbar).as_bool() && wait(){return true;}
    if let Ok(element)=ui.ElementFromHandle(taskbar){let _=element.SetFocus();}
    wait()
}

unsafe fn tray_overflow_button(ui:&windows::Win32::UI::Accessibility::IUIAutomation,taskbar:HWND)->Option<windows::Win32::UI::Accessibility::IUIAutomationElement>{
    use windows::Win32::UI::Accessibility::*;
    let usable=|item:&IUIAutomationElement|item.CurrentIsEnabled().is_ok_and(|b|b.as_bool())
        && item.CurrentIsOffscreen().is_ok_and(|b|!b.as_bool())
        && item.CurrentNativeWindowHandle().map_or(true,|h|h.0.is_null() || IsWindowVisible(h).as_bool())
        && item.CurrentBoundingRectangle().is_ok_and(|r|r.right>r.left && r.bottom>r.top && r.right-r.left<=200 && r.bottom-r.top<=200)
        && item.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId).is_ok();
    let legacy=FindWindowExW(taskbar,None,w!("TrayNotifyWnd"),PCWSTR::null()).ok()
        .and_then(|tray|FindWindowExW(tray,None,w!("Button"),PCWSTR::null()).ok())
        .and_then(|button|ui.ElementFromHandle(button).ok());
    if let Some(button)=legacy.filter(usable){return Some(button);}
    let items=ui.ElementFromHandle(taskbar).ok()?.FindAll(TreeScope_Descendants,&ui.CreateTrueCondition().ok()?).ok()?;
    let mut found=None;
    for i in 0..items.Length().ok()?{
        let Ok(item)=items.GetElement(i) else{continue;};
        if item.CurrentControlType().ok()!=Some(UIA_ButtonControlTypeId)
            || !item.CurrentName().is_ok_and(|name|matches!(name.to_string().trim().to_ascii_lowercase().as_str(),"显示隐藏的图标"|"顯示隱藏的圖示"|"show hidden icons"))
            || !usable(&item){continue;}
        // 多個不同的摺疊按鈕時不猜，避免開啟錯誤的通知區域。
        if let Some(previous)=&found{if !ui.CompareElements(previous,&item).ok()?.as_bool(){return None;}}
        found=Some(item);
    }
    found
}

/// 微信縮在系統匣時：透過無障礙介面按下該帳號自己的系統匣按鈕，由微信自己喚起主視窗。
/// 不移動滑鼠、不向微信傳送內部訊息、不注入、不讀取內容；定位不到唯一按鈕就放棄。
/// 相容新版 Win11 系統匣與 Windows 10 / 早期 Win11 的舊式系統匣。會阻塞幾百毫秒，必須在後臺執行緒呼叫。
pub fn open_wechat_from_tray(unread:crate::app::Unread)->Result<(),String>{
    use windows::{core::Interface,Win32::{Foundation::RECT,System::Com::*,UI::{Accessibility::*,HiDpi::*}}};
    let mut log=TrayLog::new(unread);
    if unread.source!=crate::app::Messenger::Wechat {return Err(log.fail("T1","不是微信提醒"));}
    let paths=client_paths(unread.source);
    unsafe {
        let _=SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _=CoInitializeEx(None,COINIT_MULTITHREADED);
        let Some(account)=tray_account(unread.pid,&paths,&mut log) else {return Err(log.fail("T1","未找到该账号的微信托盘窗口"));};
        let Ok(ui)=CoCreateInstance::<_,IUIAutomation>(&CUIAutomation8,None,CLSCTX_INPROC_SERVER) else {return Err(log.fail("T2","无法创建系统无障碍接口"));};
        // 新系統的工作列較複雜，800ms 可能來不及遍歷完。
        if let Ok(timed)=ui.cast::<IUIAutomation2>(){let _=timed.SetConnectionTimeout(2500);let _=timed.SetTransactionTimeout(2500);}
        let Ok(all)=ui.CreateTrueCondition() else {return Err(log.fail("T2","无障碍查询条件创建失败"));};
        let describe_element=|item:&IUIAutomationElement|format!("{} · {} · {}{}",
            item.CurrentAutomationId().map(|s|s.to_string()).unwrap_or_default(),item.CurrentClassName().map(|s|s.to_string()).unwrap_or_default(),
            item.CurrentBoundingRectangle().map(show_rect).unwrap_or_default(),
            if item.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId).is_ok(){" · 可按下"}else{""});
        // 在某個視窗裡找覆蓋圖示中心、大小像系統匣按鈕、可以按下的最小控制元件。沒找到時記下掃描情況和覆蓋該點的控制元件，便於排查新系統。
        let button_at=|window:HWND,icon:RECT,log:&mut TrayLog|->Option<IUIAutomationElement>{
            let started=Instant::now();
            let items=match ui.ElementFromHandle(window).and_then(|root|root.FindAll(TreeScope_Descendants,&all)) {
                Ok(items)=>items,
                Err(error)=>{log.step(format!("  {} {:#x}：无障碍查询失败 {:#x}",class_name(window),window.0 as usize,error.code().0));return None;}
            };
            let count=items.Length().unwrap_or(0);
            let mut best:Option<(i64,IUIAutomationElement)>=None;
            let mut covering=Vec::new();
            for i in 0..count {
                let Ok(item)=items.GetElement(i) else {continue};
                if !item.CurrentIsEnabled().is_ok_and(|b|b.as_bool()) || !item.CurrentIsOffscreen().is_ok_and(|b|!b.as_bool()){continue;}
                let Ok(r)=item.CurrentBoundingRectangle() else {continue};
                if !rect_center_inside(icon,r) {continue;}
                if covering.len()<8 {covering.push(describe_element(&item));}
                if item.CurrentClassName().is_ok_and(|c|c.to_string().starts_with("Taskbar.TaskList")){continue;}
                if !button_sized(icon,r) || item.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId).is_err() {continue;}
                let area=i64::from(r.right-r.left)*i64::from(r.bottom-r.top);
                if best.as_ref().is_none_or(|(old,_)|area<*old){best=Some((area,item));}
            }
            match &best {
                Some((_,item))=>log.step(format!("命中按钮：{}",describe_element(item))),
                None=>log.step(format!("  {} {:#x}：扫描 {count} 个控件用时 {}ms，覆盖图标中心的：{}",class_name(window),window.0 as usize,started.elapsed().as_millis(),if covering.is_empty(){"无".into()}else{covering.join("；")})),
            }
            best.map(|(_,item)|item)
        };
        // 工作列上找不到時，再找資源管理器名下其他覆蓋該點的可見視窗（新系統可能把系統匣放在單獨的視窗裡）。
        let button_on_shell=|taskbar:HWND,icon:RECT,log:&mut TrayLog|->Option<IUIAutomationElement>{
            if let Some(button)=button_at(taskbar,icon,log){return Some(button);}
            use windows::Win32::Foundation::BOOL;
            struct Find{pid:u32,taskbar:HWND,center:windows::Win32::Foundation::POINT,found:Vec<HWND>}
            unsafe extern "system" fn visit(hwnd:HWND,data:LPARAM)->BOOL{
                let find=&mut *(data.0 as *mut Find);
                let mut r=RECT::default();
                if hwnd!=find.taskbar && window_pid(hwnd)==find.pid && IsWindowVisible(hwnd).as_bool() && GetWindowRect(hwnd,&mut r).is_ok()
                    && find.center.x>=r.left && find.center.x<r.right && find.center.y>=r.top && find.center.y<r.bottom {find.found.push(hwnd);}
                true.into()
            }
            let center=windows::Win32::Foundation::POINT{x:(icon.left+icon.right)/2,y:(icon.top+icon.bottom)/2};
            let mut find=Find{pid:window_pid(taskbar),taskbar,center,found:Vec::new()};
            let _=EnumWindows(Some(visit),LPARAM((&mut find as *mut Find) as isize));
            log.step(format!("资源管理器覆盖该点的其他窗口：{}",if find.found.is_empty(){"无".into()}else{find.found.iter().map(|h|format!("{} {:#x}",class_name(*h),h.0 as usize)).collect::<Vec<_>>().join("，")}));
            find.found.into_iter().take(4).find_map(|window|button_at(window,icon,log))
        };
        let invoke=|element:&IUIAutomationElement|->bool{
            element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId).and_then(|p|p.Invoke()).is_ok()
        };
        let still_over_icon=|button:&IUIAutomationElement,host:HWND,id:u32|
            tray_rect(host,id).zip(button.CurrentBoundingRectangle().ok()).is_some_and(|(r,b)|button_sized(r,b));
        let confirmed=|pid:u32,log:&mut TrayLog|{
            for _ in 0..25{
                if foreground_main_pid(unread.source,&paths)==Some(pid){return Ok(());}
                std::thread::sleep(Duration::from_millis(80));
            }
            Err(log.fail("T5","未确认目标账号的主窗口已到前台"))
        };
        // 新版 Win11 與舊式（Windows 10 / 早期 Win11）的摺疊區視窗。
        let overflow_open=||[w!("TopLevelWindowForOverflowXamlIsland"),w!("NotifyIconOverflowWindow")].into_iter()
            .find_map(|class|FindWindowW(class,PCWSTR::null()).ok().filter(|h|IsWindowVisible(*h).as_bool()));
        let Ok(taskbar)=FindWindowW(w!("Shell_TrayWnd"),PCWSTR::null()) else {return Err(log.fail("T3","找不到任务栏"));};
        // 工作列自動隱藏時縮在螢幕外，無障礙樹裡沒有系統匣按鈕：先讓它彈出來（與滑鼠移到螢幕邊緣相同），
        // 微信切到前臺後工作列會自己縮回去。
        if taskbar_autohide() {
            let before=window_rect(taskbar);
            let shown=reveal_taskbar(&ui,taskbar);
            log.step(format!("任务栏自动隐藏：弹出前 {} → 弹出后 {}（{}）",show_rect(before),show_rect(window_rect(taskbar)),if shown{"已完整显示"}else{"未能完整显示"}));
        }
        let Some((host,id))=account.icon else {
            let by_exclusion=||->Option<IUIAutomationElement>{
                let owners:std::collections::HashSet<_>=account.hosts.iter().map(|(pid,_)|*pid).filter(|p|*p!=account.pid).collect();
                let other_positions=||owners.iter().map(|pid|{
                    let icons=host_icons(&account.hosts,*pid);
                    match icons.as_slice(){[(h,id)]=>tray_rect(*h,*id),_=>None}
                }).collect::<Vec<_>>();
                // 只掃描通知區域，排除已固定的微信工作列按鈕。
                let mut buttons=Vec::new();
                let tray=FindWindowExW(taskbar,None,w!("TrayNotifyWnd"),PCWSTR::null()).ok();
                for (window,legacy) in [(Some(taskbar),false),(tray,true),(overflow_open(),true)] {
                    let Some(window)=window else{continue;};
                    let Ok(items)=ui.ElementFromHandle(window).and_then(|root|root.FindAll(TreeScope_Descendants,&all)) else{continue;};
                    for i in 0..items.Length().unwrap_or(0){
                        let Ok(item)=items.GetElement(i) else{continue;};
                        if !legacy && !item.CurrentClassName().is_ok_and(|s|s.to_string()=="SystemTray.NormalButton"){continue;}
                        if !item.CurrentName().is_ok_and(|s|wechat_tray_name(&s.to_string())){continue;}
                        let Ok(r)=item.CurrentBoundingRectangle() else{continue;};
                        if r.right<=r.left || r.bottom<=r.top || r.right-r.left>200 || r.bottom-r.top>200 || item.CurrentIsOffscreen().map_or(true,|b|b.as_bool())
                            || !item.CurrentIsEnabled().is_ok_and(|b|b.as_bool())
                            || item.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId).is_err(){continue;}
                        if !buttons.iter().any(|(_,old):&(RECT,IUIAutomationElement)|ui.CompareElements(old,&item).is_ok_and(|same|same.as_bool())){buttons.push((r,item));}
                    }
                }
                let rects=buttons.iter().map(|(r,_)|*r).collect::<Vec<_>>();
                let index=remaining_tray_button(&rects,&other_positions())?;
                if tray_hosts(&paths)!=account.hosts || buttons.iter().any(|(rect,b)|b.CurrentBoundingRectangle().map_or(true,|r|r!=*rect))
                    || remaining_tray_button(&rects,&other_positions())!=Some(index){
                    return None;
                }
                Some(buttons.swap_remove(index).1)
            };
            let mut target=by_exclusion();
            let mut opened=None;
            if target.is_none() && overflow_open().is_none(){
                let chevron=tray_overflow_button(&ui,taskbar);
                if let Some(button)=chevron.filter(|b|invoke(b)){
                    opened=Some(button);
                    for _ in 0..12{
                        std::thread::sleep(Duration::from_millis(80));
                        let Some(overflow)=overflow_open() else{continue;};
                        match host_icons(&account.hosts,account.pid).as_slice(){
                            [(h,id)]=>if let Some(r)=tray_rect(*h,*id){target=button_at(overflow,r,&mut log).filter(|b|still_over_icon(b,*h,*id));},
                            []=>target=by_exclusion(),
                            _=>break,
                        }
                        if target.is_some(){break;}
                    }
                }
            }
            let pressed=target.as_ref().is_some_and(invoke);
            if !pressed{
                if overflow_open().is_some(){if let Some(button)=opened{let _=invoke(&button);}}
                return Err(log.fail("T1","无法唯一确认该账号的托盘按钮，请手动打开微信"));
            }
            return confirmed(account.pid,&mut log);
        };
        let Some(icon)=tray_rect(host,id) else {return Err(log.fail("T1","系统未报告托盘图标位置"));};
        log.step(format!("图标位置：{}",show_rect(icon)));
        let was_open=overflow_open().is_some();
        if let Some(overflow)=overflow_open(){
            if let Some(button)=button_at(overflow,icon,&mut log){
                if !still_over_icon(&button,host,id) || !invoke(&button){return Err(log.fail("T5","折叠区图标已变化，请重试"));}
                return confirmed(account.pid,&mut log);
            }
        }
        let chevron:IUIAutomationElement;
        let from_chevron:bool;
        if let Some(button)=button_on_shell(taskbar,icon,&mut log) {
            if !still_over_icon(&button,host,id) || !invoke(&button){return Err(log.fail("T5","任务栏按钮已变化或无法按下"));}
            std::thread::sleep(Duration::from_millis(250));
            // 按下後摺疊區展開了：說明圖示收在摺疊區，命中的是「顯示隱藏的圖示」（新版 Win11 會報告這個位置）。
            if overflow_open().is_none() {return confirmed(account.pid,&mut log);}
            log.step("命中的是折叠按钮，继续在折叠区里找");
            chevron=button;from_chevron=true;
        } else {
            // 圖示可能仍報告收起時的位置；兩種識別路徑都透過系統摺疊按鈕展開後重新定位。
            let Some(element)=tray_overflow_button(&ui,taskbar) else {return Err(log.fail("T3","任务栏上找不到微信托盘按钮或隐藏图标按钮"));};
            log.step(format!("任务栏上未命中；折叠按钮：{}",describe_element(&element)));
            if !was_open && !invoke(&element){return Err(log.fail("T5","折叠按钮无法按下"));}
            chevron=element;from_chevron=false;
        }
        let started=Instant::now();
        let mut pressed=false;
        while started.elapsed()<Duration::from_millis(1500) {
            std::thread::sleep(Duration::from_millis(80));
            let Some(overflow)=overflow_open() else {continue};
            // 摺疊區展開後，系統報告的才是圖示在摺疊區裡的真實位置；多開時據此區分帳號。
            let Some(inside)=tray_rect(host,id).filter(|r|!from_chevron || !rect_center_inside(*r,icon)) else {continue};
            if let Some(button)=button_at(overflow,inside,&mut log){
                log.step(format!("折叠区 {} 图标位置：{}",class_name(overflow),show_rect(inside)));
                if still_over_icon(&button,host,id) && invoke(&button){pressed=true;break;}
            }
        }
        if pressed {return confirmed(account.pid,&mut log);}
        // 沒按成：把展開的摺疊區收回去，不留在螢幕上。
        if !was_open && overflow_open().is_some(){let _=invoke(&chevron);}
        Err(log.fail(if started.elapsed()>=Duration::from_millis(1500){"T4"}else{"T5"},"折叠区里找不到或按不下微信托盘按钮"))
    }
}

struct State {
    message: u32,
    taskbar_created: u32,
    source:crate::messenger::Messenger,
    registered: bool,
    paths: Vec<String>,
    /// 各帳號獨立合併閃動；確認檢視後立即重置。
    burst: Vec<(u32,Burst)>,
    /// 尚未取走的提醒：帳號程序與閃動的視窗。
    pending: Vec<(u32,isize)>,
    ctx: eframe::egui::Context,
}

impl State {
    fn attention(&mut self,pid:u32,window:isize,now:Instant)->bool{
        if !self.burst.iter().any(|(account,_)|*account==pid){self.burst.push((pid,Burst::default()));}
        if !self.burst.iter_mut().find(|(account,_)|*account==pid).unwrap().1.attention(now,window){return false;}
        match self.pending.iter_mut().find(|(account,_)|*account==pid){Some(entry)=>entry.1=window,None=>self.pending.push((pid,window))}
        true
    }
    fn clear(&mut self,pid:u32){
        self.burst.retain(|(account,_)|*account!=pid);
        self.pending.retain(|(account,_)|*account!=pid);
    }
}

unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM, _: usize, data: usize) -> LRESULT {
    {
        let slot=&*(data as *const RefCell<State>);
        if let Ok(mut state)=slot.try_borrow_mut(){
            if message==state.taskbar_created && message!=0 {
                state.registered=RegisterShellHookWindow(hwnd).as_bool();
                state.ctx.request_repaint();
            }
            if message==state.message && matches!(wp.0, FLASH | 4 | 0x8004) {
                let target=HWND(lp.0 as *mut _);
                let foreground=foreground_main_pid(state.source,&state.paths);
                let ours=matches(target,&state.paths);
                if ours {
                    client_trace(state.source,format!("{} 目标 {}·进程 {}·{} 前台 {}·客户端主窗口进程 {foreground:?}",match wp.0{FLASH=>"闪动",4=>"激活",_=>"激活(rude)"},describe(target),window_pid(target),state.source.name(),describe(GetForegroundWindow())));
                }
                if ours{
                    let source_pid=window_pid(target);
                    let office=if state.source!=crate::messenger::Messenger::Wechat && wp.0==FLASH{crate::messenger::desktop::flash_target(state.source,target,&state.paths)}else{None};
                    let pid=if state.source==crate::messenger::Messenger::Wechat{account_pid(source_pid,&state.paths)}else{office.map_or(source_pid,|unread|unread.pid)};
                    if foreground==Some(pid){
                        if state.pending.iter().any(|(p,_)|*p==pid){client_trace(state.source,format!("  → 账号 {pid} 在前台，清掉待取提醒"));}
                        // 正在看這個帳號：它的提醒作廢，其它帳號不受影響。
                        state.clear(pid);
                    }else if wp.0==FLASH {
                        let target=office.map_or(target,|unread|HWND(unread.window as *mut _));
                        let queued=state.attention(pid,target.0 as isize,Instant::now());
                        client_trace(state.source,format!("  → {}",if queued{"提交提醒或更新窗口"}else{"合并同一轮闪动"}));
                        if queued{state.ctx.request_repaint();}
                    }
                }
            }
        }
    }
    if message==WM_NCDESTROY {
        let _=DeregisterShellHookWindow(hwnd);
        let _=RemoveWindowSubclass(hwnd,Some(subclass),SUBCLASS_ID);
    }
    DefSubclassProc(hwnd,message,wp,lp)
}

fn client_trace(source:crate::messenger::Messenger,line:String){
    if source==crate::messenger::Messenger::Wechat{trace(line);}else{crate::messenger::windows::trace(format!("{} {line}",source.name()));}
}

/// 視窗的簡短描述：控制代碼、類名、可見 / 最小化。不讀標題。
fn describe(hwnd:HWND)->String{
    if hwnd.0.is_null(){return "无".into();}
    unsafe{format!("{:#x} {} {}{}",hwnd.0 as usize,class_name(hwnd),if IsWindowVisible(hwnd).as_bool(){"可见"}else{"隐藏"},if IsIconic(hwnd).as_bool(){"·最小化"}else{""})}
}

/// 微信提醒事件記錄：只保留最近 300 條，寫到 diagnostics/wechat-events.log；不記標題和聊天內容。
pub fn trace(line:impl Into<String>){
    static LOG:std::sync::Mutex<std::collections::VecDeque<String>>=std::sync::Mutex::new(std::collections::VecDeque::new());
    let Ok(mut log)=LOG.lock() else {return};
    log.push_back(format!("{} {}",chrono::Local::now().format("%H:%M:%S%.3f"),line.into()));
    while log.len()>300 {log.pop_front();}
    let directory=crate::model::config_dir().join("diagnostics");
    if std::fs::create_dir_all(&directory).is_ok(){let _=std::fs::write(directory.join("wechat-events.log"),log.iter().cloned().collect::<Vec<_>>().join("\n")+"\n");}
}

/// 島仔自己是否以管理員身份執行（寫進診斷記錄）。
fn process_elevated()->bool{
    use windows::Win32::{Security::{GetTokenInformation,TokenElevation,TOKEN_ELEVATION,TOKEN_QUERY},System::Threading::{GetCurrentProcess,OpenProcessToken}};
    unsafe{
        let mut token=windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&mut token).is_err(){return false;}
        let mut elevation=TOKEN_ELEVATION::default();let mut size=0u32;
        let ok=GetTokenInformation(token,TokenElevation,Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),std::mem::size_of::<TOKEN_ELEVATION>() as u32,&mut size).is_ok();
        let _=CloseHandle(token);
        ok && elevation.TokenIsElevated!=0
    }
}

pub struct Listener { hwnd: HWND, state: Box<RefCell<State>> }
impl Listener {
    pub fn new(ctx: eframe::egui::Context) -> Result<Self,String> {
        let result=Self::start(ctx,crate::messenger::Messenger::Wechat);
        // 每次啟動監聽都記一條：日誌裡沒有這一行，就說明監聽從未啟動。
        let elevated=if process_elevated(){"是"}else{"否"};
        match &result {
            Ok(listener)=>trace(format!("监听已启动 · 岛仔管理员权限={elevated} · 微信安装路径：{}",listener.state.borrow().paths.iter().filter(|p|!p.starts_with('*')).cloned().collect::<Vec<_>>().join("，"))),
            Err(error)=>trace(format!("监听启动失败：{error} · 岛仔管理员权限={elevated}")),
        }
        result
    }
    pub(crate) fn for_client(source:crate::messenger::Messenger,ctx:eframe::egui::Context)->Result<Self,String>{Self::start(ctx,source)}
    fn start(ctx: eframe::egui::Context,source:crate::messenger::Messenger) -> Result<Self,String> {
        let paths=client_paths(source);
        if paths.is_empty(){return Err(format!("未找到{}安装路径，请安装后重新开启提醒",source.name()));}
        unsafe {
            let message=RegisterWindowMessageW(w!("SHELLHOOK"));
            if message==0{return Err(format!("无法注册{}提醒事件",source.name()));}
            let taskbar_created=RegisterWindowMessageW(w!("TaskbarCreated"));
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!("DaozaiWechatListener"),WS_POPUP,0,0,0,0,None,None,None,None).map_err(|_|format!("无法创建{}监听窗口",source.name()))?;
            // 島仔以管理員身份執行時，系統會攔下資源管理器發來的視窗事件；顯式允許這兩條訊息。
            for allowed in [message,taskbar_created] {
                let _=ChangeWindowMessageFilterEx(hwnd,allowed,MSGFLT_ALLOW,None);
            }
            let state=Box::new(RefCell::new(State{message,taskbar_created,source,registered:true,paths,burst:Vec::new(),pending:Vec::new(),ctx}));
            let listener=Self{hwnd,state};
            SetWindowSubclass(hwnd,Some(subclass),SUBCLASS_ID,(&*listener.state as *const RefCell<State>) as usize).ok().map_err(|_|format!("无法接入{}提醒事件",source.name()))?;
            RegisterShellHookWindow(hwnd).ok().map_err(|_|format!("无法监听{}任务栏提醒",source.name()))?;
            Ok(listener)
        }
    }
    pub fn registered(&self)->bool{self.state.borrow().registered}
    /// 取走新提醒：每項是（帳號程序，閃動的視窗）。
    pub fn take(&self) -> Vec<(u32,isize)> {std::mem::take(&mut self.state.borrow_mut().pending)}
    pub fn forget(&self,pid:u32){
        self.state.borrow_mut().clear(pid);
    }
}
impl Drop for Listener {
    fn drop(&mut self){unsafe{
        let _=DeregisterShellHookWindow(self.hwnd);
        let _=RemoveWindowSubclass(self.hwnd,Some(subclass),SUBCLASS_ID);
        let _=DestroyWindow(self.hwnd);
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
#[cfg(feature="cn-messengers")]
    fn all_taskbar_clients_first_flash_after_minimizing_a_viewed_account_is_not_swallowed(){unsafe{
        use crate::messenger::Messenger;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();let pid=std::process::id();
        for (source,class) in [(Messenger::Wechat,w!("WeChatMainWndForPC")),(Messenger::WeCom,w!("WeWorkWindow")),(Messenger::Feishu,w!("Chrome_WidgetWin_1")),(Messenger::DingTalk,w!("Qt51511QWindowIcon")),(Messenger::DingTalk,w!("StandardFrame_DingTalk"))]{
            RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:class,..Default::default()});
            let window=CreateWindowExW(WINDOW_EX_STYLE(0),class,w!("Daozai synthetic client"),WS_OVERLAPPEDWINDOW,-10000,-10000,400,500,None,None,instance,None).unwrap();
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,0,0,0,0,None,None,None,None).unwrap();
            let message=RegisterWindowMessageW(w!("SHELLHOOK"));
            let state=Box::new(RefCell::new(State{message,taskbar_created:RegisterWindowMessageW(w!("TaskbarCreated")),source,registered:true,paths:vec![normalized(&std::env::current_exe().unwrap().to_string_lossy())],burst:Vec::new(),pending:Vec::new(),ctx:Default::default()}));
            SetWindowSubclass(hwnd,Some(subclass),SUBCLASS_ID,(&*state as *const RefCell<State>)as usize).ok().unwrap();
            let listener=Listener{hwnd,state};
            listener.forget(pid);
            // 只向本測試的監聽視窗模擬事件；隱藏的合成主視窗代表已從前臺收起。
            SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(window.0 as isize));let first=listener.take();
            SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(window.0 as isize));let repeated=listener.take();
            DestroyWindow(window).unwrap();drop(listener);
            assert_eq!(first,[(pid,window.0 as isize)],"{} 收起后的第一轮消息不能被查看后的宽限期吞掉",source.name());
            assert!(repeated.is_empty(),"同一轮持续闪动仍只提醒一次");
        }
    }}
    #[test]
    fn office_shell_events_preserve_unknown_instances_until_a_new_unambiguous_event(){unsafe{
        use crate::messenger::Messenger;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();let pid=std::process::id();
        for (source,class) in [(Messenger::WeCom,w!("WeWorkWindow")),(Messenger::Feishu,w!("Chrome_WidgetWin_1")),(Messenger::DingTalk,w!("Qt51511QWindowIcon"))]{
            RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:class,..Default::default()});
            let create=||CreateWindowExW(WINDOW_EX_STYLE(0),class,w!("Daozai synthetic client"),WS_OVERLAPPEDWINDOW,-10000,-10000,400,500,None,None,instance,None).unwrap();
            let (first,second)=(create(),create());
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,0,0,0,0,None,None,None,None).unwrap();
            let message=RegisterWindowMessageW(w!("SHELLHOOK"));
            let state=Box::new(RefCell::new(State{message,taskbar_created:RegisterWindowMessageW(w!("TaskbarCreated")),source,registered:true,paths:vec![normalized(&std::env::current_exe().unwrap().to_string_lossy())],burst:Vec::new(),pending:Vec::new(),ctx:Default::default()}));
            SetWindowSubclass(hwnd,Some(subclass),SUBCLASS_ID,(&*state as *const RefCell<State>)as usize).ok().unwrap();
            let listener=Listener{hwnd,state};
            // 模擬訊息只發給本測試自己的監聽視窗，不傳送給任何真實客戶端。
            SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(first.0 as isize));
            assert_eq!(listener.take(),[(0,0)]);
            SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(first.0 as isize));assert!(listener.take().is_empty());
            listener.forget(0);DestroyWindow(second).unwrap();
            SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(first.0 as isize));
            assert_eq!(listener.take(),[(pid,first.0 as isize)],"只能让新的确定事件产生新的已知提醒");
            DestroyWindow(first).unwrap();drop(listener);
        }
    }}
    use crate::messenger::windows::open_unread_with_paths;
    #[test]
    fn clicking_wechat_flash_never_shows_hidden_helper(){unsafe{
        let window=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,0,0,0,0,None,None,None,None).unwrap();
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let unread=crate::app::Unread{source:crate::app::Messenger::Wechat,pid:std::process::id(),window:window.0 as isize};
        let result=open_unread_with_paths(unread,&paths);
        let visible=IsWindowVisible(window).as_bool();
        DestroyWindow(window).unwrap();
        assert!(!visible && result.is_err(),"点击提醒不应显示隐藏的辅助窗口");
    }}
    #[test]
    fn clicking_wechat_flash_never_shows_hidden_qt_frame(){unsafe{
        // 復現 v0.6.60 白屏：微信 4 隱藏的 Qt 視窗帶系統選單 / 最小化按鈕，過去會被 SW_SHOW 顯示出來。
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();
        let class=WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:w!("Qt51514QWindowIcon"),..Default::default()};
        RegisterClassW(&class);
        let style=WINDOW_STYLE(0xC60B0000 & !WS_CHILD.0);
        let window=CreateWindowExW(WINDOW_EX_STYLE(0),w!("Qt51514QWindowIcon"),w!("Weixin"),style,0,0,300,200,None,None,instance,None).unwrap();
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let pid=std::process::id();
        let flashed=open_unread_with_paths(crate::app::Unread{source:crate::app::Messenger::Wechat,pid,window:window.0 as isize},&paths);
        let unknown=open_unread_with_paths(crate::app::Unread{source:crate::app::Messenger::Wechat,pid:0,window:window.0 as isize},&paths);
        let visible=IsWindowVisible(window).as_bool();
        DestroyWindow(window).unwrap();
        assert!(!visible,"隐藏的微信窗口不得被显示");
        assert!(flashed.is_err() && unknown.is_err());
    }}
    #[test]
    #[ignore="需要本机已启动的微信；会把微信主窗口切到前台"]
    fn local_wechat_hidden_windows_stay_hidden_when_opened(){unsafe{
        use windows::Win32::Foundation::BOOL;
        unsafe extern "system" fn collect(hwnd:HWND,data:LPARAM)->BOOL{
            let (paths,out)=&mut *(data.0 as *mut (Vec<String>,Vec<(HWND,bool)>));
            if matches(hwnd,paths){out.push((hwnd,IsWindowVisible(hwnd).as_bool()));}
            true.into()
        }
        let mut found=(installed_paths(),Vec::new());
        let _=EnumWindows(Some(collect),LPARAM((&mut found as *mut (Vec<String>,Vec<(HWND,bool)>)) as isize));
        assert!(!found.1.is_empty(),"本机没有微信窗口");
        for &(hwnd,_) in &found.1 {
            let unread=crate::app::Unread{source:crate::app::Messenger::Wechat,pid:window_pid(hwnd),window:hwnd.0 as isize};
            let result=open_unread_with_paths(unread,&found.0);
            std::thread::sleep(Duration::from_millis(300));
            for &(other,was_visible) in &found.1 {
                assert!(was_visible || !IsWindowVisible(other).as_bool(),"隐藏窗口 {other:?} 被显示（目标 {hwnd:?}，结果 {result:?}）");
            }
            println!("目标 {hwnd:?} 可见={} → {result:?}",found.1.iter().any(|(h,v)|*h==hwnd && *v));
        }
    }}
    #[test]
    fn tray_button_names_identify_wechat_only(){
        for name in ["微信","微信 - 2 条新消息","WeChat","Weixin\n有新消息"] {assert!(wechat_tray_name(name),"{name}");}
        for name in ["微信输入法","任务栏输入指示 单击右键查看微信输入法设置","企业微信","QQ: 12345",""] {assert!(!wechat_tray_name(name),"{name}");}
    }
    #[test]
    fn dual_open_flashes_map_to_their_own_tray(){
        use std::collections::HashMap;
        // 兩個帳號：主號 100（子程序 101、102），備用號 200（子程序 201），都由啟動器 50 拉起。
        let parents:HashMap<u32,u32>=[(100,50),(101,100),(102,101),(200,50),(201,200),(50,4)].into_iter().collect();
        let hosts=[100,200];
        assert_eq!(account_tray_pid(100,&hosts,&parents),Some(100));
        assert_eq!(account_tray_pid(102,&hosts,&parents),Some(100),"主号子进程的闪动对应主号托盘");
        assert_eq!(account_tray_pid(201,&hosts,&parents),Some(200),"备用号子进程的闪动对应备用号托盘");
        assert_eq!(account_tray_pid(50,&hosts,&parents),None,"启动器下有两个账号时分不清，不得猜");
        assert_eq!(account_tray_pid(0,&hosts,&parents),None);
        // 備用號由主號拉起：備用號子程序取最近的祖先，不串到主號。
        let nested:HashMap<u32,u32>=[(100,50),(200,100),(201,200)].into_iter().collect();
        assert_eq!(account_tray_pid(201,&hosts,&nested),Some(200));
        // 系統匣在閃動程序的子程序裡（只有一個時才用）。
        assert_eq!(account_tray_pid(100,&[300],&[(300,100)].into_iter().collect()),Some(300));
        // 父程序表裡有環也不會卡住。
        let cycle:HashMap<u32,u32>=[(1,2),(2,1)].into_iter().collect();
        assert_eq!(account_tray_pid(1,&[9],&cycle),None);
    }
    #[test]
    fn multi_account_tray_requires_one_to_one_evidence(){
        use windows::Win32::Foundation::RECT;
        let rect=|x|RECT{left:x,top:0,right:x+20,bottom:20};
        for n in 1..=5{
            let buttons=(0..n).map(|i|rect(i*24)).collect::<Vec<_>>();
            for target in 0..buttons.len(){
                let others=buttons.iter().enumerate().filter(|(i,_)|*i!=target).map(|(_,r)|Some(*r)).collect::<Vec<_>>();
                assert_eq!(remaining_tray_button(&buttons,&others),Some(target));
                if !others.is_empty(){let mut missing=others.clone();missing[0]=None;assert_eq!(remaining_tray_button(&buttons,&missing),None);}
            }
        }
        assert_eq!(remaining_tray_button(&[rect(0),rect(24),rect(48)],&[Some(rect(0)),Some(rect(0))]),None);
        let parents=[(101,100),(201,200),(301,300),(401,400),(100,50),(200,100),(300,50),(400,50)].into_iter().collect();
        for owner in [100,200,300,400]{assert_eq!(account_tray_pid(owner+1,&[100,200,300,400],&parents),Some(owner));}
        assert_eq!(account_tray_pid(50,&[100,200,300,400],&parents),None);
    }
    #[test]
    fn offscreen_wechat_frame_is_not_treated_as_open(){unsafe{
        // 復現雙開主號：有一個「可見」但不在任何顯示器上的微信視窗，過去會被當成主視窗「切換」過去，使用者什麼也看不到。
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();
        let class=WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:w!("Qt51514QWindowIcon"),..Default::default()};
        RegisterClassW(&class);
        let style=WS_POPUP|WS_SYSMENU|WS_MINIMIZEBOX|WS_CAPTION;
        let window=CreateWindowExW(WINDOW_EX_STYLE(0),w!("Qt51514QWindowIcon"),w!("Weixin"),style,-30000,-30000,900,700,None,None,instance,None).unwrap();
        let _=ShowWindow(window,SW_SHOWNOACTIVATE);
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let pid=std::process::id();
        let qualifies=visible_main_frame(window,crate::app::Messenger::Wechat);
        let usable=usable_main_frame(window,crate::app::Messenger::Wechat);
        let found=visible_main_window_of(pid,crate::app::Messenger::Wechat,&paths);
        let result=open_unread_with_paths(crate::app::Unread{source:crate::app::Messenger::Wechat,pid,window:window.0 as isize},&paths);
        DestroyWindow(window).unwrap();
        assert!(qualifies,"测试窗口应当像微信主窗口");
        assert!(!usable && found.is_none(),"屏幕外的窗口不算用户看得到的主窗口");
        assert!(result.is_err(),"不得假装切换成功");
    }}
    #[test]
    fn process_tree_is_read_from_the_system(){
        let parents=parent_processes();
        assert!(parents.contains_key(&std::process::id()));
        assert!(!ancestors(std::process::id(),&parents).is_empty(),"测试进程应有父进程");
    }
    struct TrayFixture {window:HWND,thread:Option<std::thread::JoinHandle<()>>,clicks:std::sync::Arc<std::sync::atomic::AtomicUsize>}
    impl TrayFixture {
        fn new(legacy:bool,buttons:&[(&str,bool,bool)])->Self{
            use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
            let buttons=buttons.iter().map(|(name,enabled,visible)|(name.to_string(),*enabled,*visible)).collect::<Vec<_>>();
            let clicks=Arc::new(AtomicUsize::new(0));let received=clicks.clone();
            let (tx,rx)=std::sync::mpsc::channel();
            let thread=std::thread::spawn(move||unsafe{
                use windows::Win32::{Foundation::COLORREF,System::LibraryLoader::GetModuleHandleW,UI::Input::KeyboardAndMouse::EnableWindow};
                unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{
                    if m==WM_COMMAND {
                        let clicks=GetWindowLongPtrW(h,GWLP_USERDATA) as *const AtomicUsize;
                        if !clicks.is_null(){(*clicks).fetch_add(1,Ordering::SeqCst);}
                    }
                    if m==WM_DESTROY{PostQuitMessage(0);}
                    DefWindowProcW(h,m,w,l)
                }
                unsafe extern "system" fn child_proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
                let instance=GetModuleHandleW(None).unwrap().into();
                RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:w!("DaozaiTrayFixture"),..Default::default()});
                RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(child_proc),hInstance:instance,lpszClassName:w!("TrayNotifyWnd"),..Default::default()});
                // 透明視窗只提供系統無障礙控制元件，不顯示介面，也不搶佔前臺。
                let window=CreateWindowExW(WS_EX_LAYERED|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW,w!("DaozaiTrayFixture"),w!(""),WS_POPUP,10,10,240,220,None,None,instance,None).unwrap();
                SetLayeredWindowAttributes(window,COLORREF(0),0,LWA_ALPHA).unwrap();
                SetWindowLongPtrW(window,GWLP_USERDATA,Arc::as_ptr(&received) as isize);
                let parent=if legacy{CreateWindowExW(WINDOW_EX_STYLE(0),w!("TrayNotifyWnd"),w!(""),WS_CHILD|WS_VISIBLE,0,0,240,220,window,None,instance,None).unwrap()}else{window};
                for (i,(name,enabled,visible)) in buttons.iter().enumerate(){
                    let style=WS_CHILD|if *visible{WS_VISIBLE}else{WINDOW_STYLE(0)};
                    let button=CreateWindowExW(WINDOW_EX_STYLE(0),w!("Button"),&windows::core::HSTRING::from(name),style,4,4+i as i32*40,200,32,parent,None,instance,None).unwrap();
                    let _=EnableWindow(button,*enabled);
                }
                let _=ShowWindow(window,SW_SHOWNOACTIVATE);tx.send(window.0 as isize).unwrap();
                let mut message=MSG::default();while GetMessageW(&mut message,None,0,0).0>0{let _=TranslateMessage(&message);DispatchMessageW(&message);}
            });
            Self{window:HWND(rx.recv_timeout(Duration::from_secs(5)).unwrap() as *mut _),thread:Some(thread),clicks}
        }
    }
    impl Drop for TrayFixture{fn drop(&mut self){unsafe{let _=PostMessageW(self.window,WM_CLOSE,WPARAM(0),LPARAM(0));}if let Some(thread)=self.thread.take(){thread.join().unwrap();}}}

    #[test]
    fn tray_overflow_button_supports_legacy_and_modern_layouts(){unsafe{
        use windows::Win32::{System::Com::*,UI::Accessibility::*};
        let _=CoInitializeEx(None,COINIT_MULTITHREADED);
        let ui:IUIAutomation=CoCreateInstance(&CUIAutomation8,None,CLSCTX_INPROC_SERVER).unwrap();
        for legacy in [true,false]{
            let fixture=TrayFixture::new(legacy,&[("显示隐藏的图标",true,true)]);
            let button=tray_overflow_button(&ui,fixture.window).unwrap_or_else(||panic!("未找到折叠按钮，旧式布局={legacy}"));
            assert_eq!(button.CurrentName().unwrap().to_string(),"显示隐藏的图标");
            button.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId).unwrap().Invoke().unwrap();
            if !legacy{
                let until=Instant::now()+Duration::from_secs(2);
                while fixture.clicks.load(std::sync::atomic::Ordering::SeqCst)==0 && Instant::now()<until{std::thread::sleep(Duration::from_millis(10));}
                assert_eq!(fixture.clicks.load(std::sync::atomic::Ordering::SeqCst),1);
            }
        }
    }}

    #[test]
    fn tray_overflow_button_rejects_unusable_or_ambiguous_controls(){unsafe{
        use windows::Win32::{System::Com::*,UI::Accessibility::*};
        let _=CoInitializeEx(None,COINIT_MULTITHREADED);
        let ui:IUIAutomation=CoCreateInstance(&CUIAutomation8,None,CLSCTX_INPROC_SERVER).unwrap();
        for legacy in [true,false]{
            for buttons in [vec![("显示隐藏的图标",false,true)],vec![("显示隐藏的图标",true,false)]]{
                let fixture=TrayFixture::new(legacy,&buttons);
                assert!(tray_overflow_button(&ui,fixture.window).is_none(),"不得点击不可用的按钮：{buttons:?}");
            }
        }
        for buttons in [vec![("微信",true,true)],vec![("显示隐藏的图标",true,true),("Show hidden icons",true,true)]]{
            let fixture=TrayFixture::new(false,&buttons);
            assert!(tray_overflow_button(&ui,fixture.window).is_none(),"不得点击不确定的按钮：{buttons:?}");
        }
        for name in ["Show hidden icons","顯示隱藏的圖示"]{
            let fixture=TrayFixture::new(false,&[(name,true,true)]);
            assert!(tray_overflow_button(&ui,fixture.window).is_some(),"未识别系统语言：{name}");
        }
    }}

    #[test]
    fn tray_overflow_lookup_recovers_after_taskbar_reveal_without_cached_coordinates(){unsafe{
        use windows::Win32::{Foundation::RECT,Graphics::Gdi::*,System::Com::*,UI::Accessibility::*};
        let _=CoInitializeEx(None,COINIT_MULTITHREADED);
        let ui:IUIAutomation=CoCreateInstance(&CUIAutomation8,None,CLSCTX_INPROC_SERVER).unwrap();
        let fixture=TrayFixture::new(false,&[("显示隐藏的图标",true,true)]);
        assert!(taskbar_fully_shown(fixture.window));
        let rect=window_rect(fixture.window);
        let mut monitor=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>() as u32,..Default::default()};
        GetMonitorInfoW(MonitorFromWindow(fixture.window,MONITOR_DEFAULTTONEAREST),&mut monitor).ok().unwrap();
        SetWindowPos(fixture.window,None,rect.left,monitor.rcMonitor.bottom-1,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE).unwrap();
        assert!(!taskbar_fully_shown(fixture.window),"仅露出一个像素时应等待任务栏展开");
        SetWindowPos(fixture.window,None,rect.left,rect.top,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE).unwrap();
        assert!(taskbar_fully_shown(fixture.window));
        let button=tray_overflow_button(&ui,fixture.window).expect("任务栏展开后应重新查找新版折叠按钮");
        let bounds=button.CurrentBoundingRectangle().unwrap();
        let reported=RECT{left:17,top:1631,right:57,bottom:1671};
        assert!(!button_sized(reported,bounds),"用户日志中的旧坐标不应命中新按钮");
        assert!(button_sized(bounds,bounds));
    }}

    #[test]
    #[ignore="只读检查本机任务栏，需要通知区域含有隐藏图标"]
    fn local_tray_overflow_button_is_discoverable(){unsafe{
        use windows::Win32::{System::Com::*,UI::Accessibility::*};
        let _=CoInitializeEx(None,COINIT_MULTITHREADED);
        let ui:IUIAutomation=CoCreateInstance(&CUIAutomation8,None,CLSCTX_INPROC_SERVER).unwrap();
        let taskbar=FindWindowW(w!("Shell_TrayWnd"),PCWSTR::null()).unwrap();
        let button=tray_overflow_button(&ui,taskbar).expect("未找到本机隐藏图标按钮");
        println!("本机折叠按钮：{} · {}",button.CurrentClassName().unwrap(),show_rect(button.CurrentBoundingRectangle().unwrap()));
    }}

    #[test]
    fn tray_buttons_match_by_icon_center(){
        use windows::Win32::Foundation::RECT;
        let icon=RECT{left:1531,top:1017,right:1581,bottom:1067};
        assert!(rect_center_inside(icon,RECT{left:1532,top:1018,right:1582,bottom:1068}));
        assert!(!rect_center_inside(icon,RECT{left:1582,top:1018,right:1632,bottom:1068}),"相邻图标不得命中");
        assert!(!rect_center_inside(icon,RECT::default()),"空矩形不得命中");
        // 舊式系統匣按鈕比圖示略大；整條工作列這類容器不算按鈕。
        assert!(button_sized(RECT{left:1600,top:1050,right:1616,bottom:1066},RECT{left:1596,top:1044,right:1620,bottom:1074}));
        assert!(!button_sized(icon,RECT{left:0,top:1140,right:1920,bottom:1200}));
    }
    #[test]
    fn tray_open_rejects_unknown_accounts(){
        // 非微信程序的閃動不得退到本機微信系統匣（本機裝了微信時也一樣）。
        assert!(open_wechat_from_tray(crate::app::Unread{source:crate::app::Messenger::Wechat,pid:std::process::id(),window:0}).is_err());
        assert!(wechat_in_tray(crate::app::Unread{source:crate::app::Messenger::Wechat,pid:std::process::id(),window:0}).is_err());
        assert!(open_wechat_from_tray(crate::app::Unread{source:crate::app::Messenger::Qq,pid:1,window:0}).is_err());
        assert!(wechat_in_tray(crate::app::Unread{source:crate::app::Messenger::Qq,pid:std::process::id(),window:0}).is_err(),"QQ 不走微信托盘");
    }
    #[test]
    #[ignore="需要本机已登录的微信；会把微信关到托盘再唤起"]
    fn local_wechat_opens_from_tray(){unsafe{
        let paths=installed_paths();
        let pid=std::process::id();let _=pid;
        let main=[0u32].into_iter().find_map(|_|{
            use windows::Win32::Foundation::BOOL;
            unsafe extern "system" fn visit(hwnd:HWND,data:LPARAM)->BOOL{
                let (paths,out)=&mut *(data.0 as *mut (Vec<String>,Option<HWND>));
                if visible_main_frame(hwnd,crate::app::Messenger::Wechat) && matches(hwnd,paths){*out=Some(hwnd);return false.into();}
                true.into()
            }
            let mut found=(paths.clone(),None);
            let _=EnumWindows(Some(visit),LPARAM((&mut found as *mut (Vec<String>,Option<HWND>)) as isize));
            found.1
        });
        // 微信已在系統匣時直接從系統匣開始測；否則先把主視窗關到系統匣。
        let main=match main {
            Some(main)=>{PostMessageW(main,WM_SYSCOMMAND,WPARAM(SC_CLOSE as usize),LPARAM(0)).unwrap();std::thread::sleep(Duration::from_secs(2));main}
            None=>{
                let [(pid,_)]=tray_hosts(&paths)[..] else {panic!("先打开微信")};
                let mut hidden=None;
                unsafe extern "system" fn first(hwnd:HWND,data:LPARAM)->windows::Win32::Foundation::BOOL{
                    let (pid,out)=&mut *(data.0 as *mut (u32,Option<HWND>));
                    if window_pid(hwnd)==*pid && class_name(hwnd).ends_with("QWindowIcon") && GetWindowLongW(hwnd,GWL_STYLE) as u32 & WS_MINIMIZEBOX.0!=0 && GetWindowTextLengthW(hwnd)>0 {*out=Some(hwnd);return false.into();}
                    true.into()
                }
                let mut data=(pid,None);let _=EnumWindows(Some(first),LPARAM((&mut data as *mut (u32,Option<HWND>)) as isize));
                hidden=hidden.or(data.1);hidden.expect("找不到微信主窗口")
            }
        };
        let unread=crate::app::Unread{source:crate::app::Messenger::Wechat,pid:window_pid(main),window:main.0 as isize};
        println!("任务栏自动隐藏={}",taskbar_autohide());
        assert!(!IsWindowVisible(main).as_bool(),"微信应已缩到托盘");
        assert!(wechat_in_tray(unread).is_ok());
        assert!(open_unread_with_paths(unread,&paths).is_err(),"缩在托盘时普通跳转不碰任何窗口");
        let result=open_wechat_from_tray(unread);
        std::thread::sleep(Duration::from_secs(2));
        let overflow=FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"),PCWSTR::null()).ok().is_some_and(|h|IsWindowVisible(h).as_bool());
        println!("托盘唤起 → {result:?}；主窗口可见={}；前台={}；折叠区仍展开={overflow}",IsWindowVisible(main).as_bool(),GetForegroundWindow()==main);
        assert!(result.is_ok() && IsWindowVisible(main).as_bool() && !IsHungAppWindow(main).as_bool() && !overflow);
    }}
    #[test]
    fn capture_overlays_do_not_count_as_viewing_a_messenger(){
        use crate::app::Messenger;
        let main=(WS_SYSMENU|WS_MINIMIZEBOX).0;
        assert!(client_main_frame(Messenger::Qq,"Chrome_WidgetWin_1",main,0));
        assert!(client_main_frame(Messenger::Wechat,"Qt51514QWindowIcon",main,0));
        assert!(!client_main_frame(Messenger::Qq,"Chrome_WidgetWin_1",WS_POPUP.0,WS_EX_TOPMOST.0));
        assert!(!client_main_frame(Messenger::Wechat,"Qt51514QWindowIcon",WS_POPUP.0,0));
        assert!(!client_main_frame(Messenger::Wechat,"Screenshot",main,0));
    }
    #[test]
    fn weixin_qt_main_window_without_system_menu_counts_as_viewed(){
        use crate::app::Messenger;
        assert!(client_main_frame(Messenger::Wechat,"Qt51514QWindowIcon",0x96c70000,0x100));
        assert!(!client_main_frame(Messenger::Wechat,"Qt51514QWindowIcon",WS_POPUP.0,0x100));
    }
    #[test]
    fn qq_nt_main_window_without_system_menu_counts_as_viewed(){
        use crate::app::Messenger::Qq;
        assert!(client_main_frame(Qq,"Chrome_WidgetWin_1",0x14c70000,0x100));
        assert!(!client_main_frame(Qq,"Chrome_WidgetWin_1",0x04c20000,0x80108));
        assert!(!client_main_frame(Qq,"Chrome_WidgetWin_1",0x04030000,0x80000));
    }
    #[test]
    #[ignore="需要本机 QQ 主窗口实际位于前台"]
    fn local_qq_main_window_is_viewed(){
        assert!(client_in_foreground(crate::app::Messenger::Qq));
    }
    #[test]
    fn protocol_registration_requires_an_absolute_wechat_executable(){
        assert_eq!(protocol_executable(r#"D:\Weixin\Weixin.exe "%1""#),Some(PathBuf::from(r"D:\Weixin\Weixin.exe")));
        assert_eq!(protocol_executable(r#""D:\Program Files\WeChat.exe" "%1""#),Some(PathBuf::from(r"D:\Program Files\WeChat.exe")));
        assert!(protocol_executable("Weixin.exe %1").is_none());
        assert!(protocol_executable(r#""C:\Windows\cmd.exe" /c Weixin.exe"#).is_none());
    }
    #[test]
    #[ignore="需要已启动并位于后台的微信窗口；仅向测试窗口发送模拟事件"]
    fn local_wechat_background_window(){unsafe{
        let target:isize=std::env::var("DAOZAI_TEST_WECHAT_HWND").unwrap().parse().unwrap();
        let listener=Listener::new(Default::default()).unwrap();
        let hwnd=listener.hwnd;
        let message=RegisterWindowMessageW(w!("SHELLHOOK"));
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(target));assert!(!listener.take().is_empty());
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(target));assert!(listener.take().is_empty());
        drop(listener);assert!(!IsWindow(hwnd).as_bool());
    }}
    #[test]
    fn shell_notifications_filter_targets_merge_flashes_and_detach(){unsafe{
        let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,0,0,0,0,None,None,None,None).unwrap();
        let message=RegisterWindowMessageW(w!("SHELLHOOK"));
        let state=Box::new(RefCell::new(State{message,taskbar_created:RegisterWindowMessageW(w!("TaskbarCreated")),source:crate::messenger::Messenger::Wechat,registered:true,paths:vec![],burst:Vec::new(),pending:Vec::new(),ctx:Default::default()}));
        SetWindowSubclass(hwnd,Some(subclass),SUBCLASS_ID,(&*state as *const RefCell<State>) as usize).ok().unwrap();
        RegisterShellHookWindow(hwnd).ok().unwrap();
        let listener=Listener{hwnd,state};
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(hwnd.0 as isize));
        assert!(listener.take().is_empty(),"其他程序的闪动不得触发微信提醒");
        listener.state.borrow_mut().paths=vec![normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(0));assert!(listener.take().is_empty());
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(hwnd.0 as isize));
        assert_eq!(listener.take(),[(std::process::id(),hwnd.0 as isize)],"提醒带上账号进程和闪动的窗口");
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(hwnd.0 as isize));assert!(listener.take().is_empty());
        SendMessageW(hwnd,message,WPARAM(4),LPARAM(hwnd.0 as isize));
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(hwnd.0 as isize));assert!(listener.take().is_empty());
        listener.forget(std::process::id());
        SendMessageW(hwnd,message,WPARAM(FLASH),LPARAM(hwnd.0 as isize));
        SendMessageW(hwnd,message,WPARAM(0x8004),LPARAM(hwnd.0 as isize));
        assert!(!listener.take().is_empty(),"后台窗口的激活事件应保留尚未消费的提醒");
        DeregisterShellHookWindow(hwnd).ok().unwrap();
        SendMessageW(hwnd,RegisterWindowMessageW(w!("TaskbarCreated")),WPARAM(0),LPARAM(0));
        assert!(listener.registered());
        drop(listener);assert!(!IsWindow(hwnd).as_bool());
    }}
    #[test]
    fn viewed_accounts_rearm_immediately_without_resetting_other_accounts(){
        let mut state=State{message:0,taskbar_created:0,source:crate::messenger::Messenger::Wechat,registered:false,paths:vec![],burst:vec![],pending:vec![],ctx:Default::default()};
        let start=Instant::now();
        assert!(state.attention(100,11,start));assert!(state.attention(200,22,start));assert!(state.attention(0,0,start));
        state.pending.clear();
        state.clear(0);
        assert!(!state.attention(100,11,start+Duration::from_millis(100)),"关闭未知账号不能重置其他账号的闪动合并");
        assert!(!state.attention(200,22,start+Duration::from_millis(100)));
        assert!(state.attention(0,0,start+Duration::from_millis(100)));
        state.clear(100);assert!(state.attention(100,11,start+Duration::from_millis(200)),"确认查看后，下一轮立即可提醒");
        assert!(!state.attention(200,22,start+Duration::from_millis(200)));
        assert!(state.attention(200,23,start+Duration::from_millis(300)),"窗口变化必须及时传给界面更新唤起目标");
        assert_eq!(state.burst.iter().filter(|(pid,_)|*pid==200).count(),1);
    }
    #[test]
    fn wechat_is_recognized_by_tencent_file_name_when_registry_path_is_stale(){
        // 登錄檔裡的舊路徑對不上時，仍按「檔名 + 騰訊釋出」識別；其它公司的同名程式不算。
        let stale=vec![normalized(r"C:\Old\Weixin\Weixin.exe"),"*weixin.exe".to_string()];
        let me=normalized(&std::env::current_exe().unwrap().to_string_lossy());
        assert!(!path_allowed(&me,&stale),"测试程序不是腾讯发布的微信");
        assert!(path_allowed(&normalized(r"C:\Old\Weixin\Weixin.exe"),&stale),"注册表路径仍直接认");
        assert!(!path_allowed(&me,&[]));
        // 本機裝了微信時，用它的真實程式驗證：路徑列表裡只有過期路徑也能認出來。
        if let Some(real)=installed_paths().into_iter().find(|p|!p.starts_with('*')){
            assert!(company_name(&real).to_lowercase().contains("tencent"),"本机微信的公司名：{}",company_name(&real));
            assert!(path_allowed(&real,&stale));
            assert!(!path_allowed(&real,&[normalized(r"C:\Old\Weixin\Weixin.exe"),"*qq.exe".into()]),"文件名不对不得认作其它客户端");
        }
    }
    #[test]
    fn paths_match_full_install_location(){
        assert_eq!(normalized(r"\\?\D:\Apps\Weixin.exe"),normalized("d:/apps/weixin.exe"));
        assert_ne!(normalized(r"D:\Apps\Weixin.exe"),normalized(r"D:\Other\Weixin.exe"));
    }
}
