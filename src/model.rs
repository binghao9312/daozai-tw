//! 持久化與狀態模型。
//!
//! 檔案佈局與原工具同名同結構（原工具字串中可見的路徑與鍵名），但預設目錄
//! 用獨立的 `%APPDATA%\Daozai`，避免與原工具的 `%APPDATA%\Nusa` 互相覆蓋：
//!   %APPDATA%\Daozai\settings.txt   -> pet=0/1, minimal=0/1
//!   %APPDATA%\Daozai\todo.txt
//!   %APPDATA%\Daozai\water.txt      -> bottle=<ml> / goal=<ml> / start=YYYY-MM-DD
//!   %APPDATA%\Daozai\shelf.txt      -> 置物清單（每行一個絕對路徑）
//!
//! 設定環境變數 `DAOZAI_DATA_DIR` 可把以上目錄整體改到別處（用於與已安裝的
//! 原工具並存、或跑測試時不碰真實資料）。

use std::fs;
use std::path::{Path, PathBuf};

fn env_dir(key: &str) -> PathBuf {
    PathBuf::from(std::env::var(key).unwrap_or_else(|_| ".".into()))
}

pub fn config_dir() -> PathBuf {
    if let Ok(root) = std::env::var("DAOZAI_DATA_DIR") {
        return PathBuf::from(root);
    }
    env_dir("APPDATA").join("Daozai")
}

fn read_kv(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(text) = fs::read_to_string(path) {
        for line in text.trim_start_matches('\u{feff}').lines() {
            if let Some((k, v)) = line.split_once('=') {
                out.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    out
}

fn write_kv(path: &Path, pairs: &[(String, String)]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body: String = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}\n"))
        .collect();
    atomic_write(path, body.as_bytes())
}

pub(crate) fn load_health_pending()->[bool;2] {
    let mut pending=[false;2];
    for (key,value) in read_kv(&config_dir().join("health-pending.txt")) {
        match key.as_str() {"water"=>pending[0]=value=="1","sit"=>pending[1]=value=="1",_=>{}}
    }
    pending
}

pub(crate) fn save_health_pending(pending:[bool;2]) {
    let _=write_kv(&config_dir().join("health-pending.txt"),&[
        ("water".into(),u8::from(pending[0]).to_string()),
        ("sit".into(),u8::from(pending[1]).to_string()),
    ]);
}

pub(crate) fn atomic_write(path:&Path,body:&[u8])->std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64,Ordering};
    static SERIAL:AtomicU64=AtomicU64::new(0);
    let (tmp,mut file)=loop {
        let mut name=path.file_name().ok_or_else(||std::io::Error::other("缺少檔名"))?.to_os_string();
        name.push(format!(".{}-{}.tmp",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
        let tmp=path.with_file_name(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file)=>break (tmp,file),
            Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,
            Err(e)=>return Err(e),
        }
    };
    let result=file.write_all(body).and_then(|_|file.sync_all());
    drop(file);
    let result=result.and_then(|_|replace_file(&tmp,path));
    if result.is_err(){let _=fs::remove_file(&tmp);}
    result
}

fn replace_file(tmp:&Path,path:&Path)->std::io::Result<()> {
    let result=fs::rename(tmp,path);
    #[cfg(windows)]
    if result.as_ref().is_err_and(|e|e.kind()==std::io::ErrorKind::CrossesDevices) {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{MoveFileExW,MOVEFILE_COPY_ALLOWED,MOVEFILE_REPLACE_EXISTING,MOVEFILE_WRITE_THROUGH};
        // Windows 目錄對映可能將同目錄替換報告為跨卷，交給系統複製並替換。
        let source:Vec<u16>=tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let target:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
        return unsafe{MoveFileExW(PCWSTR(source.as_ptr()),PCWSTR(target.as_ptr()),MOVEFILE_REPLACE_EXISTING|MOVEFILE_COPY_ALLOWED|MOVEFILE_WRITE_THROUGH)}.map_err(std::io::Error::other);
    }
    result
}

/// 島體配色皮膚（與形狀獨立，可任意組合）。
/// - 經典：純黑；奶油：暖白底深棕字、橙色強調（參考 Oyen）；
/// - 毛玻璃·淺 / 毛玻璃·深：系統實時模糊 + 淺色或深色玻璃。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skin {
    #[default]
    Classic,
    Cream,
    GlassLight,
    GlassDark,
    Fluid,
}

impl Skin {
    pub const ALL: [Skin; 5] = [Skin::Classic, Skin::Cream, Skin::GlassLight, Skin::GlassDark, Skin::Fluid];
    pub fn key(self) -> &'static str {
        match self { Self::Classic => "classic", Self::Fluid => "fluid", Self::Cream => "cream", Self::GlassLight => "glass_light", Self::GlassDark => "glass" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "fluid" => Self::Fluid, "cream" => Self::Cream, "glass_light" => Self::GlassLight, "glass" | "glass_dark" => Self::GlassDark, _ => Self::Classic }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Classic => "經典", Self::Fluid => "懸浮雲", Self::Cream => "奶油", Self::GlassLight => "毛玻璃·淺", Self::GlassDark => "毛玻璃·深" }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Classic => "純黑島體，與螢幕劉海融為一體，資源佔用最低。",
            Self::Fluid => "半透明灰底隨音樂封面換色，光暈跟隨滑鼠，播放進度帶有漸隱光尾。皮膚作者：啵（@hanhb123）。",
            Self::Cream => "暖白底色配深棕文字和橙色強調，柔和明亮。",
            Self::GlassLight => "淺色磨砂玻璃，透出身後桌面的顏色，文字為深色；按背景明暗自動調整，保證清晰。",
            Self::GlassDark => "深色磨砂玻璃，透出身後桌面的顏色，文字為白色；按背景明暗自動調整，保證清晰。",
        }
    }
    pub fn is_glass(self) -> bool { matches!(self, Self::GlassLight | Self::GlassDark) }
}

/// 島體形狀：貼著螢幕頂邊的劉海，或懸浮在頂邊下方的膠囊。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IslandShape {
    #[default]
    Notch,
    Pill,
}

impl IslandShape {
    pub const ALL: [IslandShape; 2] = [IslandShape::Notch, IslandShape::Pill];
    pub fn key(self) -> &'static str {
        match self { Self::Notch => "notch", Self::Pill => "pill" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "pill" => Self::Pill, _ => Self::Notch }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Notch => "劉海", Self::Pill => "膠囊" }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Notch => "貼著螢幕頂邊，頂部兩側向外柔和過渡，像螢幕劉海。",
            Self::Pill => "懸浮在螢幕頂邊下方，四角圓潤，收起時是完整的膠囊。",
        }
    }
}

/// 島體停靠的螢幕邊緣：頂部為橫向劉海 / 膠囊，左右兩側為豎向排列。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dock {
    #[default]
    Top,
    Left,
    Right,
}

impl Dock {
    pub const ALL: [Dock; 3] = [Dock::Top, Dock::Left, Dock::Right];
    pub fn key(self) -> &'static str {
        match self { Self::Top => "top", Self::Left => "left", Self::Right => "right" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "left" => Self::Left, "right" => Self::Right, _ => Self::Top }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Top => "頂部", Self::Left => "左側", Self::Right => "右側" }
    }
    pub fn is_side(self) -> bool { self != Self::Top }
}

/// 島體大小（百分比）的範圍與步進。
pub const ISLAND_SIZE_MIN: u32 = 80;
pub const ISLAND_SIZE_MAX: u32 = 140;
pub const ISLAND_SIZE_STEP: u32 = 5;
/// 水平位置：-100 = 最左，0 = 居中，100 = 最右（按可移動範圍的百分比）。
pub const ISLAND_OFFSET_MAX: i32 = 100;

pub fn island_size_value(value: u32) -> u32 {
    let value = value.clamp(ISLAND_SIZE_MIN, ISLAND_SIZE_MAX);
    (value + ISLAND_SIZE_STEP / 2) / ISLAND_SIZE_STEP * ISLAND_SIZE_STEP
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub display_id: String,
    /// 島體皮膚、毛玻璃是否使用系統模糊、大小百分比、水平位置。
    pub skin: Skin,
    pub island_shape: IslandShape,
    pub glass_blur: bool,
    pub island_size: u32,
    pub island_offset: i32,
    /// OLED 畫素偏移：島體收起靜止時沿停靠邊緩慢移動幾個畫素，減輕 OLED 靜態顯示留下的痕跡。
    /// 預設關閉，開啟後也不改動 `island_offset`。
    pub oled_shift: bool,
    /// 停靠邊緣；左右停靠時 `island_offset` 表示上下位置。
    pub dock: Dock,
    pub settings_theme: crate::settings_theme::SettingsTheme,
    pub pet: bool,
    pub minimal: bool,
    pub browser_minimal: bool,
    pub avoid_browsers: bool,
    pub avoid_apps: Vec<(String,String)>,
    pub visible_pages: [bool; 9],
    pub page_order: [usize; 9],
    pub show_clock: bool,
    pub show_battery: bool,
    pub show_weather: bool,
    pub clock_12h: bool,
    pub clock_period: u8,
    pub clock_seconds: bool,
    pub resident_order: Vec<usize>,
    pub system_hud: bool,
    pub bluetooth_notifications: bool,
    pub read_notifications:bool,
    pub wechat_notifications:bool,
    pub wecom_notifications:bool,
    pub feishu_notifications:bool,
    pub dingtalk_notifications:bool,
    /// 微信縮在系統匣時，點提醒透過系統系統匣按鈕喚起微信（預設關閉）。
    pub wechat_tray_open:bool,
    pub qq_only:bool,
    pub notification_preview:bool,
    pub notifications: bool,
    pub sound: bool,
    pub lyrics_enabled:bool,
    pub island_lyrics:bool,
    pub lyric_delay_ms:i32,
    pub hide_fullscreen: bool,
    /// 啟用頂欄適配，按顯示方式避讓頂欄或保持島體在頂欄上方。
    pub avoid_top_bar: bool,
    pub top_bar_on_top: bool,
    pub focus_minutes: u32,
    pub rest_minutes: u32,
    pub water_minutes: u32,
    pub serving_ml: u32,
    pub water_start: u32,
    pub water_end: u32,
    /// 久坐提醒：連續使用電腦達到 sit_minutes 分鐘後提醒起身；與喝水共用提醒時段。
    pub sit_remind: bool,
    pub sit_minutes: u32,
    /// 午休免打擾：lunch_start..lunch_end（一天中的分鐘數）內喝水和久坐都不提醒。
    pub lunch_rest: bool,
    pub lunch_start: u32,
    pub lunch_end: u32,
    /// v0.5.5 郵件提醒：總開關、驗證碼高亮、輪詢間隔（秒）、會議提前量（分鐘）。
    pub mail_enabled: bool,
    pub mail_sync_operations:bool,
    pub mail_code: bool,
    pub mail_code_hide_seconds:u32,
    pub mail_interval: u32,
    pub mail_ics: u32,
    pub ignored_mail_events: Vec<String>,
    /// v0.5.6 日程：還沒配置時在島上給一次「接入飛書日曆」的提示；
    /// 使用者點開設定或關掉後置 false，不再彈。
    pub calendar_prompt: bool,
    pub calendar_auto_sync: bool,
    /// 記錄最近複製的文字（最近記錄與固定條目均在本機儲存）。
    pub clipboard_history: bool,
    /// 天氣頁要看的城市；留空表示按網路出口自動定位。
    pub weather_city: String,
    /// 使用者確認的具體地點；重新整理時直接使用座標，避免同名城市重新匹配。
    pub weather_position: Option<crate::weather::Position>,
    /// 已儲存、可在島上下拉切換的城市（不含自動定位），最多 `weather::MAX_CITIES` 個。
    pub weather_saved: Vec<crate::weather::Position>,
    /// 相容渲染：用 Direct3D 12 代替 OpenGL（部分顯示卡的 OpenGL 驅動會崩潰或畫面錯亂）。重啟後生效。
    pub render_compat: bool,
    /// 自動切換到相容渲染的原因；手動開啟時為空。
    pub render_compat_reason: String,
    /// 首次啟動是否已執行 Google Workspace CLI（gws）自動登入／綁定流程。
    pub gws_onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { display_id: String::new(), skin: Skin::Classic, island_shape: IslandShape::Notch, glass_blur: true, island_size: 100, island_offset: 0, oled_shift: false, dock: Dock::Top, settings_theme: Default::default(), pet: true, minimal: false, browser_minimal: false, avoid_browsers: true, avoid_apps: Vec::new(), visible_pages: [true; 9], page_order: [0,1,2,3,4,5,6,7,8], show_clock: true, show_battery: true, show_weather: true, clock_12h: false, clock_period: 0, clock_seconds: false, resident_order: Vec::new(), system_hud: true, bluetooth_notifications: true, read_notifications:false, wechat_notifications:false, wecom_notifications:false, feishu_notifications:false, dingtalk_notifications:false, wechat_tray_open:false, qq_only:false, notification_preview:false, notifications: true, sound: false, lyrics_enabled:true, island_lyrics:false, lyric_delay_ms:-350, hide_fullscreen: true, avoid_top_bar: true, top_bar_on_top: false, focus_minutes: 25, rest_minutes: 5, water_minutes: 45, serving_ml: 250, water_start: 9, water_end: 22, sit_remind: true, sit_minutes: 60, lunch_rest: true, lunch_start: 720, lunch_end: 810, mail_enabled: true, mail_sync_operations:false, mail_code: true, mail_code_hide_seconds:60, mail_interval: 120, mail_ics: 15, ignored_mail_events: Vec::new(), calendar_prompt: true, calendar_auto_sync: true, clipboard_history: true, weather_city: String::new(), weather_position: None, weather_saved: Vec::new(), render_compat: false, render_compat_reason: String::new(), gws_onboarded: false }
    }
}

impl Settings {
    pub fn avoids_top_bar(&self) -> bool { self.avoid_top_bar && !self.top_bar_on_top }

    /// 主視窗（靈動島）的介面縮放倍率。
    pub fn island_scale(&self) -> f32 {
        island_size_value(self.island_size) as f32 / 100.0
    }

    /// 水平位置，-1..=1。
    pub fn island_offset_ratio(&self) -> f32 {
        self.island_offset.clamp(-ISLAND_OFFSET_MAX, ISLAND_OFFSET_MAX) as f32 / ISLAND_OFFSET_MAX as f32
    }

    pub fn clock_text(&self, clock: &str) -> String {
        use chrono::Timelike;
        let Ok(time) = chrono::NaiveTime::parse_from_str(clock, "%H:%M:%S").or_else(|_|chrono::NaiveTime::parse_from_str(clock,"%H:%M")) else { return if self.clock_seconds {"--:--:--"}else{"--:--"}.into(); };
        let (pm, hour) = time.hour12();
        let mut digits=if self.clock_12h {format!("{}:{:02}",hour,time.minute())}else{time.format("%H:%M").to_string()};
        if self.clock_seconds {digits.push_str(&format!(":{:02}",time.second()));}
        let period=match self.clock_period {
            1=>if pm {"下午"}else{"上午"},
            2=>match time.hour() {0..=5=>"凌晨",6..=10=>"上午",11..=12=>"中午",13..=16=>"下午",17..=18=>"傍晚",_=>"晚上"},
            _=>return digits,
        };
        format!("{period} {digits}")
    }

    pub fn load() -> Self {
        let mut s = Self::default();
        for (k, v) in read_kv(&config_dir().join("settings.txt")) {
            match k.as_str() {
                "settings_theme" => s.settings_theme=crate::settings_theme::SettingsTheme::parse(&v),
                "display_id" => s.display_id = v,
                "skin" => {
                    // 早期測試版把膠囊當作皮膚儲存。
                    if v == "pill" { s.island_shape = IslandShape::Pill; }
                    s.skin = Skin::parse(&v);
                }
                "island_shape" => s.island_shape = IslandShape::parse(&v),
                "dock" => s.dock = Dock::parse(&v),
                "glass_blur" => s.glass_blur = v != "0",
                "island_size" => s.island_size = island_size_value(v.parse::<u32>().unwrap_or(100)),
                "island_offset" => s.island_offset = v.parse::<i32>().unwrap_or(0).clamp(-ISLAND_OFFSET_MAX, ISLAND_OFFSET_MAX),
                "oled_shift" => s.oled_shift = v != "0",
                "pet" => s.pet = v == "1" || v.eq_ignore_ascii_case("true"),
                "minimal" => s.minimal = v == "1" || v.eq_ignore_ascii_case("true"),
                "browser_minimal" => s.browser_minimal = v == "1",
                "avoid_browsers" => s.avoid_browsers = v != "0",
                "avoid_apps" => s.avoid_apps = serde_json::from_str(&v).unwrap_or_default(),
                "resident_order" => {
                    let mut order:Vec<usize>=v.split(',').filter_map(|n|n.parse().ok()).collect();
                    if order.len()==6 && !order.contains(&crate::bar_layout::WEATHER) {
                        if let Some(clock)=order.iter().position(|slot|*slot==crate::bar_layout::CLOCK){order.insert(clock+1,crate::bar_layout::WEATHER);}
                    }
                    if crate::bar_layout::valid_order(&order){s.resident_order=order;}
                }
                "page_order" => {
                    if let Ok(order)=v.split(',').map(str::parse::<usize>).collect::<Result<Vec<_>,_>>() {
                        if order.len()==9 && (0..9).all(|page|order.contains(&page)) {
                            s.page_order.copy_from_slice(&order);
                        } else if order.len()==8 && (0..8).all(|page|order.contains(&page)) {
                            // 舊配置裡沒有天氣頁：接在最後，其餘順序保持不變。
                            s.page_order[..8].copy_from_slice(&order);
                            s.page_order[8]=8;
                        }
                    }
                }
                "clock_period" => s.clock_period = v.parse::<u8>().ok().filter(|v|*v<=2).unwrap_or(0),
                "clock_12h" => s.clock_12h = v == "1",
                "clock_seconds" => s.clock_seconds = v == "1",
                "show_battery" => s.show_battery = v != "0",
                "show_clock" => s.show_clock = v != "0",
                "show_weather" => s.show_weather = v != "0",
                "visible_pages" if (6..=9).contains(&v.len()) && v.bytes().all(|b| b==b'0' || b==b'1') => {
                    for (enabled,value) in s.visible_pages.iter_mut().zip(v.bytes()) { *enabled=value==b'1'; }
                    for enabled in &mut s.visible_pages[v.len()..]{*enabled=v.bytes().any(|b|b==b'1');}
                }
                "read_notifications" => s.read_notifications=v=="1",
                "wechat_notifications" => s.wechat_notifications=v=="1",
                "wecom_notifications" => s.wecom_notifications=v=="1",
                "feishu_notifications" => s.feishu_notifications=v=="1",
                "dingtalk_notifications" => s.dingtalk_notifications=v=="1",
                "wechat_tray_open" => s.wechat_tray_open=v=="1",
                "qq_only" => s.qq_only=v=="1",
                "notification_preview" => s.notification_preview=v=="1",
                "system_hud" => s.system_hud = v != "0",
                "bluetooth_notifications" => s.bluetooth_notifications = v != "0",
                "notifications" => s.notifications = v != "0",
                "sound" => s.sound = v == "1",
                "lyrics_enabled" => s.lyrics_enabled=v!="0",
                "island_lyrics" => s.island_lyrics=v=="1",
                "lyric_delay_ms" => s.lyric_delay_ms=v.parse::<i32>().unwrap_or(-350).clamp(-5000,5000),
                "hide_fullscreen" => s.hide_fullscreen = v != "0",
                "avoid_top_bar" => s.avoid_top_bar = v != "0",
                "top_bar_on_top" => s.top_bar_on_top = v == "1",
                "focus_minutes" => s.focus_minutes = v.parse::<u32>().unwrap_or(25).clamp(1, 180),
                "rest_minutes" | "short_minutes" => s.rest_minutes = v.parse::<u32>().unwrap_or(5).clamp(1, 60),
                "water_minutes" => s.water_minutes = v.parse::<u32>().unwrap_or(45).clamp(5, 180),
                "serving_ml" => s.serving_ml = v.parse::<u32>().unwrap_or(250).clamp(50, 1000),
                "water_start" => s.water_start = v.parse::<u32>().unwrap_or(9).min(23),
                "water_end" => s.water_end = v.parse::<u32>().unwrap_or(22).min(24),
                "sit_remind" => s.sit_remind = v != "0",
                "sit_minutes" => s.sit_minutes = v.parse::<u32>().unwrap_or(60).clamp(15, 180),
                "lunch_rest" => s.lunch_rest = v != "0",
                "lunch_start" => s.lunch_start = v.parse::<u32>().unwrap_or(720).min(1439),
                "lunch_end" => s.lunch_end = v.parse::<u32>().unwrap_or(810).min(1439),
                "mail_enabled" => s.mail_enabled = v != "0",
                "mail_sync_operations" => s.mail_sync_operations=v=="1",
                "mail_code" => s.mail_code = v != "0",
                "mail_code_hide_seconds" => s.mail_code_hide_seconds=v.parse::<u32>().unwrap_or(60).clamp(15,300),
                "mail_interval" => s.mail_interval = match v.parse::<u32>().unwrap_or(120) { 60 => 60, 300 => 300, _ => 120 },
                "ignored_mail_events" => s.ignored_mail_events = serde_json::from_str(&v).unwrap_or_default(),
                "mail_ics" => s.mail_ics = match v.parse::<u32>().unwrap_or(15) { 5 => 5, 30 => 30, _ => 15 },
                "calendar_prompt" => s.calendar_prompt = v != "0",
                "calendar_auto_sync" => s.calendar_auto_sync = v != "0",
                "clipboard_history" => s.clipboard_history = v != "0",
                "weather_city" => s.weather_city = v.trim().to_string(),
                "weather_position" => s.weather_position=serde_json::from_str::<Option<crate::weather::Position>>(&v).ok().flatten().filter(|p|p.valid()),
                "weather_saved" => s.weather_saved=serde_json::from_str::<Vec<crate::weather::Position>>(&v).unwrap_or_default().into_iter().filter(|p|p.valid()).collect(),
                "render_compat" => s.render_compat = v == "1",
                "render_compat_reason" => s.render_compat_reason = v.chars().filter(|c| !c.is_control()).take(200).collect(),
                "gws_onboarded" => s.gws_onboarded = v == "1",
                _ => {}
            }
        }
        if !s.visible_pages.contains(&true) { s.visible_pages[0]=true; }
        // 舊版只有一個指定城市：遷移進已儲存列表，升級後下拉框裡能直接看到它。
        if let Some(position)=s.weather_position.clone().filter(|p|p.label==s.weather_city) {
            if !s.weather_saved.contains(&position) {s.weather_saved.insert(0,position);}
        }
        s.weather_saved.dedup();s.weather_saved.truncate(crate::weather::MAX_CITIES);
        s
    }

    pub fn save(&self) {
        let _ = write_kv(
            &config_dir().join("settings.txt"),
            &[
                ("display_id".into(),self.display_id.clone()),
                ("skin".into(),self.skin.key().into()),
                ("island_shape".into(),self.island_shape.key().into()),
                ("dock".into(),self.dock.key().into()),
                ("glass_blur".into(),(self.glass_blur as u8).to_string()),
                ("island_size".into(),island_size_value(self.island_size).to_string()),
                ("island_offset".into(),self.island_offset.clamp(-ISLAND_OFFSET_MAX,ISLAND_OFFSET_MAX).to_string()),
                ("oled_shift".into(),(self.oled_shift as u8).to_string()),
                ("settings_theme".into(),self.settings_theme.key().into()),
                ("pet".into(), if self.pet { "1".to_string() } else { "0".to_string() }),
                ("minimal".into(), if self.minimal { "1".to_string() } else { "0".to_string() }),
                ("browser_minimal".into(), (self.browser_minimal as u8).to_string()),
                ("avoid_browsers".into(), (self.avoid_browsers as u8).to_string()),
                ("avoid_apps".into(), serde_json::to_string(&self.avoid_apps).unwrap_or_default()),
                ("visible_pages".into(), self.visible_pages.iter().map(|enabled|if *enabled {'1'}else{'0'}).collect()),
                ("page_order".into(),self.page_order.iter().map(usize::to_string).collect::<Vec<_>>().join(",")),
                ("resident_order".into(),self.resident_order.iter().map(usize::to_string).collect::<Vec<_>>().join(",")),
                ("clock_period".into(), self.clock_period.to_string()),
                ("clock_12h".into(), (self.clock_12h as u8).to_string()),
                ("clock_seconds".into(), (self.clock_seconds as u8).to_string()),
                ("show_battery".into(), (self.show_battery as u8).to_string()),
                ("show_clock".into(), (self.show_clock as u8).to_string()),
                ("show_weather".into(), (self.show_weather as u8).to_string()),
                ("read_notifications".into(),(self.read_notifications as u8).to_string()),
                ("wechat_notifications".into(),(self.wechat_notifications as u8).to_string()),
                ("wecom_notifications".into(),(self.wecom_notifications as u8).to_string()),
                ("feishu_notifications".into(),(self.feishu_notifications as u8).to_string()),
                ("dingtalk_notifications".into(),(self.dingtalk_notifications as u8).to_string()),
                ("wechat_tray_open".into(),(self.wechat_tray_open as u8).to_string()),
                ("qq_only".into(),(self.qq_only as u8).to_string()),
                ("notification_preview".into(),(self.notification_preview as u8).to_string()),
                ("system_hud".into(), (self.system_hud as u8).to_string()),
                ("bluetooth_notifications".into(), (self.bluetooth_notifications as u8).to_string()),
                ("notifications".into(), (self.notifications as u8).to_string()),
                ("sound".into(), (self.sound as u8).to_string()),
                ("lyrics_enabled".into(),(self.lyrics_enabled as u8).to_string()),
                ("island_lyrics".into(),(self.island_lyrics as u8).to_string()),
                ("lyric_delay_ms".into(),self.lyric_delay_ms.to_string()),
                ("hide_fullscreen".into(), (self.hide_fullscreen as u8).to_string()),
                ("avoid_top_bar".into(), (self.avoid_top_bar as u8).to_string()),
                ("top_bar_on_top".into(), (self.top_bar_on_top as u8).to_string()),
                ("focus_minutes".into(), self.focus_minutes.to_string()),
                ("rest_minutes".into(), self.rest_minutes.to_string()),
                ("water_minutes".into(), self.water_minutes.to_string()),
                ("serving_ml".into(), self.serving_ml.to_string()),
                ("water_start".into(), self.water_start.to_string()),
                ("water_end".into(), self.water_end.to_string()),
                ("sit_remind".into(), (self.sit_remind as u8).to_string()),
                ("sit_minutes".into(), self.sit_minutes.to_string()),
                ("lunch_rest".into(), (self.lunch_rest as u8).to_string()),
                ("lunch_start".into(), self.lunch_start.to_string()),
                ("lunch_end".into(), self.lunch_end.to_string()),
                ("mail_enabled".into(), (self.mail_enabled as u8).to_string()),
                ("mail_sync_operations".into(),(self.mail_sync_operations as u8).to_string()),
                ("mail_code".into(), (self.mail_code as u8).to_string()),
                ("mail_code_hide_seconds".into(), self.mail_code_hide_seconds.to_string()),
                ("mail_interval".into(), self.mail_interval.to_string()),
                ("mail_ics".into(), self.mail_ics.to_string()),
                ("ignored_mail_events".into(), serde_json::to_string(&self.ignored_mail_events).unwrap()),
                ("calendar_prompt".into(), (self.calendar_prompt as u8).to_string()),
                ("calendar_auto_sync".into(), (self.calendar_auto_sync as u8).to_string()),
                ("clipboard_history".into(), (self.clipboard_history as u8).to_string()),
                ("weather_city".into(), self.weather_city.clone()),
                ("weather_position".into(), serde_json::to_string(&self.weather_position).unwrap()),
                ("weather_saved".into(), serde_json::to_string(&self.weather_saved).unwrap()),
                ("render_compat".into(), (self.render_compat as u8).to_string()),
                ("render_compat_reason".into(), self.render_compat_reason.clone()),
                ("gws_onboarded".into(), (self.gws_onboarded as u8).to_string()),
            ],
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct TodoItem {
    pub text: String,
    /// 一次性待辦：完成日期；每週重複：最近一次完成的日期（只算當天）。YYYY-MM-DD
    pub done_on: Option<String>,
    /// 本地日期與時間：YYYY-MM-DD HH:MM；每週重複只用其中的時間。
    pub due: Option<String>,
    pub reminded: bool,
    /// 每週重複：bit0 = 週一 … bit6 = 週日；0 = 不重複（一次性待辦）。
    pub repeat: u8,
    /// 每週重複：當天已提醒的日期，第二天自動重新提醒。
    pub reminded_on: Option<String>,
}

pub const WEEKDAY_NAMES: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];
pub const REPEAT_ALL: u8 = 0b111_1111;
pub const REPEAT_WORKDAYS: u8 = 0b001_1111;
pub const REPEAT_WEEKEND: u8 = 0b110_0000;

fn weekday_bit(date: &str) -> Option<u8> {
    use chrono::Datelike;
    let day = chrono::NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()?;
    Some(1 << day.weekday().num_days_from_monday())
}

/// 「每天」「工作日」「週末」「每週一、三、五」。
pub fn repeat_label(mask: u8) -> String {
    match mask & REPEAT_ALL {
        0 => "不重複".into(),
        REPEAT_ALL => "每天".into(),
        REPEAT_WORKDAYS => "工作日".into(),
        REPEAT_WEEKEND => "週末".into(),
        mask => {
            let days: Vec<&str> = (0..7).filter(|i| mask & (1 << i) != 0).map(|i| WEEKDAY_NAMES[i]).collect();
            format!("每週{}", days.join("、"))
        }
    }
}

/// 列表裡的短標籤：「每天」「工作日」「週末」「週一三五」。
pub fn repeat_tag(mask: u8) -> String {
    match mask & REPEAT_ALL {
        0 => String::new(),
        REPEAT_ALL => "每天".into(),
        REPEAT_WORKDAYS => "工作日".into(),
        REPEAT_WEEKEND => "週末".into(),
        mask => format!("週{}", (0..7).filter(|i| mask & (1 << i) != 0).map(|i| WEEKDAY_NAMES[i]).collect::<String>()),
    }
}

impl TodoItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Self::default() }
    }

    pub fn is_repeating(&self) -> bool { self.repeat & REPEAT_ALL != 0 }

    /// 這一天是否要做：一次性待辦每天都顯示；重複待辦只在選中的星期幾顯示。
    pub fn scheduled_on(&self, today: &str) -> bool {
        !self.is_repeating() || weekday_bit(today).is_some_and(|bit| self.repeat & bit != 0)
    }

    /// 今天是否已完成：重複待辦第二天自動變回未完成。
    pub fn done_for(&self, today: &str) -> bool {
        if self.is_repeating() { self.done_on.as_deref() == Some(today) } else { self.done_on.is_some() }
    }

    /// 勾選 / 取消勾選。
    pub fn toggle(&mut self, today: &str) {
        self.done_on = if self.done_for(today) { None } else { Some(today.to_string()) };
    }

    /// `now` 為 `YYYY-MM-DD HH:MM`。一次性待辦按完整時間提醒一次；
    /// 重複待辦在每個選中的日子到點提醒一次。
    pub fn reminder_due(&self, now: &str) -> bool {
        let Some(due) = self.due.as_deref() else { return false; };
        let today = now.get(..10).unwrap_or("");
        if self.done_for(today) { return false; }
        if !self.is_repeating() { return !self.reminded && due <= now; }
        let (Some(at), Some(clock)) = (due.get(11..16), now.get(11..16)) else { return false; };
        self.scheduled_on(today) && self.reminded_on.as_deref() != Some(today) && at <= clock
    }

    pub fn mark_reminded(&mut self, now: &str) {
        if self.is_repeating() { self.reminded_on = now.get(..10).map(str::to_string); } else { self.reminded = true; }
    }

    /// 懸停提示裡的提醒說明。
    pub fn reminder_text(&self) -> Option<String> {
        let due = self.due.as_deref()?;
        Some(if self.is_repeating() { format!("{} {}", repeat_label(self.repeat), due.get(11..16).unwrap_or(due)) } else { due.to_string() })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TodoList {
    pub items: Vec<TodoItem>,
}

impl TodoList {
    pub fn load() -> Self {
        fs::read_to_string(config_dir().join("todo.txt")).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut list = Self::default();
        {
            for line in text.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                // 格式：<x 或 o>|<完成日期或 ->|正文；也接受純文字行（視為未完成）
                if !line.contains('|') {
                    list.items.push(TodoItem::new(line.trim()));
                    continue;
                }
                // 每週重複的標記寫成 `r<星期位>`（舊版本讀到會當作未完成的普通待辦）。
                let mut parts = line.splitn(3, '|');
                let flag = parts.next().unwrap_or("o");
                let when = parts.next().unwrap_or("-");
                let raw = parts.next().unwrap_or("").trim();
                let repeat = flag.strip_prefix('r').and_then(|bits| bits.parse::<u8>().ok()).unwrap_or(0) & REPEAT_ALL;
                let (due, reminded_field, text) = if let Some(rest) = raw.strip_prefix("@due:") {
                    let mut fields = rest.splitn(3, '|');
                    let due = fields.next().unwrap_or("").to_string();
                    let reminded = fields.next().unwrap_or("0").to_string();
                    (Some(due), reminded, fields.next().unwrap_or("").to_string())
                } else { (None, "0".to_string(), raw.to_string()) };
                if text.is_empty() {
                    continue;
                }
                let done = (flag == "x" || repeat != 0) && when != "-";
                list.items.push(TodoItem {
                    text, due, repeat,
                    reminded: repeat == 0 && reminded_field == "1",
                    reminded_on: (repeat != 0 && reminded_field.len() == 10).then_some(reminded_field),
                    done_on: done.then(|| when.to_string()),
                });
            }
        }
        list
    }

    pub fn save(&self) {
        let body = self.serialize();
        let path = config_dir().join("todo.txt");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = atomic_write(&path, body.as_bytes());
    }

    pub fn serialize(&self) -> String {
        self
            .items
            .iter()
            .map(|i| {
                let state = if i.is_repeating() {
                    format!("r{}|{}", i.repeat & REPEAT_ALL, i.done_on.as_deref().unwrap_or("-"))
                } else {
                    i.done_on.as_ref().map(|d| format!("x|{d}")).unwrap_or_else(|| "o|-".into())
                };
                let text = i.text.replace(['\n', '\r'], " ");
                let reminded = if i.is_repeating() { i.reminded_on.clone().unwrap_or_else(|| "0".into()) } else { (i.reminded as u8).to_string() };
                match &i.due {
                    Some(due) => format!("{state}|@due:{due}|{reminded}|{text}\n"),
                    None => format!("{state}|{text}\n"),
                }
            })
            .collect()
    }

    pub fn done_today(&self, today: &str) -> usize {
        self.items
            .iter()
            .filter(|i| i.done_on.as_deref() == Some(today))
            .count()
    }

    /// 今天要做的待辦（不在今天的重複待辦除外）中，(已完成, 總數)。
    pub fn progress(&self, today: &str) -> (usize, usize) {
        let scheduled = self.items.iter().filter(|i| i.scheduled_on(today));
        let (mut done, mut total) = (0, 0);
        for item in scheduled { total += 1; if item.done_for(today) { done += 1; } }
        (done, total)
    }

    /// 今天未完成在前，其他日期的未完成其次，已完成沉底；原始儲存順序不變。
    pub fn display_order(&self, today: &str) -> Vec<usize> {
        let mut order:Vec<_>=(0..self.items.len()).collect();
        order.sort_by_key(|&i|{let item=&self.items[i];(item.done_for(today),!item.scheduled_on(today))});
        order
    }
}

#[derive(Clone, Debug)]
pub struct Water {
    pub bottle_ml: u32,
    pub goal_ml: u32,
    pub start_day: String,
    pub drunk_ml: u32,
    pub refills: u32,
    /// 每小時飲水量（ml），給飲水頁的柱狀圖用。
    pub hours: [u32; 24],
    /// 定時提醒開關。
    pub remind: bool,
}

impl Default for Water {
    fn default() -> Self {
        Self { bottle_ml: 2000, goal_ml: 2000, start_day: String::new(), drunk_ml: 0, refills: 0, hours: [0; 24], remind: true }
    }
}

impl Water {
    /// `water.txt` 只寫原工具已知的三個鍵；復刻版新增的進度鍵寫到 `water-extra.txt`，
    /// 避免給原工具的檔案引入未知鍵。
    pub fn load(today: &str) -> Self {
        let mut w = Self::default();
        for (k, v) in read_kv(&config_dir().join("water.txt")) {
            match k.as_str() {
                "bottle" => w.bottle_ml = v.parse::<f32>().map(|x| x as u32).unwrap_or(2000),
                "goal" => w.goal_ml = v.parse().unwrap_or(2000),
                "start" => w.start_day = v,
                _ => {}
            }
        }
        for (k, v) in read_kv(&config_dir().join("water-extra.txt")) {
            match k.as_str() {
                "drunk" => w.drunk_ml = v.parse().unwrap_or(0),
                "refills" => w.refills = v.parse().unwrap_or(0),
                "remind" => w.remind = v != "0",
                "hours" => {
                    for (i, x) in v.split(',').take(24).enumerate() {
                        w.hours[i] = x.trim().parse().unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        w.goal_ml=w.goal_ml.clamp(500,6000);
        w.bottle_ml=w.bottle_ml.clamp(50,6000);
        if w.start_day != today {
            // 跨天重置，與原工具 start= 欄位語義一致
            w.start_day = today.to_string();
            w.drunk_ml = 0;
            w.refills = 0;
            w.hours = [0; 24];
            w.save();
        }
        w
    }

    pub fn save(&self) {
        let _ = write_kv(
            &config_dir().join("water.txt"),
            &[
                ("bottle".into(), self.bottle_ml.to_string()),
                ("goal".into(), self.goal_ml.to_string()),
                ("start".into(), self.start_day.clone()),
            ],
        );
        let _ = write_kv(
            &config_dir().join("water-extra.txt"),
            &[
                ("drunk".into(), self.drunk_ml.to_string()),
                ("refills".into(), self.refills.to_string()),
                ("remind".into(), if self.remind { "1".into() } else { "0".into() }),
                (
                    "hours".into(),
                    self.hours.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(","),
                ),
            ],
        );
    }

    /// 喝一口：計入總量與當前小時。
    pub fn drink(&mut self, ml: u32, hour: usize) {
        let total=self.drunk_ml.saturating_add(ml).min(self.goal_ml.max(ml).saturating_mul(4));
        let added=total.saturating_sub(self.drunk_ml);
        self.drunk_ml=total;
        self.hours[hour.min(23)]=self.hours[hour.min(23)].saturating_add(added);
        self.save();
    }

    pub fn percent(&self) -> f32 {
        if self.goal_ml == 0 {
            0.0
        } else {
            (self.drunk_ml as f32 / self.goal_ml as f32 * 100.0).min(100.0)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub items: Vec<PathBuf>,
}

impl Shelf {
    pub fn load() -> Self {
        let mut s = Self::default();
        if let Ok(text) = fs::read_to_string(config_dir().join("shelf.txt")) {
            for line in text.lines() {
                let p = PathBuf::from(line.trim());
                if !line.trim().is_empty() && !s.items.contains(&p) {
                    s.items.push(p);
                }
            }
        }
        s
    }

    pub fn save(&self) {
        let body: String = self
            .items
            .iter()
            .map(|p| format!("{}\n", p.display()))
            .collect();
        let path = config_dir().join("shelf.txt");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = atomic_write(&path, body.as_bytes());
    }

    /// 原工具的做法：記錄檔案所在位置，不復制檔案本體（官網說明
    /// "the shelf remembers where your files live"）。
    pub fn add(&mut self, src: &Path) -> std::io::Result<()> {
        if !src.exists() { return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "檔案不存在")); }
        if !self.items.iter().any(|p| p == src) {
            self.items.push(src.to_path_buf());
            self.save();
        }
        Ok(())
    }

    pub fn remove(&mut self, p: &Path) {
        self.items.retain(|item| item != p);
        self.save();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.save();
    }
}

pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.1} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_appearance_values_are_normalized() {
        assert_eq!(island_size_value(100), 100);
        assert_eq!(island_size_value(10), ISLAND_SIZE_MIN);
        assert_eq!(island_size_value(999), ISLAND_SIZE_MAX);
        assert_eq!(island_size_value(113), 115);
        assert_eq!(island_size_value(112), 110);
        for skin in Skin::ALL { assert_eq!(Skin::parse(skin.key()), skin); }
        assert_eq!(Skin::parse("unknown"), Skin::Classic);
        assert_eq!(Skin::parse("glass"), Skin::GlassDark, "舊版本儲存的毛玻璃即深色毛玻璃");
        for shape in IslandShape::ALL { assert_eq!(IslandShape::parse(shape.key()), shape); }
        assert_eq!(Settings::default().island_shape, IslandShape::Notch);
        let mut s = Settings::default();
        assert_eq!((s.skin, s.glass_blur, s.island_scale(), s.island_offset_ratio()), (Skin::Classic, true, 1.0, 0.0));
        s.island_size = 125; s.island_offset = -250;
        assert_eq!((s.island_scale(), s.island_offset_ratio()), (1.25, -1.0));
    }

    #[test]
    fn quality_save_preserves_neighbor_and_cleans_failed_writes() {
        let dir=std::env::temp_dir().join(format!("daozai-quality-save-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path=dir.join("settings.txt");let neighbor=dir.join("settings.tmp");
        fs::write(&neighbor,b"other data").unwrap();
        let saved=atomic_write(&path,b"settings");
        let untouched=fs::read(&neighbor).ok()==Some(b"other data".to_vec());
        let blocked=dir.join("blocked.txt");fs::create_dir_all(&blocked).unwrap();
        assert!(atomic_write(&blocked,b"cannot replace directory").is_err());
        let leftovers=fs::read_dir(&dir).unwrap().flatten().filter(|e|e.path()!=path && e.path()!=neighbor && e.path()!=blocked).count();
        let failed_tmp=blocked.with_extension("tmp").exists();
        for entry in fs::read_dir(&dir).unwrap().flatten(){if entry.file_type().unwrap().is_file(){fs::remove_file(entry.path()).unwrap();}}
        fs::remove_dir(blocked).unwrap();fs::remove_dir(dir).unwrap();
        assert!(saved.is_ok() && untouched,"儲存不能覆蓋或移走同名 .tmp 檔案");
        assert!(!failed_tmp && leftovers==0,"失敗的儲存應清除本次臨時檔案");
    }

    #[test]
    fn atomic_write_replaces_existing_settings() {
        let dir=std::env::temp_dir().join(format!("daozai-save-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&dir).unwrap();
        let path=dir.join("settings.txt");
        atomic_write(&path,b"focus_minutes=25\n").unwrap();
        atomic_write(&path,b"focus_minutes=26\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(),b"focus_minutes=26\n");
        assert!(!path.with_extension("tmp").exists(),"儲存後不應遺留臨時配置");
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn quality_concurrent_saves_publish_one_complete_value() {
        let dir=std::env::temp_dir().join(format!("daozai-quality-concurrent-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();
        let path=dir.join("cache.lrc");let gate=std::sync::Arc::new(std::sync::Barrier::new(4));
        let workers:Vec<_>=(0..4u8).map(|byte|{let path=path.clone();let gate=gate.clone();std::thread::spawn(move||{gate.wait();atomic_write(&path,&vec![byte;128*1024])})}).collect();
        for worker in workers{worker.join().unwrap().unwrap();}
        let content=fs::read(&path).unwrap();assert_eq!(content.len(),128*1024);assert!(content.iter().all(|b|*b==content[0]));
        assert_eq!(fs::read_dir(&dir).unwrap().count(),1);fs::remove_file(path).unwrap();fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn weekly_todos_round_trip_and_reset_each_day() {
        // 2026-11-02 是週一。
        let mut weekly = TodoItem::new("週會 | 準備材料");
        weekly.repeat = 0b000_0101; // 週一、週三
        weekly.due = Some("2026-11-02 09:30".into());
        weekly.toggle("2026-11-02");
        weekly.reminded_on = Some("2026-11-02".into());
        let once = TodoItem { done_on: Some("2026-11-01".into()), reminded: true, due: Some("2026-11-01 08:00".into()), ..TodoItem::new("一次性") };
        let list = TodoList { items: vec![weekly.clone(), once.clone()] };
        let loaded = TodoList::parse(&list.serialize());
        assert_eq!(loaded.items.len(), 2);
        let w = &loaded.items[0];
        assert_eq!((w.text.as_str(), w.repeat, w.done_on.as_deref(), w.reminded_on.as_deref()), ("週會 | 準備材料", 0b101, Some("2026-11-02"), Some("2026-11-02")));
        assert!(w.done_for("2026-11-02") && !w.done_for("2026-11-04"), "重複待辦第二天自動變回未完成");
        assert!(w.scheduled_on("2026-11-04") && !w.scheduled_on("2026-11-03"));
        assert!(!w.reminder_due("2026-11-02 10:00"), "當天已提醒");
        assert!(!w.reminder_due("2026-11-04 09:00") && w.reminder_due("2026-11-04 09:30"));
        assert!(!w.reminder_due("2026-11-03 10:00"), "不在選中的星期幾不提醒");
        let o = &loaded.items[1];
        assert_eq!((o.repeat, o.reminded, o.done_on.as_deref()), (0, true, Some("2026-11-01")));
        assert!(o.done_for("2026-11-09") && o.scheduled_on("2026-11-03"));
        assert_eq!(loaded.progress("2026-11-03"), (1, 1));
        assert_eq!(loaded.progress("2026-11-04"), (1, 2));
        assert_eq!(loaded.display_order("2026-11-03"), vec![0, 1],"已完成事項應排在其他日期的未完成事項後面");
        assert_eq!(repeat_label(REPEAT_WORKDAYS), "工作日");
        assert_eq!(repeat_label(0b101_0101), "每週一、三、五、日");
        assert_eq!(repeat_tag(0b001_0101), "週一三五");
    }

    #[test]
    fn human_size_formats_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536 * 1024), "1.5 MB");
        assert_eq!(human_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn water_percent_clamps_at_100() {
        let w = Water { drunk_ml: 1000, ..Water::default() };
        assert!((w.percent() - 50.0).abs() < 0.01);
        let w = Water { drunk_ml: 5000, ..w };
        assert!((w.percent() - 100.0).abs() < 0.01);
    }
}

/// 僅儲存應用標識與開關，不儲存通知內容。
#[derive(Clone,Debug,Default)]
pub struct NotificationApps {pub items:Vec<(String,String,bool)>}
impl NotificationApps {
    pub fn load()->Self {
        let path=config_dir().join("notification-apps.json");
        let Ok(raw)=fs::read(path) else{return Self::default()};
        let Ok(json)=serde_json::from_slice::<serde_json::Value>(&raw) else{return Self::default()};
        let items=json.as_array().into_iter().flatten().filter_map(|a|Some((a.get("id")?.as_str()?.to_owned(),a.get("name")?.as_str()?.to_owned(),a.get("enabled")?.as_bool()?))).collect();Self{items}
    }
    pub fn save(&self){
        let list:Vec<_>=self.items.iter().map(|(id,name,enabled)|serde_json::json!({"id":id,"name":name,"enabled":enabled})).collect();
        let path=config_dir().join("notification-apps.json");if let Some(parent)=path.parent(){let _=fs::create_dir_all(parent);}if let Ok(data)=serde_json::to_vec(&list){let _=atomic_write(&path,&data);}
    }
    pub fn observe(&mut self,id:&str,name:&str)->bool{
        if let Some(item)=self.items.iter().find(|a|a.0==id){return item.2;}
        let enabled=true;
        self.items.push((id.to_string(),name.to_string(),enabled));self.save();enabled
    }
}
