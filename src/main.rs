#![cfg_attr(windows, windows_subsystem = "windows")]
//! Windows 外殼：無邊框透明視窗 + 系統匣 + 系統資料，介面全部交給 `daozai::app::App`。

use eframe::egui::{self, Color32, Pos2};
use daozai::app::{self, App, Cmd, Env, Options};
use daozai::smtc;
#[cfg(windows)]
use daozai::messenger::{self,OpenError,TaskbarEvent};
use daozai::poll_clock::PollClock;
use std::time::{Duration, Instant};

mod diagnostics;
mod graphics;

struct Shell {
    app: App,
    t0: Instant,
    #[cfg(windows)]
    frame_at: Instant,
    /// 相容渲染上一次請求讀回畫面的時刻（無輸入的持續動畫按約 30 幀/秒讀回）。
    #[cfg(windows)]
    readback_at: Option<Instant>,
    media: smtc::Shared,
    seek_rx: Option<(&'static str,std::sync::mpsc::Receiver<bool>)>,
    media_control:Option<smtc::ControlWorker>,
    music_volume_rx:Option<(String,std::sync::mpsc::Receiver<bool>)>,
    _resource_monitor:Option<daozai::resources::Monitor>,
    resource_at:PollClock,
    guard_notified:bool,
    media_enabled: smtc::Poller,
    /// 剛點過播放控制或音量時，短時間內按最快頻率取樣，讓按鈕狀態及時回顯。
    media_fast_until: Option<Instant>,
    placed: bool,
    placement_at: PollClock,
    placement_scale: f32,
    /// 上次定位時的目標螢幕、水平位置與 OLED 畫素偏移；變化後立即重新定位，不等下一次輪詢。
    placement_key: (String, i32, daozai::model::Dock, bool, (i32, i32)),
    /// OLED 畫素偏移當前實際使用的位移；展開、拖拽、隱藏時保持不變。
    #[cfg(windows)]
    oled_shift: (i32, i32),
    win_x: f32,
    sys: (Option<u8>, bool, PollClock),
    #[cfg(windows)]
    system_state: daozai::system::Snapshot,
    #[cfg(windows)]
    system_ready: bool,
    #[cfg(windows)]
    system_rx: std::sync::mpsc::Receiver<daozai::system::Observation>,
    fullscreen: Option<daozai::display_layout::Bounds>,
    input_state:daozai::input::Monitor,
    #[cfg(windows)]
    input_keys:Option<daozai::input::Shortcuts>,
    #[cfg(windows)]
    incoming: Option<daozai::system::Notifications>,
    #[cfg(windows)]
    messengers:messenger::Runtime,
    #[cfg(windows)]
    unread_at:PollClock,
    #[cfg(windows)]
    opening_messenger:Option<(app::Unread,Instant)>,
    /// 後臺執行緒正在喚起訊息客戶端。
    #[cfg(windows)]
    tray_opening:Option<(app::Unread,std::sync::mpsc::Receiver<Result<(),OpenError>>)>,
    #[cfg(windows)]
    hidden: bool,
    #[cfg(windows)]
    fullscreen_covered: bool,
    /// 毛玻璃皮膚的背景亮度取樣（僅毛玻璃皮膚執行）。
    #[cfg(windows)]
    backdrop: Option<daozai::backdrop::Sampler>,
    #[cfg(windows)]
    native: Option<daozai::native::Window>,
    #[cfg(windows)]
    tray: Option<daozai::tray::Tray>,
    #[cfg(windows)]
    autostart: bool,
    /// v0.5.4：藍芽音訊裝置（後臺執行緒推送）與音量回聲的靜默視窗。
    #[cfg(windows)]
    bt_rx: Option<daozai::bt::Service>,
    bt: Vec<daozai::bt::Device>,
    bt_ready: bool,
    volume_quiet: PollClock,
    /// v0.5.5：郵件收信服務（工作執行緒），以及上次同步給它的帳號/開關快照。
    /// 信箱模組開啟時才存在；關閉後收信與整理執行緒隨之退出。
    mail: Option<daozai::mail::Service>,
    mail_sig: Option<(bool, u32, u8, Vec<daozai::mail::Account>)>,
    /// v0.5.6：日程（CalDAV）服務與上次下發的配置。
    /// 日程模組開啟時才存在。
    cal: Option<daozai::calendar::Service>,
    cal_sig: Option<(Vec<daozai::calendar::Config>, bool)>,
    /// 置物架模組開啟且記錄剪貼簿時才監聽系統剪貼簿。
    clipboard_watch: Option<daozai::clipboard::Watcher>,
    gws: daozai::gws::Service,
    quota: daozai::quota::Service,
    quota_sig: Option<(bool,Vec<daozai::quota::Account>)>,
    quota_at: PollClock,
}

const MODULE_OFF:&str="該功能已在「頁面管理」中關閉";

impl Shell {
    /// 按模組開關啟停後臺服務：關閉的模組不保留任何執行緒，重新開啟時從頭對齊配置。
    fn reconcile_modules(&mut self,ctx:&egui::Context){
        let mail_on=self.app.module_on(app::MAIL_PAGE);
        if mail_on!=self.mail.is_some() {self.mail=mail_on.then(daozai::mail::Service::start);self.mail_sig=None;}
        let cal_on=self.app.module_on(app::CALENDAR_PAGE);
        if cal_on!=self.cal.is_some() {self.cal=cal_on.then(daozai::calendar::Service::start);self.cal_sig=None;}
        let clipboard_on=self.app.clipboard_active();
        if clipboard_on!=self.clipboard_watch.is_some() {
            self.clipboard_watch=if clipboard_on {let ctx=ctx.clone();daozai::clipboard::watch(self.app.clipboard.clone(),move||ctx.request_repaint())}else{None};
        }
    }

    #[cfg(windows)]
    fn confirm_messenger_open(&mut self,unread:app::Unread)->bool{
        let foreground=self.messengers.foreground(&[unread.source]);
        if !self.app.confirm_messenger_open(unread,foreground){
            let seen=foreground.map(|(source,pid)|format!("{} 帳號 {pid}",source.name())).unwrap_or_else(messenger::windows::describe_foreground);
            messenger::windows::trace(format!("喚起 {} 帳號 {}：未到前臺（前臺是 {seen}）",unread.source.name(),unread.pid));
            return false;
        }
        messenger::windows::trace(format!("喚起 {} 帳號 {}：已在前臺",unread.source.name(),unread.pid));
        self.messengers.viewed_client(unread.source,unread.pid);
        true
    }

    #[cfg(windows)]
    fn clear_exited_messengers(&mut self,ctx:&egui::Context){
        for unread in self.messengers.exited(&self.app.pending_messengers){
            messenger::windows::trace(format!("清除已退出的{}提醒 帳號 {}",unread.source.name(),unread.pid));
            if self.tray_opening.as_ref().is_some_and(|(target,_)|*target==unread) || self.opening_messenger.is_some_and(|(target,_)|target==unread){
                self.app.messenger_unavailable(format!("{} 帳號已退出",unread.source.name()));
            }
            self.app.discard_unread(unread);
            ctx.request_repaint();
        }
        self.unread_at=PollClock::now();
    }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        diagnostics::graphics(cc);
        let running_compat=cc.wgpu_render_state.is_some();
        diagnostics::startup("載入本地配置");
        let mut app = App::new(&cc.egui_ctx, Options::default());
        app.render_compat_running=running_compat;
        #[cfg(feature="record")] let quota_demo=std::env::var("DAOZAI_QUOTA_DEMO").is_ok();
        #[cfg(not(feature="record"))] let quota_demo=false;
        diagnostics::startup("初始化額度配置");
        if !quota_demo{app.quota_initialize();}
        diagnostics::startup("啟動剪貼簿記錄");
        if std::env::args().any(|arg|arg=="--settings"){app.open_settings();}
        if std::env::args().any(|arg|arg=="--calendar"){app.open_calendar(None);}
        #[cfg(feature="record")]
        if std::env::var("DAOZAI_MAIL_DEMO").is_ok(){app.mail_demo();}
        #[cfg(feature="record")]
        if std::env::var("DAOZAI_CAL_DEMO").is_ok(){app.calendar_demo();}
        #[cfg(feature="record")]
        if let Ok(mode)=std::env::var("DAOZAI_QUOTA_DEMO"){app.quota_demo(&mode);}
        #[cfg(feature="record")]
        let gws_demo = quota_demo || std::env::var("DAOZAI_MAIL_DEMO").is_ok() || std::env::var("DAOZAI_CAL_DEMO").is_ok();
        #[cfg(not(feature="record"))]
        let gws_demo = false;
        let gws = daozai::gws::Service::start(cc.egui_ctx.clone());
        if !gws_demo {
            let has_gws_mail = app
                .mail_accounts()
                .iter()
                .any(|a| a.provider == daozai::mail::Provider::GoogleWorkspace);
            let has_gws_cal = app
                .calendar_configs()
                .iter()
                .any(|c| c.provider == daozai::calendar::Provider::GoogleWorkspace);
            if !app.settings.gws_onboarded && !(has_gws_mail && has_gws_cal) {
                app.settings.gws_onboarded = true;
                app.settings.save();
                app.gws_event(daozai::gws::Event {
                    phase: daozai::gws::Phase::Checking,
                    auto_bind: false,
                });
                gws.check(true, true);
            } else {
                gws.check(false, false);
            }
        }
        diagnostics::startup("初始化媒體和系統匣");
        let media = smtc::shared();
        let media_enabled=smtc::spawn_poller(media.clone(),cc.egui_ctx.clone());
        media_enabled.store(app.settings.visible_pages[0],std::sync::atomic::Ordering::Relaxed);
        #[cfg(windows)]
        let autostart = daozai::tray::autostart_enabled();
        #[cfg(windows)]
        let tray = app.start_tray(daozai::tray::State {
            pet: app.settings.pet,
            minimal: app.settings.minimal,
            autostart,
        });
        diagnostics::startup("初始化通知監聽");
        #[cfg(windows)]
        let incoming=if app.settings.read_notifications {match daozai::system::connect_with_settings(cc.egui_ctx.clone(),&app.settings){Ok(rx)=>Some(rx),Err(e)=>{app.listener_status=format!("Windows 無法讀取通知：{}",e.code());None}}}else{None};
        // 音訊、網路與輸入法查詢由後臺完成，避免首幀等待系統服務。
        diagnostics::startup("建立後臺取樣與視窗");
        let initial_input=None;
        #[cfg(windows)] let system_state=daozai::system::Snapshot::default();
        #[cfg(windows)] let system_rx=daozai::system::observe(cc.egui_ctx.clone(),system_state.clone(),initial_input);
        // 島體大小隻作用於主視窗，在首幀之前生效，避免啟動時先按 100% 閃一下。
        #[cfg(windows)] eframe::set_root_viewport_scale(app.settings.island_scale());
        Self {
            app,
            t0: Instant::now(),
            #[cfg(windows)]
            frame_at: Instant::now(),
            #[cfg(windows)]
            readback_at: None,
            media,
            seek_rx: None,
            media_control:None,
            music_volume_rx:None,
            _resource_monitor:daozai::resources::Monitor::start(daozai::model::config_dir().join("diagnostics")),
            resource_at:PollClock::default(),
            guard_notified:false,
            media_enabled,
            media_fast_until: None,
            placed: false,
            placement_at: PollClock::default(),
            placement_scale: 0.0,
            placement_key: (String::new(), 0, daozai::model::Dock::Top, true, (0, 0)),
            #[cfg(windows)]
            oled_shift: (0, 0),
            win_x: 0.0,
            sys: (None, false, PollClock::default()),
            #[cfg(windows)]
            system_state,
            #[cfg(windows)] system_ready:false,
            #[cfg(windows)] system_rx,
            fullscreen:None,
            input_state:daozai::input::Monitor::new(initial_input),
            #[cfg(windows)] input_keys:daozai::input::Shortcuts::start(),
            #[cfg(windows)] incoming,
            #[cfg(windows)] messengers:Default::default(),
            #[cfg(windows)] unread_at:PollClock::default(),
            #[cfg(windows)] opening_messenger:None,
            #[cfg(windows)] tray_opening:None,
            #[cfg(windows)] hidden:false,
            #[cfg(windows)] fullscreen_covered:false,
            #[cfg(windows)] backdrop:None,
            #[cfg(windows)]
            native: daozai::native::Window::new(cc),
            #[cfg(windows)]
            tray,
            #[cfg(windows)]
            autostart,
            #[cfg(windows)]
            bt_rx: None,
            bt: Vec::new(),
            bt_ready: false,
            volume_quiet: PollClock::default(),
            mail: None,
            mail_sig: None,
            cal: None,
            cal_sig: None,
            clipboard_watch: None,
            gws,
            quota: daozai::quota::Service::start(), quota_sig: None, quota_at: PollClock::default(),
        }
    }

    #[cfg(windows)]
    fn poll_tray(&mut self) {
        use daozai::tray::{self, Event};
        let events = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for e in events {
            match e {
                Event::OpenSettings=>self.app.open_settings(),
                Event::OpenCalendar=>self.app.open_calendar(None),
                Event::TogglePet => {
                    self.app.set_pet(!self.app.settings.pet);
                    self.app.say(if self.app.settings.pet { "吉祥物已開啟" } else { "吉祥物已關閉" });
                }
                Event::ToggleMinimal => {
                    self.app.set_minimal(!self.app.settings.minimal);
                    self.app.say(if self.app.settings.minimal { "極簡劉海已開啟" } else { "極簡劉海已關閉" });
                }
                Event::ToggleAutostart => {
                    let want = !self.autostart;
                    match tray::set_autostart(want) {
                        Ok(()) => {
                            self.autostart = want;
                            self.app.say(if want { "已設為開機啟動" } else { "已取消開機啟動" });
                        }
                        Err(e) => self.app.say(e),
                    }
                }
            }
        }
        if let Some(t) = &self.tray {
            t.sync(tray::State { pet: self.app.settings.pet, minimal: self.app.settings.minimal, autostart: self.autostart });
        }
    }

    fn env(&mut self, ctx: &egui::Context) -> Env {
        if let Some((source,rx))=&self.music_volume_rx {
            if let Ok(success)=rx.try_recv() {
                if !success && self.media.lock().is_ok_and(|media|media.source==*source) {
                    self.app.say("音樂應用音量暫不可調，請稍後重試");
                }
                self.music_volume_rx=None;
            }
        }
        let (clock, today, date) = local_now();
        // 電量變化很慢，每 5 秒讀一次就夠
        if self.sys.2.ready(Duration::from_secs(5)) {
            let (b, c) = read_power();
            if self.app.settings.show_battery && self.sys.0.is_some() && self.sys.1!=c && self.app.settings.system_hud {self.app.alert(if c {"電源已連線"}else{"已切換電池供電"},b.map(|v|format!("當前電量 {v}%")).unwrap_or_else(||"桌面電源狀態已更新".into()),5,false);}
            if self.app.settings.show_battery && b.is_some_and(|v|v<=20) && !c && self.sys.0.is_some_and(|v|v>20) {self.app.alert("電量偏低",format!("剩餘 {}% · 請連線電源",b.unwrap()),5,true);}
            self.sys = (b, c, PollClock::now());
        }
        let ppp = ctx.pixels_per_point();
        let origin = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min).unwrap_or(Pos2::new(self.win_x, 0.0));
        #[cfg(windows)]
        let origin=self.native.as_ref().and_then(|w|w.logical_origin(ppp)).unwrap_or(origin);
        let cursor = cursor_logical(ppp).map(|(x, y)| Pos2::new(x - origin.x, y - origin.y));
        // 所在螢幕換算到視窗邏輯座標，供通知詳情等浮層避開螢幕邊緣。
        #[cfg(windows)]
        let screen=daozai::display_layout::selected(&self.app.displays,&self.app.settings.display_id).map(|d|{
            // 與島體擺放用同一範圍：避開頂欄後，拖動換邊和浮層都以頂欄下沿為頂邊。
            let b=daozai::display_layout::placement_bounds(d,self.app.settings.avoids_top_bar());
            egui::Rect::from_min_max(Pos2::new(b.left as f32/ppp-origin.x,b.top as f32/ppp-origin.y),Pos2::new(b.right as f32/ppp-origin.x,b.bottom as f32/ppp-origin.y))
        });
        #[cfg(not(windows))]
        let screen=None;
        #[cfg(windows)]
        let (backdrop,glass_blur)=(self.backdrop.as_ref().and_then(|b|b.color()),self.native.as_ref().is_some_and(|w|w.blur_active()));
        #[cfg(not(windows))]
        let (backdrop,glass_blur)=(None,false);
        Env {
            t: self.t0.elapsed().as_secs_f64(),
            clock,
            today,
            date,
            battery: self.sys.0,
            charging: self.sys.1,
            network: Some(self.system_state.network),
            cursor,
            idle_seconds: {
                #[cfg(windows)] {Some(daozai::system::idle_seconds())}
                #[cfg(not(windows))] {None}
            },
            screen_locked: {
                #[cfg(windows)] {daozai::system::screen_locked()}
                #[cfg(not(windows))] {false}
            },
            fullscreen_hidden: {
                #[cfg(windows)] {self.fullscreen_covered}
                #[cfg(not(windows))] {false}
            },
            browser_at_top: {
                #[cfg(windows)] {if self.app.settings.browser_minimal {self.native.as_mut().and_then(|native|native.browser_at_top(&self.app.settings))}else{None}}
                #[cfg(not(windows))] {None}
            },
            media: self.media.lock().map(|m| m.clone()).unwrap_or_default(),
            volume: self.system_state.volume,
            bt: self.bt.clone(),
            bt_ready: self.bt_ready,
            screen,
            backdrop,
            glass_blur,
            opaque_surface: {
                #[cfg(windows)] {self.native.as_ref().is_some_and(|w|w.opaque_surface())}
                #[cfg(not(windows))] {false}
            },
        }
    }
}

impl eframe::App for Shell {
    fn raw_input_hook(&mut self,_ctx:&egui::Context,input:&mut egui::RawInput) {
        if input.viewport_id==egui::ViewportId::ROOT {self.app.filter_todo_input(input);}
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let frame_start=Instant::now();
        if self.resource_at.ready(Duration::from_secs(30)){
            let (count,bytes)={let textures=ctx.tex_manager();let textures=textures.read();(textures.num_allocated(),textures.allocated().map(|(_,info)|info.bytes_used()).sum())};
            daozai::resources::TEXTURES.store(count,std::sync::atomic::Ordering::Relaxed);
            daozai::resources::TEXTURE_BYTES.store(bytes,std::sync::atomic::Ordering::Relaxed);
            daozai::resources::GALLEYS.store(ctx.fonts(|fonts|fonts.num_galleys_in_cache()),std::sync::atomic::Ordering::Relaxed);
            self.resource_at=PollClock::now();
        }
        let guarded=daozai::resources::GUARD.tripped();
        if guarded && !self.guard_notified {
            // 資源保護熔斷：停止音樂會話輪詢（含封面、網易雲日誌）與歌詞查詢，保住使用者電腦，重啟後恢復。
            self.guard_notified=true;self.app.opts.fetch_lyrics=false;
            self.app.alert("島仔已暫停音樂功能","檢測到資源佔用異常，已暫停讀取音樂訊息和歌詞。重啟島仔即可恢復；診斷記錄在資料目錄的 diagnostics 資料夾。",0,false);
        }
        self.media_enabled.store(self.app.settings.visible_pages[0] && !guarded,std::sync::atomic::Ordering::Relaxed);
        if self.media_fast_until.is_some_and(|until|Instant::now()>=until){self.media_fast_until=None;}
        self.media_enabled.set_interval(if self.media_fast_until.is_some(){smtc::POLL_FAST}else{self.app.media_poll_interval()});
        // 設定裡改了島體大小：下一幀起主視窗按新倍率柵格化，定位邏輯隨畫素比變化同步調整畫布。
        #[cfg(windows)] eframe::set_root_viewport_scale(self.app.settings.island_scale());
        #[cfg(windows)]
        {
            // 用幀間隔限制重新整理，繞過部分 Intel 驅動在垂直同步中的忙等待。
            let delay=Duration::from_secs_f64(1.0/60.0).saturating_sub(self.frame_at.elapsed());
            if !delay.is_zero(){std::thread::sleep(delay);}
            self.frame_at=Instant::now();
        }
        #[cfg(windows)]
        self.poll_tray();
        #[cfg(windows)]
        let foreground_changed=daozai::native::take_foreground_change();
        #[cfg(windows)]
        if foreground_changed {
            // 切到別的程式：收起，並在游標離開前不再自動展開
            self.app.foreground_changed();
        }
        #[cfg(windows)]
        let offset=self.app.placement_offset();
        #[cfg(windows)]
        let shift=self.oled_shift;
        // 拖動時位置不取整，按萬分之一比例比較，視窗逐幀平滑跟隨。
        #[cfg(windows)]
        let placement_key=(self.app.settings.display_id.clone(),(offset*10000.0).round() as i32,self.app.settings.dock,self.app.settings.avoids_top_bar(),shift);
        #[cfg(windows)]
        if !self.placed || self.placement_scale!=ctx.pixels_per_point() || self.placement_key!=placement_key || self.placement_at.ready(Duration::from_secs(1)) {
            // 只有位置變化時沿用已知螢幕，避免拖動中每幀列舉顯示器。
            let offset_only=self.placed && self.placement_scale==ctx.pixels_per_point() && !self.placement_at.ready(Duration::from_secs(1))
                && (&self.placement_key.0,self.placement_key.2,self.placement_key.3,self.placement_key.4)==(&placement_key.0,placement_key.2,placement_key.3,placement_key.4);
            if !offset_only {
                self.placement_at=PollClock::now();
                self.app.displays=daozai::display_layout::connected();
            }
            self.placement_scale=ctx.pixels_per_point();
            self.placement_key=placement_key;
            let display=daozai::display_layout::selected(&self.app.displays,&self.app.settings.display_id);
            if display.is_some_and(|d|self.native.as_ref().is_some_and(|w|w.place_on_display(self.placement_scale,daozai::display_layout::placement_bounds(d,self.app.settings.avoids_top_bar()),offset,self.app.settings.dock,shift))) {
                if !self.placed {diagnostics::startup("目標螢幕物理座標定位完成，已請求顯示");}
                self.placed=true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            } else if self.t0.elapsed()>=Duration::from_secs(2) {
                // Do not leave a hidden startup window forever when monitor discovery fails.
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
        #[cfg(not(windows))]
        if !self.placed {
            if let Some(mon)=ctx.input(|i|i.viewport().monitor_size) {
                self.placed=true;self.win_x=((mon.x-app::WIN_W)/2.0).round();
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(self.win_x,0.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
        #[cfg(windows)]
        {
            while let Ok(observation)=self.system_rx.try_recv(){
                let next=observation.input;
                let manual=next.is_some_and(|state|self.input_keys.as_ref().is_some_and(|keys|keys.allows(state.context)));
                if let Some(label)=self.input_state.observe(next,manual){
                    if let Some(keys)=&self.input_keys{keys.clear();}
                    if self.app.settings.system_hud{self.app.alert("鍵盤輸入語言",label,5,false);}
                }
                let new=observation.system;
                // 切換輸出裝置時，系統會恢復該裝置的音量；等待它穩定後再提示調節。
                if new.audio_endpoint!=self.system_state.audio_endpoint {self.volume_quiet=PollClock::now();}
                if self.system_ready && self.app.settings.system_hud {
                    for (title,body) in new.changes(&self.system_state) {
                        // 拖動音量條剛改過音量時，系統回聲不再單獨彈一條。
                        if title=="系統音量" && !self.volume_quiet.ready(Duration::from_millis(900)) { continue; }
                        self.app.alert(title,body,5,false);
                    }
                }
                self.system_state=new;
                self.system_ready=true;
                self.fullscreen=observation.fullscreen;
            }
            self.fullscreen_covered=daozai::display_layout::hide_for_fullscreen(self.app.settings.hide_fullscreen,self.fullscreen,daozai::display_layout::selected(&self.app.displays,&self.app.settings.display_id).map(|d|d.bounds));
            self.hidden=self.app.should_hide_for_fullscreen(self.fullscreen_covered);
            // 毛玻璃：在島體左右兩側和畫布底邊（都在視窗區域之外）取樣，估計島背後的明暗。
            if self.app.settings.skin.is_glass() && !self.hidden {
                let sampler=self.backdrop.get_or_insert_with(daozai::backdrop::Sampler::start);
                let ppp=ctx.pixels_per_point();
                if let Some(origin)=self.native.as_ref().and_then(|w|w.logical_origin(ppp)) {
                    let points=self.app.backdrop_points();
                    sampler.set_points(points.iter().map(|&(x,y)|(((origin.x+x)*ppp).round() as i32,((origin.y+y)*ppp).round() as i32)).collect());
                }
            } else {
                self.backdrop=None;
            }
            if self.app.settings.bluetooth_notifications {
                if self.bt_rx.is_none() {self.bt_rx=Some(daozai::bt::spawn());}
            } else {
                self.bt_rx=None;
                self.bt.clear();
                self.bt_ready=false;
            }
            if let Some(service)=&self.bt_rx {
                while let Ok(found)=service.rx.try_recv(){ self.bt=found; self.bt_ready=true; }
            }
            let previous_pending=self.app.pending_messengers.clone();
            for event in self.messengers.poll_taskbar(ctx,&self.app.settings){match event{
                TaskbarEvent::Status(client,status)=>self.app.set_messenger_status(client,status),
                TaskbarEvent::Message(unread)=>{
                    if self.messengers.track_unread(unread)==Err(OpenError::Exited){continue;}
                    self.app.receive_messenger_reminder(unread);
                }
            }}
            if let Some(rx)=&self.incoming {
                rx.taskbar_settings(&self.app.settings);
                while let Ok(event)=rx.try_recv(){match event {
                    daozai::system::Incoming::Revoked=>{self.app.settings.read_notifications=false;self.app.settings.save();},
                    daozai::system::Incoming::Status(s)=>self.app.listener_status=s,
                    daozai::system::Incoming::Message(a,id,b,target)=>{
                        if target.is_some_and(|target|self.messengers.track(target)==Err(OpenError::Exited)){continue;}
                        self.app.receive_notification_from(a,id,b,target.map(|target|target.unread));
                    }
                }}
            }
            if self.app.pending_messengers!=previous_pending || self.unread_at.ready(Duration::from_secs(1)){
                self.clear_exited_messengers(ctx);
            }
            if !self.app.pending_messengers.is_empty(){ctx.request_repaint_after(Duration::from_secs(1));}
            if foreground_changed || self.app.pending_messengers!=previous_pending {
                // 在微信 / QQ 裡開啟了某個帳號的視窗：清掉這個帳號的未讀。
                let mut sources:Vec<app::Messenger>=self.app.pending_messengers.iter().map(|u|u.source).collect();sources.dedup();
                match self.messengers.foreground(&sources){
                    Some((source,pid))=>{
                        if self.app.pending_messengers.iter().any(|u|u.source==source){messenger::windows::trace(format!("島上清除 {} 帳號 {pid}（前臺切換={foreground_changed}）",source.name()));}
                        self.messengers.viewed_client(source,pid);
                        self.app.viewed_client(source,pid);
                    }
                    None=>if foreground_changed && !sources.is_empty(){messenger::windows::trace(format!("前臺切換但不是訊息客戶端主視窗：{}；島上未讀 {:?}",messenger::windows::describe_foreground(),self.app.pending_messengers.iter().map(|u|(u.source.name(),u.pid)).collect::<Vec<_>>()));},
                }
            }
            if self.tray_opening.as_ref().is_some_and(|(unread,_)|!self.app.pending_messengers.contains(unread)){
                self.tray_opening=None;
            }
            if let Some((unread,rx))=&self.tray_opening {
                let unread=*unread;
                match rx.try_recv(){
                    Ok(Ok(()))=>{
                        self.tray_opening=None;
                        if !self.confirm_messenger_open(unread){
                            self.opening_messenger=Some((unread,Instant::now()));
                            ctx.request_repaint_after(Duration::from_secs(4));
                        }
                    }
                    Ok(Err(error))=>{
                        self.tray_opening=None;
                        if error==OpenError::Exited{self.app.discard_unread(unread);}
                        else if unread.source.adapter().dismiss_unread{self.app.reveal_messenger_notice(unread);}
                        self.app.messenger_unavailable(error.message(unread.source));
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected)=>{
                        messenger::windows::trace(format!("喚起 {} 帳號 {}：喚起執行緒異常結束，未返回結果",unread.source.name(),unread.pid));
                        self.tray_opening=None;self.app.messenger_unavailable(format!("{} 暫時無法響應，請重試",unread.source.name()));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty)=>{}
                }
            }
            if let Some((unread,started))=self.opening_messenger {
                if !self.app.pending_messengers.contains(&unread){self.opening_messenger=None;}
                else if started.elapsed()>=Duration::from_secs(4){
                    self.opening_messenger=None;
                    if !self.confirm_messenger_open(unread){
                        self.app.messenger_unavailable(format!("{}尚未切到前臺，可點圖示重試",unread.source.name()));
                    }
                }
            }
        }
        if let Some((message,rx))=&self.seek_rx {
            if let Some(success)=match rx.try_recv(){Ok(success)=>Some(success),Err(std::sync::mpsc::TryRecvError::Disconnected)=>Some(false),Err(_)=>None}{
                let message=*message;
                self.seek_rx=None;
                if !success{self.app.say(message);}
            }
        }
        let env = self.env(ctx);
        self.reconcile_modules(ctx);
        while let Some(ev) = self.gws.try_recv() {
            self.app.gws_event(ev);
        }
        // v0.5.5 郵件：開關 / 間隔 / 過濾 / 帳號清單變了就重新對齊收信執行緒，再把事件交給介面。
        // 每幀只比較不復制，確有變化時才複製帳號清單。
        if let Some(mail)=&self.mail {
            let (mail_enabled,mail_interval,mail_filter)=(self.app.mail_active(),self.app.settings.mail_interval,self.app.settings.mail_filter);
            if !self.mail_sig.as_ref().is_some_and(|known|(known.0,known.1,known.2)==(mail_enabled,mail_interval,mail_filter) && known.3.as_slice()==self.app.mail_accounts()){
                let sig=(mail_enabled,mail_interval,mail_filter,self.app.mail_accounts().to_vec());
                let accounts:&[daozai::mail::Account]=if sig.0{ &sig.3 }else{ &[] };
                mail.sync(accounts,sig.1,sig.2);
                self.mail_sig=Some(sig);
            }
            while let Some(ev)=mail.try_recv(){self.app.mail_event(ev);}
        }
        // 日程連線或自動同步開關變化時下發給工作執行緒。
        // 錄製/截圖演示用的是假帳號，別讓真服務去連（會立刻失敗、把狀態改成紅色）。
        #[cfg(feature="record")]
        let cal_demo=std::env::var("DAOZAI_CAL_DEMO").is_ok();
        #[cfg(not(feature="record"))]
        let cal_demo=false;
        if let Some(service)=&self.cal {
            let auto_sync=self.app.settings.calendar_auto_sync;
            let cal_same=self.cal_sig.as_ref().is_some_and(|(known,known_sync)|*known_sync==auto_sync && if cal_demo{known.is_empty()}else{self.app.calendar_configs_match(known)});
            if !cal_same{
                let cal=if cal_demo{Vec::new()}else{self.app.calendar_configs()};
                service.configure(cal.clone(),auto_sync);
                self.cal_sig=Some((cal,auto_sync));
            }
            while let Some(ev)=service.try_recv(){self.app.calendar_event(ev);}
        }
        #[cfg(feature="record")] let quota_demo=std::env::var("DAOZAI_QUOTA_DEMO").is_ok();
        #[cfg(not(feature="record"))] let quota_demo=false;
        if !quota_demo {
            let (active,config)=(self.app.quota_active(),self.app.quota_config());
            if !self.quota_sig.as_ref().is_some_and(|known|quota_sig_matches(known,active,config)) || self.quota_at.ready(Duration::from_secs(180)){
                let sig=quota_sig(active,config);
                let accounts:Vec<_>=sig.1.iter().filter(|a|sig.0 && a.enabled).cloned().collect();
                if self.quota.refresh(accounts.clone()){self.app.quota_pending(&accounts);self.quota_sig=Some(sig);self.quota_at=PollClock::now();}
            }
            while let Some(ev)=self.quota.try_recv(self.app.quota_config()){self.app.quota_event(ev);}
        }
        let mut out = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                // 左右停靠時畫布是豎向的 SIDE_W × SIDE_H。
                let size=if self.app.settings.dock.is_side() {egui::vec2(daozai::side_dock::SIDE_W,daozai::side_dock::SIDE_H)} else {egui::vec2(app::WIN_W,app::WIN_H)};
                let win = egui::Rect::from_min_size(ui.max_rect().min,size);
                out = Some(self.app.frame(ui, win, &env));
            });
        let Some(out) = out else { return };

        #[cfg(windows)]
        {
            // frame() 剛推進 tick_alerts()：若有通知剛抵達或剛結束，本幀立即同步視窗顯示／隱藏狀態。
            let was_hidden=self.hidden;
            self.hidden=self.app.should_hide_for_fullscreen(self.fullscreen_covered);
            if self.hidden!=was_hidden {ctx.request_repaint();}
            // 先處理本幀輸入和動畫，再決定是否移動，避免懸停或按下的首幀仍使用舊狀態。
            let hovering=env.cursor.is_some_and(|p|out.hit.contains(p)||out.notice_hit.is_some_and(|r|r.contains(p)));
            let shift=if !self.app.settings.oled_shift {(0,0)}
                else if !self.hidden && !hovering && self.app.oled_shift_idle(ctx) {
                    ctx.request_repaint_after(daozai::oled_shift::until_next_step(self.t0.elapsed()));
                    let target=daozai::oled_shift::shift(true,self.app.settings.dock,self.t0.elapsed());
                    daozai::oled_shift::toward(self.oled_shift,target)
                } else {self.oled_shift};
            if shift!=self.oled_shift {
                self.oled_shift=shift;
                let display=daozai::display_layout::selected(&self.app.displays,&self.app.settings.display_id);
                if display.is_some_and(|d|self.native.as_ref().is_some_and(|w|w.place_on_display(ctx.pixels_per_point(),daozai::display_layout::placement_bounds(d,self.app.settings.avoids_top_bar()),self.app.placement_offset(),self.app.settings.dock,shift))) {
                    self.placement_key.4=shift;
                }
                ctx.request_repaint();
            }
        }

        #[cfg(windows)]
        if let Some(native) = &mut self.native {
            // 相容渲染：讀回的畫面交給分層視窗，邊緣保留抗鋸齒。畫面變化（或有其他輸入）時才請求下一次讀回，
            // 讀回完成本身觸發的那一幀若畫面沒變就停下，靜止時不會一直空轉。
            if native.wants_frames() {
                let (shot,other_input)=ctx.input(|i|(
                    i.events.iter().rev().find_map(|e|match e{egui::Event::Screenshot{image,..}=>Some(image.clone()),_=>None}),
                    i.events.iter().any(|e|!matches!(e,egui::Event::Screenshot{..})),
                ));
                let changed=shot.as_ref().is_some_and(|image|native.present_frame(image.size,image.as_raw()));
                if shot.is_none() || changed || other_input {
                    // 拖動、懸停等有輸入時滿幀跟手；歌詞滾動這類自己在動的畫面約 30 幀/秒讀回，
                    // 未到間隔就預約一次重繪，到時補上讀回，動畫停下時的最後一幀不會漏掉。
                    let now=Instant::now();
                    match daozai::layered::readback_delay(self.readback_at,now,other_input) {
                        None=>{self.readback_at=Some(now);ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));}
                        Some(wait)=>ctx.request_repaint_after(wait),
                    }
                }
            }
            let r = |r: egui::Rect| (r.left(), r.top(), r.width(), r.height());
            // 保留主視窗的事件迴圈，避免全屏隱藏後設定視窗無法關閉或重新開啟。
            let region=if self.hidden {(0.0,0.0,0.0,0.0)} else {r(out.region)};
            let hit=if self.hidden {(0.0,0.0,0.0,0.0)} else {r(out.hit)};
            let want_blur = self.app.settings.skin.is_glass() && self.app.settings.glass_blur;
            native.update(region, hit, if self.hidden{None}else{out.notice_hit.map(r)},if self.hidden{None}else{out.file_drop_zone.map(r)},if self.hidden{None}else{out.glass.as_deref()},want_blur,ctx.pixels_per_point(), out.editing, !self.hidden && out.expanded);
            if !self.hidden && self.app.settings.avoids_top_bar() && self.app.settings.top_bar_on_top && !self.app.settings.dock.is_side() {
                if let Some(display)=daozai::display_layout::selected(&self.app.displays,&self.app.settings.display_id) {
                    native.keep_above_top_bar(display.bounds);
                }
            }
        }
        for cmd in out.cmds {
            match cmd {
                Cmd::Media(t) => {
                    self.media_fast_until=Some(Instant::now()+Duration::from_secs(3));self.media_enabled.set_interval(smtc::POLL_FAST);
                    let source=self.media.lock().map(|media|media.source.clone()).unwrap_or_default();
                    let message=if source.eq_ignore_ascii_case("cloudmusic.exe"){"網易雲暫未適配進度跳轉，請在客戶端調整"}
                        else if source.eq_ignore_ascii_case("bodian_pc.exe"){"波點音樂暫未適配進度跳轉，請在客戶端調整"}
                        else if source.eq_ignore_ascii_case("kwmusic.exe"){"酷我音樂暫未適配進度跳轉，請在客戶端調整"}
                        else{"播放器未完成跳轉，請開啟客戶端後重試"};
                    if self.media_control.is_none(){self.media_control=smtc::ControlWorker::start(ctx.clone());}
                    match self.media_control.as_ref().ok_or(()).and_then(|worker|worker.submit(t,source)){
                        Ok(Some(rx))=>self.seek_rx=Some((message,rx)),Ok(None)=>{},Err(())=>self.app.say("播放器正在響應，請稍後重試"),
                    }
                }
                Cmd::MusicVolume(command,source)=>{
                    self.media_fast_until=Some(Instant::now()+Duration::from_secs(3));self.media_enabled.set_interval(smtc::POLL_FAST);
                    if self.media_control.is_none(){self.media_control=smtc::ControlWorker::start(ctx.clone());}
                    match self.media_control.as_ref().ok_or(()).and_then(|worker|worker.submit(command,source.clone())){
                        Ok(Some(rx))=>self.music_volume_rx=Some((source,rx)),
                        _=>self.app.say("音樂應用音量暫不可調，請稍後重試"),
                    }
                }
                Cmd::ViewMessenger(unread)=>{
                    #[cfg(windows)] {
                        let was_pending=self.app.pending_messengers.contains(&unread);
                        self.clear_exited_messengers(ctx);
                        if !self.app.pending_messengers.contains(&unread){
                            if was_pending{
                                messenger::windows::trace(format!("喚起 {} 帳號 {}：未開始，帳號程式已退出",unread.source.name(),unread.pid));
                                self.app.messenger_unavailable(OpenError::Exited.message(unread.source));
                            }
                            continue;
                        }
                        if self.tray_opening.is_some(){
                            messenger::windows::trace(format!("喚起 {} 帳號 {}：未開始，上一次喚起還沒結束",unread.source.name(),unread.pid));
                            self.app.messenger_unavailable(OpenError::Busy.message(unread.source));ctx.request_repaint();continue;
                        }
                        match self.messengers.request(unread,&self.app.settings).start(ctx.clone()){
                            Ok(rx)=>self.tray_opening=Some((unread,rx)),
                            Err(error)=>{
                                messenger::windows::trace(format!("喚起 {} 帳號 {}：未開始 {error:?}",unread.source.name(),unread.pid));
                                if error==OpenError::Unidentified{self.app.reveal_messenger_notice(unread);}
                                self.app.messenger_unavailable(error.message(unread.source));ctx.request_repaint();
                            }
                        }
                    }
                }
                Cmd::RestartApp=>{
                    // 先拉起新程序（它會等本程序退出），再像系統匣「退出」一樣摘圖示後結束。
                    if diagnostics::spawn_replacement(){
                        self.app.save_window_before_exit();
                        #[cfg(windows)] if let Some(tray)=&self.tray {tray.exit_process();}
                        std::process::exit(0);
                    }else{self.app.say("無法重新啟動島仔，請從系統匣退出後手動開啟");}
                }
                Cmd::Open(p) => {if let Err(e)=open_path(&p){self.app.say(e);}},
                Cmd::OpenUrl(url)=>{
                    if let Some(url)=daozai::mail::web_url(&url) {
                        if open_path(std::path::Path::new(&url)).is_err(){self.app.say("無法開啟預設瀏覽器，請檢查 Windows 預設應用設定");}
                    }
                },
                Cmd::CalendarImport=>{
                    #[cfg(windows)] match daozai::system::pick_files_for("選擇要匯入的日曆檔案（.ics）",true){
                        Ok(files)=>if !files.is_empty(){
                            let read:Vec<(String,String)>=files.iter().map(|path|{
                                let name=path.file_name().map(|n|n.to_string_lossy().to_string()).unwrap_or_default();
                                let text=std::fs::read(path).ok().filter(|raw|raw.len()<=20*1024*1024).map(|raw|String::from_utf8_lossy(&raw).into_owned()).unwrap_or_default();
                                (name,text)
                            }).collect();
                            let note=self.app.import_ics(&read);self.app.say(note);
                        },
                        Err(e)=>self.app.say(format!("無法選擇檔案：{}",e.code())),
                    }
                },
                Cmd::AddFiles=>{
                    #[cfg(windows)] match daozai::system::pick_files(){Ok(files)=>if !files.is_empty(){self.app.on_drop(files);self.app.show_page(3);},Err(e)=>self.app.say(format!("無法選擇檔案：{}",e.code()))}
                },
                Cmd::DragFile(p)=>{
                    #[cfg(windows)] if let Err(e)=daozai::system::drag_file(&p){self.app.say(format!("無法拖出檔案：{}",e.code()));}
                    ctx.input_mut(|i| i.pointer=Default::default());
                },
                Cmd::DisconnectNotifications=>{#[cfg(windows)]{self.incoming=None;self.app.settings.read_notifications=false;self.app.settings.save();self.app.listener_status="已斷開，不再讀取 Windows 通知".into();}},
                Cmd::Reveal(p)=>{let _=daozai::child_process::spawn(std::process::Command::new("explorer.exe").arg(format!("/select,{}",p.display())));},
                Cmd::QuotaChooseFile=>{
                    #[cfg(windows)] match daozai::system::pick_files_for("選擇 AI 帳號的登入 JSON 檔案",false){Ok(files)=>if let Some(path)=files.into_iter().next(){self.app.quota_set_source(path);},Err(_)=>self.app.say("無法選擇登入檔案")}
                },
                Cmd::QuotaRefresh=>{
                    if self.quota_at.ready(Duration::from_secs(15)) {
                        let (active,config)=(self.app.quota_active(),self.app.quota_config());let accounts:Vec<_>=config.accounts.iter().filter(|a|active && a.enabled).cloned().collect();
                        if self.quota.refresh(accounts.clone()){self.app.quota_pending(&accounts);self.quota_at=PollClock::now();}
                    }else{self.app.say("請稍後再重新整理");}
                },
                Cmd::QuotaConnected=>{self.quota_sig=None;},
                Cmd::ToggleAutostart=>{
                    #[cfg(windows)] {let want=!self.autostart;match daozai::tray::set_autostart(want){Ok(())=>self.autostart=want,Err(e)=>self.app.say(e)}}
                },
                Cmd::SetVolume(v)=>{
                    #[cfg(windows)] daozai::system::set_volume(v);
                    self.volume_quiet=PollClock::now();
                },
                Cmd::ToggleMute=>{
                    #[cfg(windows)] daozai::system::toggle_mute();
                    self.volume_quiet=PollClock::now();
                },
                Cmd::Copy(text)=>{ctx.copy_text(text);self.app.say("已複製");},
                Cmd::Notify(title,body)=>{
                    #[cfg(windows)] if let Some(tray)=&self.tray{tray.notify(&title,&body,self.app.settings.sound);}
                },
                cmd @ (Cmd::RequestNotifications|Cmd::RequestQqNotifications)=>{
                    #[cfg(windows)] {self.app.settings.qq_only=matches!(cmd,Cmd::RequestQqNotifications);self.app.settings.read_notifications=true;self.app.settings.save();self.incoming=None;match daozai::system::connect_with_settings(ctx.clone(),&self.app.settings){Ok(rx)=>{self.incoming=Some(rx);self.app.listener_status="正在請求 Windows 通知許可權…".into();},Err(e)=>self.app.listener_status=format!("Windows 未允許讀取通知：{}。需要相應的系統許可權或應用身份。",e.code())}}
                },
                // 模組關閉時設定項已置灰；萬一仍有命令到達，只提示，不為它臨時啟動服務。
                Cmd::MailSync=>if let Some(mail)=&self.mail {
                    let enabled=self.app.mail_active();
                    let interval=self.app.settings.mail_interval;
                    let filter=self.app.settings.mail_filter;
                    let accounts=if enabled{self.app.mail_accounts().to_vec()}else{Vec::new()};
                    mail.sync(&accounts,interval,filter);
                    self.mail_sig=Some((enabled,interval,filter,self.app.mail_accounts().to_vec()));
                },
                Cmd::MailTest(account,password)=>match &self.mail {Some(mail)=>mail.test(account,password),None=>self.app.say(MODULE_OFF)},
                Cmd::MailManage(request)=>match &self.mail {Some(mail)=>mail.manage(request),None=>self.app.say(MODULE_OFF)},
                Cmd::CalendarTest(cfg,password)=>match &self.cal {Some(cal)=>cal.test(cfg,password),None=>self.app.say(MODULE_OFF)},
                Cmd::CalendarSync=>match &self.cal {Some(cal)=>cal.sync_now(),None=>self.app.say(MODULE_OFF)},
                Cmd::CalendarMonth(month)=>if let Some(cal)=&self.cal {cal.view_month(month);},
                Cmd::CalendarForget=>{
                    if let Some(target)=self.app.selected_calendar_account(){
                        if let Err(error)=daozai::calendar::forget_account(&target){self.app.say(&error);continue;}
                    }
                    if let Some(old)=self.app.forget_calendar(){daozai::calendar::secrets::delete(&old);}
                    if let Some(cal)=&self.cal {
                        let configs=self.app.calendar_configs();
                        cal.configure(configs.clone(),self.app.settings.calendar_auto_sync);
                        self.cal_sig=Some((configs,self.app.settings.calendar_auto_sync));
                    }
                },
                Cmd::GwsCheck => self.gws.check(false, true),
                Cmd::GwsLogin => self.gws.login(),
                Cmd::GwsSetup => self.gws.setup(),
                Cmd::GwsImportSecret => self.gws.import_secret(),
                Cmd::GwsInstall => self.gws.install(),
                Cmd::Focus => ctx.send_viewport_cmd(egui::ViewportCommand::Focus),
            }
        }


        #[cfg(windows)] let autostart = self.autostart;
        #[cfg(not(windows))] let autostart = false;
        self.app.settings_ui(ctx, autostart);
        self.app.mail_reader_ui(ctx);
        #[cfg(windows)] let repaint_after=if self.hidden{Duration::from_secs(1)}else{out.repaint_after};
        #[cfg(not(windows))] let repaint_after=out.repaint_after;
        schedule_repaint(ctx,repaint_after);
        // 只記慢幀：卡頓反饋時能直接看出是渲染慢還是輸入被吞（頁面/展開狀態一併記下）。
        let elapsed=frame_start.elapsed();
        if elapsed>=Duration::from_millis(60) {
            let (page,open)=self.app.debug_state();
            daozai::resources::append_log(&daozai::model::config_dir().join("diagnostics"),"frames",
                &format!("幀 {:.0} ms｜頁面 {}｜展開 {}",elapsed.as_secs_f64()*1000.0,page,open));
        }
    }
}

// ---------------- 額度執行緒對齊 ----------------

/// 額度執行緒只關心總開關和帳號身份；別名、置頂隻影響顯示，改了不重新查詢。
/// `enabled` 是生效的開關：AI 額度模組開啟且額度設定裡開啟。
fn quota_sig(enabled:bool,config:&daozai::quota::Config)->(bool,Vec<daozai::quota::Account>){
    (enabled,config.accounts.iter().cloned().map(|mut a|{a.alias.clear();a.pinned=false;a}).collect())
}

/// 與 `quota_sig(enabled,config)==*known` 等價，但每幀呼叫時不復制帳號。
fn quota_sig_matches(known:&(bool,Vec<daozai::quota::Account>),enabled:bool,config:&daozai::quota::Config)->bool{
    known.0==enabled && known.1.len()==config.accounts.len() && known.1.iter().zip(&config.accounts).all(|(known,current)|{
        // 不用 `..`：帳號新增欄位時這裡會編譯失敗，提醒決定新欄位是否需要重新查詢。
        let daozai::quota::Account{id,provider,alias:_,source,slot,enabled,pinned:_}=current;
        known.id==*id && known.provider==*provider && known.source==*source && known.slot==*slot && known.enabled==*enabled
    })
}

#[cfg(test)]
mod quota_sig_tests {
    use super::*;
    use daozai::quota::{Account,Config};

    fn account(id:&str)->Account{
        Account{id:id.into(),provider:daozai::quota::Provider::Claude,alias:"工作".into(),source:"a.json".into(),slot:None,enabled:true,pinned:false}
    }

    #[test]
    fn matching_without_copy_agrees_with_the_copied_signature(){
        let config=Config{accounts:vec![account("a"),account("b")],..Default::default()};
        let known=quota_sig(true,&config);
        assert!(quota_sig_matches(&known,true,&config));
        let mut display_only=config.clone();display_only.accounts[0].alias="改名".into();display_only.accounts[1].pinned=true;
        assert!(quota_sig_matches(&known,true,&display_only),"改別名、置頂不應重新查詢");
        assert_eq!(quota_sig(true,&display_only),known);
        let changes:[fn(&mut Config);6]=[
            |c|c.enabled=false,
            |c|c.accounts[1].enabled=false,
            |c|c.accounts[0].source="b.json".into(),
            |c|c.accounts[0].slot=Some("org".into()),
            |c|{c.accounts.pop();},
            |c|c.accounts.swap(0,1),
        ];
        for change in changes {
            let mut changed=config.clone();change(&mut changed);
            assert!(!quota_sig_matches(&known,changed.enabled,&changed));
            assert_ne!(quota_sig(changed.enabled,&changed),known,"兩種判斷必須一致");
        }
        assert!(!quota_sig_matches(&known,false,&config),"關閉 AI 額度模組後應停止查詢");
    }
}

// ---------------- 系統訊息 ----------------

fn schedule_repaint(ctx:&egui::Context,interval:Duration){
    // egui 會預減一幀；本程式自行排程重新整理，補回後才能保持實際的等待間隔。
    let predicted=ctx.input(|i|Duration::from_secs_f32(i.predicted_dt));
    ctx.request_repaint_after(interval.saturating_add(predicted));
}

#[cfg(test)]
mod repaint_tests {
    use super::*;

    fn scheduled_delay(interval:Duration,immediate:bool)->Duration{
        let ctx=egui::Context::default();
        let mut delay=Duration::ZERO;
        for frame in 0..4 {
            let output=ctx.run(egui::RawInput{time:Some(frame as f64),predicted_dt:1.0/60.0,..Default::default()},|ctx|{
                if immediate{ctx.request_repaint();}
                schedule_repaint(ctx,interval);
            });
            delay=output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        }
        delay
    }

    #[test]
    fn background_repaint_keeps_the_requested_frame_interval(){
        for fps in [60.0,30.0,20.0,10.0,1.0] {
            let interval=Duration::from_secs_f64(1.0/fps);
            let actual=scheduled_delay(interval,false);
            assert!(actual.abs_diff(interval)<Duration::from_micros(1),"目標 {fps} 幀不應被提前喚醒：{actual:?}");
        }
    }

    #[test]
    fn interaction_and_service_events_still_repaint_immediately(){
        assert_eq!(scheduled_delay(Duration::from_secs(1),true),Duration::ZERO);
    }
}

#[cfg(windows)]
fn local_now() -> (String, String, String) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    let wd = ["週日", "週一", "週二", "週三", "週四", "週五", "週六"];
    (
        format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond),
        format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay),
        format!("{}月{}日 {}", st.wMonth, st.wDay, wd[(st.wDayOfWeek as usize).min(6)]),
    )
}

#[cfg(not(windows))]
fn local_now() -> (String, String, String) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    (format!("{h:02}:{m:02}:{s:02}"), "1970-01-01".into(), "1月1日 週四".into())
}

#[cfg(windows)]
fn read_power() -> (Option<u8>, bool) {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut st = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut st) }.is_ok() {
        let pct = if st.BatteryLifePercent == 255 { None } else { Some(st.BatteryLifePercent) };
        (pct, st.ACLineStatus == 1)
    } else {
        (None, false)
    }
}

#[cfg(not(windows))]
fn read_power() -> (Option<u8>, bool) {
    (None, true)
}

#[cfg(windows)]
fn cursor_logical(ppp: f32) -> Option<(f32, f32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((p.x as f32 / ppp, p.y as f32 / ppp))
}

#[cfg(not(windows))]
fn cursor_logical(_ppp: f32) -> Option<(f32, f32)> {
    None
}

#[cfg(windows)]
fn open_path(p: &std::path::Path)->Result<(),String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{ShellExecuteExW,SHOpenWithDialog,SHELLEXECUTEINFOW,OPENASINFO,SEE_MASK_FLAG_NO_UI,OAIF_EXEC};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let mut wide: Vec<u16> = p.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut info=SHELLEXECUTEINFOW{cbSize:std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,fMask:SEE_MASK_FLAG_NO_UI,lpVerb:windows::core::w!("open"),lpFile:PCWSTR(wide.as_ptr()),nShow:SW_SHOWNORMAL.0,..Default::default()};
    open_path_result(p,unsafe{ShellExecuteExW(&mut info)},||unsafe{
        SHOpenWithDialog(None,&OPENASINFO{pcszFile:PCWSTR(wide.as_ptr()),oaifInFlags:OAIF_EXEC,..Default::default()})
    })
}

#[cfg(windows)]
fn open_path_result(p:&std::path::Path,result:windows::core::Result<()>,choose:impl FnOnce()->windows::core::Result<()>)->Result<(),String>{
    use windows::Win32::Foundation::{ERROR_CANCELLED,ERROR_NO_ASSOCIATION};
    let result=match result{
        Err(error) if error.code()==windows::core::HRESULT::from_win32(ERROR_NO_ASSOCIATION.0)&&p.is_file()=>choose(),
        other=>other,
    };
    match result{
        Err(error) if error.code()==windows::core::HRESULT::from_win32(ERROR_CANCELLED.0)=>Ok(()),
        other=>other.map_err(|_|"無法開啟，請檢查檔案位置和預設應用".into()),
    }
}

#[cfg(not(windows))]
fn open_path(_p: &std::path::Path)->Result<(),String> {Ok(())}

#[cfg(all(test,windows))]
mod open_path_tests {
    use super::*;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED,ERROR_CANCELLED,ERROR_FILE_NOT_FOUND,ERROR_NO_ASSOCIATION};
    fn error(code:windows::Win32::Foundation::WIN32_ERROR)->windows::core::Error{windows::core::Error::from_hresult(windows::core::HRESULT::from_win32(code.0))}

    #[test]
    fn unassociated_files_offer_open_with_and_cancellation_is_normal(){
        let path=std::env::temp_dir().join(format!("島仔 無關聯-{}.daozai-open-test",std::process::id()));
        std::fs::write(&path,b"fixture").unwrap();
        for cancel in [false,true]{
            let mut chosen=false;
            let result=open_path_result(&path,Err(error(ERROR_NO_ASSOCIATION)),||{chosen=true;if cancel{Err(error(ERROR_CANCELLED))}else{Ok(())}});
            std::fs::remove_file(&path).unwrap();
            assert!(chosen,"檔案沒有預設關聯時應提供系統開啟方式");
            assert_eq!(result,Ok(()),"使用者取消選擇不應顯示開啟失敗");
            if !cancel{std::fs::write(&path,b"fixture").unwrap();}
        }
    }

    #[test]
    fn default_apps_urls_and_other_errors_never_open_the_file_picker(){
        let file=std::env::temp_dir().join(format!("daozai-open-errors-{}",std::process::id()));
        std::fs::write(&file,b"fixture").unwrap();
        for result in [Ok(()),Err(error(ERROR_ACCESS_DENIED)),Err(error(ERROR_FILE_NOT_FOUND))]{
            let success=result.is_ok();
            let actual=open_path_result(&file,result,||panic!("已有預設程式或其他開啟錯誤不應彈出選擇器"));
            assert_eq!(actual.is_ok(),success);
        }
        assert!(open_path_result(&file,Err(error(ERROR_NO_ASSOCIATION)),||Err(error(ERROR_ACCESS_DENIED))).is_err(),"選擇器失敗應保留錯誤提示");
        std::fs::remove_file(file).unwrap();
        for (path,result,success) in [
            ("https://example.com",Ok(()),true),
            ("https://example.com",Err(error(ERROR_NO_ASSOCIATION)),false),
            ("missing.daozai-open-test",Err(error(ERROR_NO_ASSOCIATION)),false),
            ("missing.daozai-open-test",Err(error(ERROR_FILE_NOT_FOUND)),false),
            ("blocked.daozai-open-test",Err(error(ERROR_ACCESS_DENIED)),false),
        ]{
            let actual=open_path_result(std::path::Path::new(path),result,||panic!("非檔案關聯問題不能彈出開啟方式"));
            assert_eq!(actual.is_ok(),success);
        }
    }
}

fn main() -> std::process::ExitCode {
    diagnostics::install();
    // 收集診斷：在單例項檢查、建立視窗和初始化顯示卡之前處理，島仔啟動即崩潰、卡死時也能用。
    #[cfg(windows)]
    if daozai::diagnostics_tool::requested(std::env::args()) {
        return match daozai::diagnostics_tool::run(&daozai::model::config_dir()) {
            Ok(_) => std::process::ExitCode::SUCCESS,
            Err(error) => {diagnostics::report("診斷收集未完成。", &error); std::process::ExitCode::FAILURE}
        };
    }
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            diagnostics::report("島仔啟動或執行失敗。", &error.to_string());
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> eframe::Result<()> {
    // 自動重啟或「立即重啟」拉起的新程序：先等舊程序退出，再做單例項檢查。
    diagnostics::wait_for_previous_instance();
    // 單例項（與原版同名互斥體）
    #[cfg(windows)]
    if !daozai::tray::single_instance() {
        if !daozai::tray::activate_existing(std::env::args().any(|arg|arg=="--calendar")) {
            diagnostics::report("島仔已在執行，但暫時無法開啟視窗。\n請稍後重試；若仍無法開啟，請在工作管理員的“詳細訊息”中結束 daozai.exe 後重試。", "已有例項可能正在啟動或已無響應。請稍後重試；若仍無法開啟，請在工作管理員的“詳細訊息”中結束 daozai.exe，再重新啟動。");
        }
        return Ok(());
    }
    #[cfg(windows)]
    std::thread::spawn(daozai::record_install_size);
    diagnostics::startup("初始化圖形視窗");
    let viewport = egui::ViewportBuilder::default()
        .with_title("島仔").with_icon(daozai::theme::app_icon())
        .with_inner_size([app::CANVAS_W, app::WIN_H])
        .with_decorations(false)
        .with_transparent(true)
        .with_active(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_always_on_top()
        .with_visible(false);
    let opts = eframe::NativeOptions { viewport, vsync: !cfg!(windows), ..Default::default() };
    graphics::run(opts,|opts|eframe::run_native("島仔", opts, Box::new(|cc| Ok(Box::new(Shell::new(cc))))))
}
