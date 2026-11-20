//! 網頁版 YouTube Music 的辨識。
//!
//! 瀏覽器分頁在系統媒體會話（SMTC）裡只有來源「Chrome」，專輯常常是空的，也沒有網址，
//! 光看會話分不出它是 YouTube Music 還是別的網站的影片，所以改看瀏覽器本身：
//!
//! 1. 先看瀏覽器視窗標題（只含目前顯示的分頁，不用任何自動化介面）；
//! 2. 找不到才用 UI Automation 讀分頁列上每個分頁的名稱（含背景分頁）。
//!    只在分頁列底下找 `TabItem`，不進入網頁內容，也不讀網頁內容。
//!
//! 掃描在背景執行緒進行，結果依「來源＋歌名＋歌手」快取；查不到時退避重試（1、2、4…最多 60 秒），
//! 不會一直打擾瀏覽器。YouTube Music 的分頁標題形如「晴天 | YouTube Music」，只要某個分頁以這首歌的名字
//! 開頭、後面接 YouTube Music，就認定這個會話是 YouTube Music。
use std::{collections::HashMap,sync::Mutex,time::{Duration,Instant}};

use windows::{core::Interface,Win32::{Foundation::*,System::Com::*,UI::{Accessibility::*,WindowsAndMessaging::*}}};

/// 會讀分頁的 Chromium 系瀏覽器；Electron 應用程式的視窗類別相同，所以還要核對執行檔名。
const BROWSERS:&[&str]=&["chrome.exe","msedge.exe","brave.exe","opera.exe","vivaldi.exe"];
/// 單一瀏覽器視窗最多讀多少個節點、往下幾層；分頁列在 BrowserView 底下約 11 層。
const MAX_NODES:usize=600;
const MAX_DEPTH:usize=14;
const CACHE_LIMIT:usize=16;
/// 新歌第一次比對時最多等掃描多久；超過就先當成不是，等背景掃描把結果寫進快取。
const FIRST_WAIT:Duration=Duration::from_millis(150);

/// 分頁或視窗的名稱是不是「這首歌」的 YouTube Music 頁面。
///
/// 視窗標題後面會接「 - Google Chrome」，分頁名稱後面可能接「 - 音訊播放中 - 高記憶體用量 - 1.0 GB」，
/// 有通知時前面還會有「(2) 」；歌名後面必須緊接分隔符與 YouTube Music，
/// 所以「晴天 (Live) | YouTube Music」不會被當成「晴天」。
pub(crate) fn names_ytmusic_page(name:&str,song:&str)->bool {
    let (name,song)=(squash(name),squash(song));
    if song.is_empty(){return false;}
    let mut rest=name.as_str();
    if let Some((count,after))=rest.strip_prefix('(').and_then(|inner|inner.split_once(')')) {
        if !count.is_empty() && count.bytes().all(|b|b.is_ascii_digit()){rest=after.trim_start();}
    }
    let Some(after)=rest.strip_prefix(song.as_str()) else {return false};
    let Some(after)=after.trim_start().strip_prefix(['|','-']) else {return false};
    after.trim_start().get(.."YouTube Music".len()).is_some_and(|head|head.eq_ignore_ascii_case("YouTube Music"))
}

/// 分頁標題常夾著不換行空白（U+00A0）或連續空白，比對前一律壓成單一空格。
fn squash(text:&str)->String {text.split_whitespace().collect::<Vec<_>>().join(" ")}

enum Verdict {Yes,Not{attempts:u32,retry_at:Instant}}
#[derive(Debug,PartialEq,Eq)]
enum Next {Yes,Wait,Scan}

/// 掃描結果快取與退避。純邏輯，不碰系統，方便單元測試。
#[derive(Default)]
struct Verdicts(HashMap<String,Verdict>);
impl Verdicts {
    fn next(&self,key:&str,now:Instant)->Next {
        match self.0.get(key) {
            Some(Verdict::Yes)=>Next::Yes,
            Some(Verdict::Not{retry_at,..}) if now<*retry_at=>Next::Wait,
            _=>Next::Scan,
        }
    }
    fn record(&mut self,key:String,found:bool,now:Instant) {
        if !self.0.contains_key(&key) && self.0.len()>=CACHE_LIMIT{self.0.clear();}
        let verdict=if found {Verdict::Yes} else {
            let attempts=match self.0.get(&key){Some(Verdict::Not{attempts,..})=>*attempts,_=>0};
            Verdict::Not{attempts:attempts+1,retry_at:now+Duration::from_secs((1u64<<attempts.min(6)).min(60))}
        };
        self.0.insert(key,verdict);
    }
}

static VERDICTS:Mutex<Option<Verdicts>>=Mutex::new(None);

fn with_verdicts<T>(run:impl FnOnce(&mut Verdicts)->T)->T {
    let mut guard=VERDICTS.lock().unwrap_or_else(|e|e.into_inner());
    run(guard.get_or_insert_with(Verdicts::default))
}

/// 這個瀏覽器會話是不是正在播放網頁版 YouTube Music。
///
/// 掃描在背景執行緒進行。換歌後第一次呼叫最多等 [`FIRST_WAIT`]（實測掃描約 20～70 毫秒，通常等得到），
/// 讓島上不會在換歌的瞬間閃一下；等不到就先回傳 false，結果寫進快取，下一次輪詢拿到。
pub(crate) fn is_ytmusic_page(source:&str,song:&str,artist:&str)->bool {
    if song.trim().is_empty(){return false;}
    let key=format!("{}|{song}|{artist}",source.to_ascii_lowercase());
    match with_verdicts(|verdicts|verdicts.next(&key,Instant::now())) {
        Next::Yes=>return true,
        Next::Wait=>return false,
        Next::Scan=>{}
    }
    // 通道上限是 1：已有掃描在跑時直接放棄這次，等它跑完。
    let song=song.to_string();
    let (reply,result)=std::sync::mpsc::channel();
    let started=crate::resources::BROWSER_TABS.spawn(move||{
        let com=unsafe{CoInitializeEx(None,COINIT_MULTITHREADED)}.is_ok();
        let found=std::panic::catch_unwind(||scan(&song)).unwrap_or(false);
        if com{unsafe{CoUninitialize();}}
        with_verdicts(|verdicts|verdicts.record(key,found,Instant::now()));
        let _=reply.send(found);
    }).is_some();
    started && result.recv_timeout(FIRST_WAIT).unwrap_or(false)
}

fn scan(song:&str)->bool {
    let windows=browser_windows();
    if windows.iter().any(|(_,title)|names_ytmusic_page(title,song)){return true;}
    let Some(ui)=automation() else {return false};
    windows.iter().any(|(hwnd,_)|tab_names(&ui,*hwnd).iter().any(|name|names_ytmusic_page(name,song)))
}

/// 可見的 Chromium 系瀏覽器頂層視窗與其標題。
fn browser_windows()->Vec<(HWND,String)> {
    unsafe extern "system" fn visit(hwnd:HWND,param:LPARAM)->BOOL {
        let found=&mut *(param.0 as *mut Vec<(HWND,String)>);
        if !IsWindowVisible(hwnd).as_bool(){return BOOL(1);}
        let mut class=[0u16;64];let n=GetClassNameW(hwnd,&mut class).max(0) as usize;
        if String::from_utf16_lossy(&class[..n])!="Chrome_WidgetWin_1"{return BOOL(1);}
        let mut pid=0;GetWindowThreadProcessId(hwnd,Some(&mut pid));
        let Some(path)=super::player_process::image_path(pid) else {return BOOL(1)};
        let exe=path.rsplit(['\\','/']).next().unwrap_or(&path);
        if !BROWSERS.iter().any(|browser|exe.eq_ignore_ascii_case(browser)){return BOOL(1);}
        let mut title=[0u16;512];let n=GetWindowTextW(hwnd,&mut title).max(0) as usize;
        found.push((hwnd,String::from_utf16_lossy(&title[..n])));
        BOOL(1)
    }
    let mut found=Vec::new();
    let _=unsafe{EnumWindows(Some(visit),LPARAM(&mut found as *mut Vec<(HWND,String)> as isize))};
    found
}

fn automation()->Option<IUIAutomation> {
    unsafe {
        let ui:IUIAutomation=CoCreateInstance(&CUIAutomation8,None,CLSCTX_INPROC_SERVER).ok()?;
        if let Ok(timed)=ui.cast::<IUIAutomation2>(){let _=timed.SetConnectionTimeout(800);let _=timed.SetTransactionTimeout(800);}
        Some(ui)
    }
}

/// 網頁內容的容器：不往裡面走，免得觸發瀏覽器為網頁開啟無障礙功能，也不讀網頁上的任何文字。
fn is_page_content(class:&str)->bool {class.contains("WebView") || class.contains("RenderWidgetHost")}

/// 逐層（廣度優先）往下找 `TabItem` 並讀名稱；遇到網頁內容就停，節點與層數都有上限。
fn tab_names(ui:&IUIAutomation,window:HWND)->Vec<String> {
    let mut names=Vec::new();
    unsafe {
        let (Ok(root),Ok(all))=(ui.ElementFromHandle(window),ui.CreateTrueCondition()) else {return names};
        let mut level=vec![root];let mut visited=0usize;
        for _ in 0..MAX_DEPTH {
            let mut next=Vec::new();
            for element in &level {
                let Ok(children)=element.FindAll(TreeScope_Children,&all) else {continue};
                for index in 0..children.Length().unwrap_or(0) {
                    let Ok(child)=children.GetElement(index) else {continue};
                    visited+=1;
                    if visited>MAX_NODES{return names;}
                    let kind=child.CurrentControlType().unwrap_or_default();
                    if kind==UIA_TabItemControlTypeId {
                        if let Ok(name)=child.CurrentName(){names.push(name.to_string());}
                        continue;
                    }
                    let class=child.CurrentClassName().map(|class|class.to_string()).unwrap_or_default();
                    if kind==UIA_DocumentControlTypeId || is_page_content(&class){continue;}
                    next.push(child);
                }
            }
            if next.is_empty(){break;}
            level=next;
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_real_tab_and_window_names_of_the_song_that_is_playing() {
        for name in [
            // 這台機器上實際讀到的分頁名稱與視窗標題。
            "晴天 | YouTube Music - 音訊播放中 - 高記憶體用量 - 1.0 GB",
            "晴天 | YouTube Music - Google Chrome",
            "(2) 晴天 | YouTube Music - Google Chrome",
            "晴天 - YouTube Music",
            "晴天 | youtube music",
        ] {assert!(names_ytmusic_page(name,"晴天"),"{name}");}
        assert!(names_ytmusic_page("Sunny Day | YouTube Music - Audio playing - Microsoft Edge","Sunny Day"));
        assert!(names_ytmusic_page("晴天 | YouTube Music","  晴天 "),"歌名前後的空白不影響");
        // 實測分頁名稱裡出現過不換行空白；歌名內部的連續空白也要能對上。
        assert!(names_ytmusic_page("晴天\u{a0}|\u{a0}YouTube Music - 音訊播放中","晴天"));
        assert!(names_ytmusic_page("Love  Story | YouTube Music","Love Story"));
        assert!(names_ytmusic_page("Love Story | YouTube Music","Love\u{a0}Story"));
    }

    #[test]
    fn other_pages_and_other_songs_do_not_match() {
        for name in [
            "(1595) YouTube - 高記憶體用量 - 1.7 GB",
            "晴天 - YouTube",
            "晴天 (Live) | YouTube Music",
            "七里香 | YouTube Music - Google Chrome",
            "我愛晴天 | YouTube Music",
            "B1_2x3_lite 完整硬體架構示意圖與逐 Block 詳解 - Google Chrome",
            "晴天 | YouTub",
            "晴天 |",
            "(abc) 晴天 | YouTube Music",
            "",
        ] {assert!(!names_ytmusic_page(name,"晴天"),"{name}");}
        assert!(!names_ytmusic_page("晴天 | YouTube Music",""),"沒有歌名就不比對");
        assert!(!names_ytmusic_page("晴天 | YouTube Music","  "));
    }

    #[test]
    fn multibyte_names_never_split_inside_a_character() {
        // 「YouTube Music」之後剛好是多位元組字元、名稱比 13 位元組短，都不能 panic。
        assert!(!names_ytmusic_page("晴天 | 晴天晴天晴天","晴天"));
        assert!(!names_ytmusic_page("晴天 | 你好","晴天"));
        assert!(!names_ytmusic_page("晴天 | YouTube Musi\u{4e2d}","晴天"));
    }

    #[test]
    fn scan_results_are_cached_and_retries_back_off() {
        let start=Instant::now();let mut verdicts=Verdicts::default();
        assert_eq!(verdicts.next("a",start),Next::Scan,"沒掃過就掃");
        verdicts.record("a".into(),false,start);
        assert_eq!(verdicts.next("a",start+Duration::from_millis(900)),Next::Wait);
        assert_eq!(verdicts.next("a",start+Duration::from_secs(1)),Next::Scan,"第一次退避 1 秒");
        verdicts.record("a".into(),false,start+Duration::from_secs(1));
        assert_eq!(verdicts.next("a",start+Duration::from_millis(2900)),Next::Wait);
        assert_eq!(verdicts.next("a",start+Duration::from_secs(3)),Next::Scan,"第二次退避 2 秒");
        let mut at=start+Duration::from_secs(3);
        for _ in 0..12 {verdicts.record("a".into(),false,at);at+=Duration::from_secs(60);}
        verdicts.record("a".into(),false,at);
        assert_eq!(verdicts.next("a",at+Duration::from_secs(59)),Next::Wait);
        assert_eq!(verdicts.next("a",at+Duration::from_secs(60)),Next::Scan,"退避上限 60 秒");
        verdicts.record("a".into(),true,at);
        assert_eq!(verdicts.next("a",at+Duration::from_secs(3600)),Next::Yes,"找到之後不再重掃");
        assert_eq!(verdicts.next("b",at),Next::Scan,"別首歌各自獨立");
    }

    #[test]
    fn cache_is_bounded() {
        let now=Instant::now();let mut verdicts=Verdicts::default();
        for index in 0..CACHE_LIMIT*3 {verdicts.record(format!("song{index}"),true,now);}
        assert!(verdicts.0.len()<=CACHE_LIMIT);
        assert_eq!(verdicts.next(&format!("song{}",CACHE_LIMIT*3-1),now),Next::Yes,"最新的一筆還在");
    }

    /// 走實際使用的路徑：第一次呼叫最多等 FIRST_WAIT，有結果就直接回 true，之後直接讀快取；
    /// 找不到的歌則在退避期間不再掃描。
    /// `DAOZAI_LIVE_SONG=歌名 cargo test --lib live_background -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_background_scan_resolves_without_blocking() {
        let song=std::env::var("DAOZAI_LIVE_SONG").expect("DAOZAI_LIVE_SONG");
        let first=Instant::now();
        let resolved=is_ytmusic_page("Chrome",&song,"live-test");
        println!("第一次呼叫 {:?}，結果 {resolved}",first.elapsed());
        assert!(first.elapsed()<FIRST_WAIT+Duration::from_millis(100),"第一次呼叫不能等太久：{:?}",first.elapsed());
        let started=Instant::now();
        while !resolved && !is_ytmusic_page("Chrome",&song,"live-test") {
            assert!(started.elapsed()<Duration::from_secs(5),"背景掃描 5 秒內沒有結果");
            std::thread::sleep(Duration::from_millis(20));
        }
        let cached=Instant::now();
        assert!(is_ytmusic_page("Chrome",&song,"live-test"));
        assert!(cached.elapsed()<Duration::from_millis(5),"有結果後不再掃描");
        // 不是 YouTube Music 的歌：不當成歌曲，退避期間立刻回傳、不再掃描。
        assert!(!is_ytmusic_page("Chrome","daozai-no-such-song","live-test"));
        let again=Instant::now();
        assert!(!is_ytmusic_page("Chrome","daozai-no-such-song","live-test"));
        assert!(again.elapsed()<Duration::from_millis(5),"退避期間不再掃描：{:?}",again.elapsed());
    }

    /// 需要真的開著 Chrome/Edge 並在分頁裡播放 YouTube Music：
    /// `DAOZAI_LIVE_SONG=晴天 cargo test --lib live_scan -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_scan_finds_the_open_ytmusic_tab() {
        let song=std::env::var("DAOZAI_LIVE_SONG").expect("DAOZAI_LIVE_SONG");
        let _com=unsafe{CoInitializeEx(None,COINIT_MULTITHREADED)};
        let started=Instant::now();
        let windows=browser_windows();
        println!("瀏覽器視窗 {} 個：{:?}",windows.len(),windows.iter().map(|(_,title)|title.as_str()).collect::<Vec<_>>());
        let ui=automation().expect("UI Automation");
        for (hwnd,_) in &windows {println!("分頁：{:#?}",tab_names(&ui,*hwnd));}
        println!("耗時 {:?}",started.elapsed());
        assert!(scan(&song),"找不到 {song}");
    }
}
