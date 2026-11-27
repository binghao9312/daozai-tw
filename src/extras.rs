//! 設定、通知佇列與島仔表情。
use crate::settings_theme::color as settings_color;
use crate::app::{App, Cmd, Env, Phase, HUD_TRACK_W, HUD_TRACK_X, HUD_W, PAGE_NAMES};
use crate::paint::{Card, H};
use crate::theme::{self, size, W};
use eframe::egui::{self, Color32, Rect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AlertSource { Internal, Messenger(crate::app::Messenger), External(String) }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum HealthReminder {Water,Sit}
impl HealthReminder {
    pub(crate) const ALL:[Self;2]=[Self::Water,Self::Sit];
    pub(crate) fn title(self)->&'static str {match self {Self::Water=>"喝水時間到了",Self::Sit=>crate::app::SIT_TITLE}}
    pub(crate) fn label(self)->&'static str {match self {Self::Water=>"待喝水",Self::Sit=>"待活動"}}
    pub(crate) fn id(self)->&'static str {match self {Self::Water=>"health-water",Self::Sit=>"health-sit"}}
    pub(crate) fn icon(self,c:&Card,x:f32,y:f32,scale:f32) {
        let c=&c.scaled(x,y,scale);
        match self {
            Self::Water=>{
                c.poly(&[(x-7.0,y-1.0),(x+5.0,y-1.0),(x+3.5,y+8.0),(x-5.5,y+8.0)],theme::BLUE);
                c.path(&[(x-9.0,y-10.0),(x-6.5,y+9.0),(x+4.5,y+9.0),(x+7.0,y-10.0)],1.6,theme::CYAN);
                c.seg((x-9.0,y-10.0),(x+7.0,y-10.0),1.6,theme::CYAN);
                c.path(&[(x+6.5,y-6.0),(x+10.5,y-6.0),(x+11.5,y-3.0),(x+10.0,y+2.0),(x+5.5,y+3.0)],1.6,theme::CYAN);
                c.seg((x-5.0,y-6.5),(x-4.2,y-2.5),1.0,Color32::from_white_alpha(170));
            },
            Self::Sit=>crate::paint::chair(c,x,y,0.6,theme::SOFT_GREEN),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Alert {
    pub title: String,
    pub source: AlertSource,
    pub body: String,
    pub page: usize,
    pub at: f64,
    pub duration: f64,
    pub bluetooth: Option<(crate::bt::DeviceKind, bool)>,
    pub meeting: Option<(crate::mail::Meeting, String)>,
    pub unread: Option<crate::app::Unread>,
}
impl Alert {
    pub(crate) fn health(&self)->Option<HealthReminder> {
        if self.source!=AlertSource::Internal{return None;}
        HealthReminder::ALL.into_iter().find(|kind|self.title==kind.title())
    }
    pub(crate) fn messenger(&self)->Option<crate::app::Messenger>{
        match &self.source {
            AlertSource::Messenger(client)=>Some(*client),
            AlertSource::External(id)=>crate::app::Messenger::from_notification_id(id).filter(|client|client.adapter().capture==crate::messenger::Capture::SystemToast),
            _=>None,
        }
    }
    pub(crate) fn is_hud(&self)->bool {self.source==AlertSource::Internal && matches!(self.title.as_str(),"系統音量"|"大寫鎖定"|"鍵盤輸入語言"|"網路連線"|"電源已連線"|"已切換電池供電"|"藍芽裝置")}
}
fn bluetooth_notice_content(body:&str)->(&str,&str,Option<u8>){
    if let Some(name)=body.strip_suffix(" · 已斷開") {return (name,"已斷開",None);}
    if body=="已斷開連線" {return ("藍芽裝置","已斷開",None);}
    if let Some((name,level))=body.rsplit_once(" · 電量 ") {
        if let Some(level)=level.strip_suffix('%').and_then(|s|s.parse::<u8>().ok()).filter(|v|*v<=100){
            return (name,"已連線",Some(level));
        }
    }
    (body,"已連線",None)
}

pub(crate) fn device_icon(c: &Card, kind: crate::bt::DeviceKind, x: f32, y: f32, size: f32) {
    let icon=match kind {
        crate::bt::DeviceKind::Bone | crate::bt::DeviceKind::Earbuds | crate::bt::DeviceKind::Headphones=>egui::include_image!("../assets/device-icons/headphones-round-bold.svg"),
        crate::bt::DeviceKind::Speaker=>egui::include_image!("../assets/device-icons/speaker-bold.svg"),
        crate::bt::DeviceKind::Keyboard=>egui::include_image!("../assets/device-icons/keyboard-bold.svg"),
        crate::bt::DeviceKind::Mouse=>egui::include_image!("../assets/device-icons/mouse-bold.svg"),
        crate::bt::DeviceKind::Gamepad=>egui::include_image!("../assets/device-icons/gamepad-bold.svg"),
        crate::bt::DeviceKind::Phone=>egui::include_image!("../assets/device-icons/smartphone-bold.svg"),
        crate::bt::DeviceKind::Other=>egui::include_image!("../assets/device-icons/bluetooth-linear.svg"),
    };
    let rect=c.rect(x,y,size,size);
    let egui::ImageSource::Bytes{uri,bytes}=icon else{return};
    let pixels=(rect.width()*c.ui.ctx().pixels_per_point()).ceil() as u32;
    let icon=egui::Image::from_bytes(format!("{uri}/{pixels}.svg"),bytes);
    if let Ok(egui::load::TexturePoll::Ready { texture })=icon.fit_to_exact_size(rect.size()).load_for_size(c.ui.ctx(),rect.size()) {
        c.painter.image(texture.id,rect,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
    }
}

pub(crate) use crate::pet::PetState;

pub(crate) fn notice_badge(c:&Card,x:f32,y:f32,a:&Alert){
    if a.source==AlertSource::Internal && a.title=="藍芽裝置"{
        c.circle(x,y,11.0,if a.body.contains("已斷開"){Color32::from_gray(95)}else{theme::BLUE});
        device_icon(c,a.bluetooth.map_or(crate::bt::DeviceKind::Other,|v|v.0),x-7.5,y-7.5,15.0);
    }else{notice_icon(&c.scaled(x,y,0.85),x,y,a);}
}
pub(crate) fn messenger_icon(c:&Card,x:f32,y:f32,source:crate::app::Messenger){(source.adapter().paint_icon)(c,x,y);}
fn notice_icon(c:&Card,x:f32,y:f32,a:&Alert) {
    if a.source!=AlertSource::Internal{crate::paint::bell(c,x,y,theme::WHITE,0.0);return;}
    match a.title.as_str() {
        "系統音量"=>{
            c.poly(&[(x-9.0,y-3.0),(x-5.0,y-3.0),(x,y-7.0),(x,y+7.0),(x-5.0,y+3.0),(x-9.0,y+3.0)],theme::WHITE);
            c.arc(x,y,5.0,-0.8,0.8,1.5,theme::WHITE);c.arc(x,y,8.0,-0.7,0.7,1.3,theme::WHITE);
        },
        "大寫鎖定"=>{
            let ink=if a.body=="已開啟"{theme::GREEN}else{theme::GRAY};
            c.poly(&[(x-8.0,y-1.0),(x,y-9.0),(x+8.0,y-1.0)],ink);
            c.fill(x-3.0,y-1.5,6.0,6.5,0.5,ink);
            c.fill(x-4.0,y+7.0,8.0,2.0,0.8,ink);
        },
        "鍵盤輸入語言"=>{
            c.stroke(x-10.0,y-8.5,20.0,17.0,4.0,1.2,theme::GRAY);
            let label=match a.body.as_str(){"中文"=>"中","英語"=>"EN",_=>"Aa"};
            c.centered_text(x,y,label,if label=="EN"{9.0}else{11.0},W::Semibold,theme::WHITE,H::Center);
        },
        "網路連線"=>{
            let connected=a.body.contains("已連線");
            let ink=if connected{theme::GREEN}else{theme::GRAY};
            if a.body.starts_with("Wi-Fi") {
                for r in [4.0,8.0,12.0]{c.arc(x,y+6.0,r,-2.35,-0.79,1.7,ink);}
                c.circle(x,y+6.0,1.6,ink);
            }else if a.body.starts_with("乙太網") {
                c.stroke(x-8.0,y-8.0,16.0,14.0,2.5,1.6,ink);
                for dx in [-4.0,-1.3,1.3,4.0]{c.seg((x+dx,y-5.0),(x+dx,y-1.0),1.2,ink);}
                c.path(&[(x-4.0,y+6.0),(x-4.0,y+9.0),(x+4.0,y+9.0),(x+4.0,y+6.0)],1.6,ink);
            }else{
                c.ring(x,y,8.0,1.4,ink);c.seg((x-8.0,y),(x+8.0,y),1.0,ink);
                c.stroke(x-3.5,y-8.0,7.0,16.0,3.5,1.0,ink);
            }
            if !connected{c.seg((x-8.0,y-8.0),(x+9.0,y+9.0),2.0,theme::WHITE);}
        },
        "電源已連線"|"已切換電池供電"|"電量偏低"=>{
            let low=a.title=="電量偏低";let ink=if low{theme::RED_TOP}else if a.title=="電源已連線"{theme::GREEN}else{theme::WHITE};
            c.stroke(x-11.0,y-6.0,20.0,12.0,3.0,1.5,ink);c.fill(x+10.0,y-2.5,2.0,5.0,0.8,ink);
            if low{c.fill(x-8.0,y-3.5,3.5,7.0,1.0,ink);}
            else if a.title=="電源已連線"{
                c.poly(&[(x,y-4.5),(x-4.0,y+0.8),(x+0.5,y+0.8)],ink);
                c.poly(&[(x+0.5,y-0.8),(x+4.0,y-0.8),(x-1.0,y+4.5)],ink);
            }else{c.fill(x-8.0,y-3.5,12.0,7.0,1.0,ink);}
        },
        "喝水時間到了"=>HealthReminder::Water.icon(c,x,y,1.0),
        crate::app::SIT_TITLE=>HealthReminder::Sit.icon(c,x,y,1.0),
        "待辦到時間了"=>crate::paint::checklist(c,x,y,23.0,Color32::from_rgb(190,180,255),Color32::from_rgb(43,35,66)),
        "會議提醒"=>{
            let label=a.meeting.as_ref().map(|(_,source)|source.as_str()).unwrap_or("");
            let source=if label=="本機日程"{crate::calendar_ui::Source::Local}else if label.starts_with("郵件"){crate::calendar_ui::Source::Mail("")}
                else if label==crate::calendar::Provider::GoogleWorkspace.label(){crate::calendar_ui::Source::Calendar(Some(crate::calendar::Provider::GoogleWorkspace))}
                else{crate::calendar_ui::Source::Calendar(crate::calendar::Provider::ALL.into_iter().find(|provider|provider.label()==label))};
            crate::calendar_ui::source_icon(c,source,x-14.0,y-14.0,28.0);
        },
        "專注計時"=>crate::paint::stopwatch(&c.scaled(x,y,0.65),x,y,8.0,0.0,theme::RED_TOP),
        _=>crate::paint::bell(&c.scaled(x,y,1.7),x,y,theme::GRAY,0.0),
    }
}

impl App {
    pub(crate) fn health_badges(&self)->Vec<HealthReminder> {
        if self.fullscreen_hidden || !self.module_on(crate::app::WATER_PAGE){return Vec::new();}
        HealthReminder::ALL.into_iter().filter(|kind|self.health_pending[*kind as usize]
            && (match kind {HealthReminder::Water=>self.water.remind,HealthReminder::Sit=>self.settings.sit_remind})
            && (self.open || !self.notice.as_ref().is_some_and(|a|a.health()==Some(*kind)))).collect()
    }
    pub(crate) fn confirm_health_reminder(&mut self,kind:HealthReminder) {
        self.health_pending[kind as usize]=false;
        crate::model::save_health_pending(self.health_pending);
        match kind {HealthReminder::Water=>self.last_reminder=self.t,HealthReminder::Sit=>self.sit_start=self.t}
        self.alerts.retain(|a|a.health()!=Some(kind));
        if self.notice.as_ref().is_some_and(|a|a.health()==Some(kind)){self.notice=None;}
        if self.hud_notice.as_ref().is_some_and(|a|a.health()==Some(kind)){self.hud_notice=None;}
    }
    pub(crate) fn phase_secs(&self, phase: Phase) -> f32 {
        (match phase {
            Phase::Focus => self.settings.focus_minutes,
            Phase::Rest => self.settings.rest_minutes,
        }) as f32
            * 60.0
    }
    /// 已連續久坐的分鐘數。
    pub(crate) fn sat_minutes(&self) -> u32 {
        ((self.t - self.sit_start).max(0.0) / 60.0) as u32
    }
    /// 下一次久坐提醒的時刻：從上次確認或起身起算滿 sit_minutes。
    pub(crate) fn sit_due(&self) -> f64 {
        self.sit_start + self.settings.sit_minutes as f64 * 60.0
    }
    /// 「起身了」：久坐計時歸零。
    pub(crate) fn stood_up(&mut self) {
        self.confirm_health_reminder(HealthReminder::Sit);
        self.pet.react(PetState::Happy);
        self.say("久坐計時已重新開始");
    }
    /// 現在是否在午休免打擾時段內（支援跨午夜；起止相同視為未設定）。
    pub(crate) fn lunch_now(&self, env: &Env) -> bool {
        let (a, b) = (self.settings.lunch_start, self.settings.lunch_end);
        let m = env.hour() as u32 * 60 + env.minute();
        self.settings.lunch_rest && a != b && if a < b { m >= a && m < b } else { m >= a || m < b }
    }
    /// 喝水和久坐提醒此刻能否彈出：在提醒時段內，且不在午休。
    pub(crate) fn remind_window(&self, env: &Env) -> bool {
        self.water_hours(env.hour() as u32) && !self.lunch_now(env)
    }
    pub(crate) fn water_hours(&self, hour: u32) -> bool {
        let (a, b) = (self.settings.water_start, self.settings.water_end);
        if a == b {
            true
        } else if a < b {
            hour >= a && hour < b
        } else {
            hour >= a || hour < b
        }
    }
    pub(crate) fn pet_state(&self) -> PetState {
        if let Some((state, _)) = self.pet.reaction {
            state
        } else if self.pet.waking_until > self.t {
            PetState::Waking
        } else if self.pomo.running && self.pomo.phase == Phase::Focus {
            PetState::Focused
        } else if self.pet.worried {
            PetState::Worried
        } else if self.media.playing && self.media.is_music() {
            if self.notice.is_some() || self.hud_notice.is_some() {PetState::Awake}else{PetState::Music}
        } else if self.pet.hovered {
            PetState::Curious
        } else if self.idle_for() >= 180.0 {
            PetState::Sleeping
        } else if self.idle_for() >= self.opts.pet_idle {
            PetState::Dozing
        } else {
            PetState::Awake
        }
    }
    pub fn alert(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        page: usize,
        system: bool,
    ) {
        let title = title.into();
        if let Some(kind)=HealthReminder::ALL.into_iter().find(|kind|title==kind.title()) {
            if self.health_pending[kind as usize]{return;}
            self.health_pending[kind as usize]=true;
            crate::model::save_health_pending(self.health_pending);
        }
        let transient = matches!(
            title.as_str(),
            "系統音量" | "大寫鎖定" | "鍵盤輸入語言" | "網路連線" | "電源已連線" | "已切換電池供電" | "藍芽裝置"
        );
        let a = Alert {
            source: AlertSource::Internal,
            title,
            body: body.into(),
            page,
            at: self.t,
            duration: if transient { 2.5 } else { 8.0 },
            bluetooth: None,
            meeting: None,
            unread: None,
        };
        self.enqueue_alert(a, system);
    }
    pub(crate) fn meeting_alert(&mut self, meeting: crate::mail::Meeting, source: String, page: usize) {
        self.enqueue_alert(Alert {
            source: AlertSource::Internal,
            title: "會議提醒".into(), body: format!("{} · {} · {}", meeting.title, meeting.at, source),
            page, at: self.t, duration: 12.0, bluetooth: None, meeting: Some((meeting, source)), unread: None,
        }, true);
    }
    pub(crate) fn bluetooth_alert(&mut self, device: &crate::bt::Device, connected: bool) {
        if !self.settings.bluetooth_notifications { return; }
        if connected && matches!(device.kind,crate::bt::DeviceKind::Bone|crate::bt::DeviceKind::Earbuds|crate::bt::DeviceKind::Headphones|crate::bt::DeviceKind::Speaker) {
            self.alerts.retain(|a|a.title!="系統音量");
            if self.notice.as_ref().is_some_and(|a|a.title=="系統音量") {self.notice=None;}
        }
        let a = Alert {
            source: AlertSource::Internal,
            title: "藍芽裝置".into(),
            body: if connected { device.summary() } else { format!("{} · 已斷開", device.name) },
            page: 5, at: self.t, duration: 2.5,
            bluetooth: Some((device.kind, connected)),
            meeting: None,
            unread: None,
        };
        self.enqueue_alert(a, false);
    }
    fn enqueue_alert(&mut self, a: Alert, system: bool) {
        // 通知僅儲存在本次執行的記憶體中。
        if a.source==AlertSource::Internal {self.history.push_front(a.clone());}
        self.history.truncate(50);
        if self.fullscreen_hidden && a.health().is_some(){return;}
        if self.settings.notifications {
            if system && a.source==AlertSource::Internal {
                self.pending
                    .push(Cmd::Notify(a.title.clone(), a.body.clone()));
            }
            let same_event=|old:&Alert| match (&a.meeting,&old.meeting) {
                (Some((next,_)),Some((previous,_)))=>crate::calendar::dedupe_key(next)==crate::calendar::dedupe_key(previous),
                _=>old.source==a.source && old.title==a.title && old.unread==a.unread && (a.is_hud() || old.body==a.body),
            };
            if a.source!=AlertSource::Internal || a.title != "藍芽裝置" { self.alerts.retain(|old| !same_event(old)); }
            if (a.source!=AlertSource::Internal || a.title != "藍芽裝置") && self.notice.as_ref().is_some_and(same_event) {
                self.notice = Some(a);
                return;
            }
            self.alerts.push_back(a);
            while self.alerts.len() > 8 {
                self.alerts.pop_front();
            }
        }
    }
    /// 該來源的所有賬號都已檢視。
    pub fn viewed_messenger(&mut self,source:crate::app::Messenger){
        self.pending_messengers.retain(|u|u.source!=source);
        self.clear_messenger_notices(source);
    }
    /// 只清已檢視的賬號；無法確認賬號的 QQ 提醒由使用者明確關閉。
    pub fn viewed_client(&mut self,source:crate::app::Messenger,pid:u32){
        self.pending_messengers.retain(|u|!u.viewed_by(source,pid));
        if pid!=0{self.clear_account_notices(source,pid);}
        if !source.adapter().keep_unidentified{self.clear_account_notices(source,0);}
        if !self.pending_messengers.iter().any(|u|u.source==source){self.clear_messenger_notices(source);}
    }
    pub fn confirm_messenger_open(&mut self,unread:crate::app::Unread,foreground:Option<(crate::app::Messenger,u32)>)->bool{
        if unread.pid==0 || foreground!=Some((unread.source,unread.pid)){return false;}
        self.viewed_client(unread.source,unread.pid);
        true
    }
    pub fn discard_unread(&mut self,unread:crate::app::Unread){
        if !self.pending_messengers.contains(&unread){return;}
        self.pending_messengers.retain(|current|*current!=unread);
        self.clear_account_notices(unread.source,unread.pid);
        if !self.pending_messengers.iter().any(|current|current.source==unread.source){self.clear_messenger_notices(unread.source);}
    }
    fn clear_account_notices(&mut self,source:crate::app::Messenger,pid:u32){
        let matches=|notice:&Alert|notice.unread.is_some_and(|u|u.source==source && u.pid==pid);
        self.alerts.retain(|notice|!matches(notice));
        if self.notice.as_ref().is_some_and(matches){self.notice=None;self.notification_bell_until=0.0;}
    }
    fn clear_messenger_notices(&mut self,source:crate::app::Messenger){
        self.alerts.retain(|a|a.messenger()!=Some(source));
        if self.notice.as_ref().is_some_and(|a|a.messenger()==Some(source)){self.notice=None;self.notification_bell_until=0.0;}
    }
    fn mark_messenger(&mut self,source:crate::app::Messenger,pid:u32,window:isize){
        if let Some(index)=self.pending_messengers.iter().position(|u|u.source==source && u.pid==pid){
            if window!=0 && self.pending_messengers[index].window!=window {
                self.pending_messengers[index].window=window;
                if source.adapter().capture==crate::messenger::Capture::Taskbar{
                    let unread=self.pending_messengers[index];
                    for alert in self.alerts.iter_mut().chain(self.notice.iter_mut()){
                        if alert.unread.is_some_and(|old|old.source==source && old.pid==pid){alert.unread=Some(unread);}
                    }
                }else{self.clear_account_notices(source,pid);}
            }
            return;
        }
        self.pending_messengers.push(crate::app::Unread{source,pid,window});
    }
    /// 橫幅「檢視」：跳到該來源最近一條未讀的視窗。
    pub(crate) fn latest_unread(&self,source:crate::app::Messenger)->crate::app::Unread{
        self.pending_messengers.iter().rev().find(|u|u.source==source).copied().unwrap_or(crate::app::Unread::of(source))
    }
    pub fn receive_wechat_reminder(&mut self){self.receive_wechat_reminder_from(0,0);}
    /// 微信視窗閃動：`pid` 是該賬號的程式，`window` 是閃動的視窗。
    pub fn receive_wechat_reminder_from(&mut self,pid:u32,window:isize){
        self.receive_messenger_reminder(crate::app::Unread{source:crate::app::Messenger::Wechat,pid,window});
    }
    pub fn receive_messenger_reminder(&mut self,unread:crate::app::Unread){
        let client=unread.source;
        let dual_channel=matches!(client,crate::app::Messenger::Discord|crate::app::Messenger::Line);
        if (client.adapter().capture!=crate::messenger::Capture::Taskbar && !dual_channel) || !client.enabled(&self.settings){return;}
        if client.adapter().capture==crate::messenger::Capture::SystemToast && !self.notification_apps.observe(client.adapter().notification_id,client.name()){return;}
        let pending=self.pending_messengers.iter().any(|old|old.source==client && old.pid==unread.pid);
        self.mark_messenger(client,unread.pid,unread.window);
        if pending{return;}
        self.notification_bell_at=self.t;self.notification_bell_until=self.t+4.5;
        self.enqueue_alert(Alert{source:AlertSource::Messenger(client),title:client.name().into(),body:format!("有新訊息 · 請到{}檢視",client.name()),page:usize::MAX,at:self.t,duration:4.5,bluetooth:None,meeting:None,unread:Some(unread)},false);
    }
    pub fn set_messenger_status(&mut self,client:crate::app::Messenger,status:String){self.messenger_statuses.insert(client,status);}
    pub fn messenger_status(&self,client:crate::app::Messenger)->&str{self.messenger_statuses.get(&client).map(String::as_str).unwrap_or("")}
    pub fn receive_notification(&mut self, app:String, id:String, body:String){
        self.receive_notification_from(app,id,body,None);
    }
    pub fn receive_notification_from(&mut self,app:String,id:String,body:String,unread:Option<crate::app::Unread>){
        if !self.settings.read_notifications || id.is_empty() || !crate::messenger::NotificationFilter::for_settings(&self.settings).allows(&id){return;}
        if crate::messenger::suppresses_toast(&app,&id,crate::messenger::toast_suppression(&self.settings)){return;}
        let client=crate::app::Messenger::from_notification_id(&id).filter(|client|client.adapter().capture==crate::messenger::Capture::SystemToast);
        if let Some(c)=client{if !c.enabled(&self.settings){return;}}
        let id=client.map(|client|client.adapter().notification_id.to_string()).unwrap_or(id);
        // 島仔自己發出的系統提醒會被通知中心回傳；已在歷史裡的同一內容不再入隊，避免重複顯示。
        if crate::messenger::Messenger::from_notification_id(&id).is_none() && self.history.iter().any(|a|format!("{} · {}",a.title,a.body)==body){return;}
        if !self.notification_apps.observe(&id,&app) || !self.settings.notifications{return;}
        let unread=client.map(|client|unread.filter(|u|u.source==client && u.pid!=0).unwrap_or(crate::app::Unread::of(client)));
        if let Some(unread)=unread{self.mark_messenger(unread.source,unread.pid,unread.window);}
        if let Some(c)=client{
            // 若工作列閃動已先產生通用提醒，系統通知抵達時直接升級為帶內容的系統通知，不重複彈出兩條橫幅。
            let generic=|a:&Alert| a.source==AlertSource::Messenger(c)
                && unread.as_ref().is_none_or(|u|u.pid==0 || a.unread.is_some_and(|old|old.pid==u.pid || old.pid==0));
            self.alerts.retain(|a|!generic(a));
            if self.notice.as_ref().is_some_and(generic){self.notice=None;}
        }
        self.notification_bell_at=self.t;
        self.notification_bell_until=self.t+4.5;
        let title=app.chars().filter(|c|!c.is_control()).take(80).collect();
        let body=if self.settings.notification_preview {
            let mut text=body.chars().filter(|c|!c.is_control()).take(300).collect::<String>();
            if let Some(code)=crate::otp::extract("",&text,&[]) {text=crate::mail_privacy::mask(&text,&code.text);}
            text
        }else{"收到新通知 · 內容已隱藏".into()};
        self.enqueue_alert(Alert{source:AlertSource::External(id),title,body,page:usize::MAX,at:self.t,duration:4.5,bluetooth:None,meeting:None,unread},false);
    }
    pub fn reveal_messenger_notice(&mut self,unread:crate::app::Unread){
        if !unread.source.adapter().dismiss_unread || !self.pending_messengers.contains(&unread){return;}
        let body=if unread.pid==0{"訊息帳號暫無法確認"}else{"暫時無法開啟對應視窗"};
        let source=if unread.source.adapter().capture==crate::messenger::Capture::Taskbar{AlertSource::Messenger(unread.source)}else{AlertSource::External(unread.source.adapter().notification_id.into())};
        self.notice=Some(Alert{source,title:unread.source.name().into(),body:body.into(),page:usize::MAX,at:self.t,duration:4.5,bluetooth:None,meeting:None,unread:Some(unread)});
        self.notification_bell_at=self.t;self.notification_bell_until=self.t+4.5;
    }
    /// 全螢幕隱藏模式下，當開啟「仍彈出通知」時是否有非健康類通知需要浮現島體。
    pub fn fullscreen_notice_visible(&self)->bool {
        self.settings.fullscreen_show_notices
            && (self.notice.as_ref().is_some_and(|a|a.health().is_none())
                || self.alerts.iter().any(|a|a.health().is_none()))
    }
    /// 結合「全螢幕隱藏」設定與「仍彈出通知」狀態，決定島體視窗實際是否應隱藏。
    pub fn should_hide_for_fullscreen(&self,fullscreen_covered:bool)->bool {
        fullscreen_covered && !self.fullscreen_notice_visible()
    }
    pub(crate) fn tick_alerts(&mut self) {
        self.pending_messengers.retain(|u|u.source.enabled(&self.settings) && (u.source.adapter().capture!=crate::messenger::Capture::SystemToast
            || !self.notification_apps.items.iter().any(|(key,_,on)|key==u.source.adapter().notification_id && !*on)));
        let allowed=|a:&Alert|match &a.source {
            AlertSource::Internal=>!self.fullscreen_hidden || a.health().is_none(),
            AlertSource::Messenger(client)=>client.enabled(&self.settings)
                && !self.notification_apps.items.iter().any(|(key,_,on)|key==client.adapter().notification_id && !*on),
            AlertSource::External(id)=>self.settings.read_notifications && crate::messenger::NotificationFilter::for_settings(&self.settings).allows(id) && self.settings.notifications && !crate::messenger::suppresses_toast(&a.title,id,crate::messenger::toast_suppression(&self.settings)) && self.notification_apps.items.iter().any(|(key,_,on)|key==id && *on),
        };
        self.alerts.retain(&allowed);
        if self.notice.as_ref().is_some_and(|a|!allowed(a)){self.notice=None;}
        if self.hud_notice.as_ref().is_some_and(|a|!allowed(a)){self.hud_notice=None;}
        if self.fullscreen_hidden {
            self.pending.retain(|cmd|!matches!(cmd,Cmd::Notify(title,_)if HealthReminder::ALL.into_iter().any(|kind|title==kind.title())));
        }
        if !self.settings.notification_preview {
            for a in self.alerts.iter_mut().chain(self.notice.iter_mut()).chain(self.hud_notice.iter_mut()) {
                if matches!(&a.source,AlertSource::External(_)){a.body="收到新通知 · 內容已隱藏".into();}
            }
        }
        // 全螢幕隱藏期間島整個不顯示：訊息類通知（Discord / LINE 等）與鈴鐺先不倒數，離開全螢幕後才開始計時，
        // 免得還沒被看到就過期。若已開啟「全螢幕隱藏時仍彈出通知」，則正常計時並在顯示完畢後自動收起。
        if self.fullscreen_hidden && !self.settings.fullscreen_show_notices && self.notice.as_ref().is_some_and(|a|matches!(a.source,AlertSource::Messenger(_)|AlertSource::External(_))) {
            if let Some(a)=self.notice.as_mut(){a.at=self.t;}
            self.notification_bell_at=self.t;self.notification_bell_until=self.t+4.5;
        }
        if self
            .notice
            .as_ref()
            .is_some_and(|a| self.t - a.at > a.duration)
        {
            self.notice = None;
        }
        if self.notice.is_none() {
            if let Some(mut a) = self.alerts.pop_front() {
                a.at = self.t;
                self.notice = Some(a);
            }
        }
    }
    pub(crate) fn draw_notice(&mut self, c:&Card,panel:Rect,a:&Alert,_cmds:&mut Vec<Cmd>){
        let nc=c.clipped(0.0,0.0,panel.width(),panel.height());
        if let Some(source)=a.messenger(){
            messenger_icon(&nc,22.0,22.0,source);
            nc.circle(31.0,14.0,2.6,Color32::from_rgb(255,72,72));
            nc.centered_text(44.0,14.0,&a.title,11.0,W::Semibold,theme::WHITE,H::Left);
            nc.centered_text(44.0,30.0,&nc.fit(&a.body,10.5,W::Regular,154.0),10.5,W::Regular,theme::GRAY,H::Left);
            if notice_button(&nc,206.0,12.0,52.0,"檢視","messenger-view"){_cmds.push(Cmd::ViewMessenger(a.unread.unwrap_or_else(||self.latest_unread(source))));}
            if notice_button(&nc,265.0,12.0,52.0,"關閉","messenger-dismiss"){
                if source.adapter().dismiss_unread{if let Some(unread)=a.unread{self.discard_unread(unread);}}
                self.notice=None;self.notification_bell_until=0.0;
            }
            return;
        }
        if a.source!=AlertSource::Internal {
            crate::paint::bell(&nc,310.0,22.0,theme::GRAY,crate::paint::bell_swing(self.t-a.at));
            nc.circle(314.0,17.5,2.0,Color32::from_rgb(255,72,72));
            nc.centered_text(16.0,14.0,&nc.fit(&a.title,11.0,W::Semibold,198.0),11.0,W::Semibold,theme::WHITE,H::Left);
            nc.centered_text(16.0,30.0,&nc.fit(&a.body,10.5,W::Regular,198.0),10.5,W::Regular,theme::GRAY,H::Left);
            if notice_button(&nc,226.0,12.0,60.0,"關閉","external-dismiss"){
                self.notice=None;
                self.notification_bell_until=0.0;
            }
            return;
        }
        if let Some((meeting,_))=&a.meeting {
            let secondary=Color32::from_gray(174);
            notice_icon(&nc.scaled(34.0,46.0,36.0/28.0),34.0,46.0,a);
            let now=chrono::Local::now();
            let start=chrono::DateTime::from_timestamp(meeting.start,0).unwrap().with_timezone(&chrono::Local);
            let left=meeting.start-now.timestamp();
            let countdown=if left>0 {format!("{} 分鐘後",(left+59)/60)}else{"正在開始".into()};
            let countdown_width=nc.text_w(&countdown,10.5,W::Semibold)+16.0;
            let countdown_x=344.0-countdown_width;
            nc.centered_text(64.0,33.0,&start.format("%H:%M").to_string(),30.0,W::Semibold,theme::WHITE,H::Left);
            nc.fill(countdown_x,23.0,countdown_width,20.0,10.0,Color32::from_gray(28));
            nc.centered_text(countdown_x+countdown_width/2.0,33.0,&countdown,10.5,W::Semibold,theme::WHITE,H::Center);
            let overflow=(nc.text_w(&meeting.title,size::TITLE,W::Semibold)-280.0).max(0.0);
            let offset=(((self.t-a.at-1.2).max(0.0) as f32)*24.0).min(overflow);
            nc.clipped(64.0,52.0,280.0,24.0).centered_text(64.0-offset,64.0,&meeting.title,size::TITLE,W::Semibold,theme::WHITE,H::Left);
            nc.seg((16.0,84.0),(344.0,84.0),0.6,Color32::from_gray(36));
            let date=format!("{}{}",if start.date_naive()==now.date_naive(){"今天 · "}else{""},start.format("%-m月%-d日"));
            nc.centered_text(16.0,110.0,&date,11.0,W::Regular,secondary,H::Left);
            let (dismiss,dc)=crate::paint::capsule(&nc,188.0,98.0,72.0,24.0,"meeting-dismiss",Color32::from_gray(32),Color32::from_gray(48));
            dc.centered_text(224.0,110.0,"關閉",10.5,W::Semibold,theme::WHITE,H::Center);
            if dismiss.on_hover_text("關閉本次提醒，日程仍保留").clicked(){
                self.notice=None;
                return;
            }
            let (button,bc)=crate::paint::capsule(&nc,272.0,98.0,72.0,24.0,"meeting-action",Color32::from_gray(42),Color32::from_gray(56));
            bc.centered_text(308.0,110.0,"檢視",10.5,W::Semibold,theme::WHITE,H::Center);
            if button.clicked() {
                self.notice=None;
                self.open_calendar(Some(meeting.clone()));
            }
            return;
        }
        if a.is_hud(){
            if a.title=="藍芽裝置" {
                let (name,status,battery)=bluetooth_notice_content(&a.body);
                nc.circle(26.0,24.0,14.0,if status=="已連線"{Color32::from_rgb(10,132,255)}else{Color32::from_gray(95)});
                let kind=a.bluetooth.map(|(kind,_)|kind).unwrap_or(crate::bt::DeviceKind::Other);
                device_icon(&nc,kind,17.0,15.0,18.0);
                let right=HUD_W-14.0;
                let name_end=if let Some(level)=battery {
                    let pct=format!("{level}%");
                    let pct_width=nc.text_w(&pct,10.5,W::Semibold);
                    let x=right-pct_width-22.0;
                    nc.stroke(x,20.0,15.0,8.0,2.0,1.0,Color32::from_gray(175));
                    nc.fill(x+15.5,22.5,2.0,3.0,0.8,Color32::from_gray(175));
                    let fill=11.0*level as f32/100.0;
                    if fill>0.0 {nc.fill(x+2.0,22.0,fill,4.0,1.0,if level<=20{theme::RED}else{theme::GREEN});}
                    nc.centered_text(right,24.0,&pct,10.5,W::Semibold,theme::WHITE,H::Right);
                    x-8.0
                } else {right};
                nc.centered_text(49.0,16.0,&nc.fit(name,11.0,W::Semibold,(name_end-49.0).max(20.0)),11.0,W::Semibold,theme::WHITE,H::Left);
                nc.centered_text(49.0,32.0,status,10.5,W::Regular,theme::GRAY,H::Left);
                return;
            }
            match a.title.as_str(){
                "系統音量"=>{
                    nc.poly(&[(17.0,16.0),(21.0,16.0),(26.0,12.0),(26.0,26.0),(21.0,22.0),(17.0,22.0)],theme::WHITE);
                    if self.volume_ui.is_some_and(|(_,muted)|muted){nc.seg((29.0,16.0),(34.0,22.0),1.4,theme::WHITE);nc.seg((34.0,16.0),(29.0,22.0),1.4,theme::WHITE);}
                    else{nc.arc(26.0,19.0,5.0,-0.8,0.8,1.5,theme::WHITE);nc.arc(26.0,19.0,8.0,-0.7,0.7,1.3,theme::WHITE);}
                },
                _=>notice_icon(&nc,24.0,19.0,a),
            }
            if a.title=="系統音量" {
                self.draw_volume_hud(&nc);
            } else {
                let (title,body)=if a.title=="鍵盤輸入語言"{("輸入語言",a.body.as_str())}
                    else if a.title=="已切換電池供電"{("電池供電",a.body.as_str())}
                    else if a.title=="網路連線"{a.body.split_once(" · ").unwrap_or(("網路",a.body.as_str()))}
                    else{(a.title.as_str(),a.body.as_str())};
                nc.centered_text(48.0,19.0,&nc.fit(title,11.0,W::Semibold,90.0),11.0,W::Semibold,theme::WHITE,H::Left);
                nc.centered_text(HUD_W-16.0,19.0,&nc.fit(body,10.5,W::Regular,92.0),10.5,W::Regular,if a.title=="大寫鎖定"&&a.body=="已開啟"||a.title=="網路連線"&&a.body.contains("已連線"){theme::GREEN}else{theme::GRAY},H::Right);
            }
        } else {
            let is_sit=a.health()==Some(HealthReminder::Sit);let is_water=a.health()==Some(HealthReminder::Water);let is_focus=a.page==1;let is_power=a.title=="電量偏低";
            notice_icon(&nc,22.0,22.0,a);
            let max=if is_water||is_sit{125.0}else{195.0};
            nc.centered_text(43.0,14.0,&nc.fit(&a.title,11.0,W::Semibold,max),11.0,W::Semibold,theme::WHITE,H::Left);
            nc.centered_text(43.0,30.0,&nc.fit(&a.body,10.5,W::Regular,max),10.5,W::Regular,theme::GRAY,H::Left);
            if is_sit {
                if notice_button(&nc,181.0,12.0,62.0,"知道了","sit-confirm-read"){self.confirm_health_reminder(HealthReminder::Sit);}
                if notice_button(&nc,250.0,12.0,68.0,"起身了","sit-confirm"){self.notice=None;self.stood_up();}
            }else if is_water {
                if notice_button(&nc,181.0,12.0,62.0,"知道了","water-confirm-read"){self.confirm_health_reminder(HealthReminder::Water);}
                if notice_button(&nc,250.0,12.0,68.0,"喝一口","water-confirm"){self.notice=None;self.drink_water();}
            }else{
                let label=if is_focus{"開始"}else if is_power{"知道了"}else{"檢視"};
                if notice_button(&nc,257.0,12.0,60.0,label,"notice-action"){
                    self.notice=None;if is_power{return;}if is_focus{self.pomo.running=true;}else if a.page<6{
                        self.show_page(a.page);
                        if a.page==4 {
                            let order=self.todos.display_order(&self.today);
                            if let Some(index)=order.iter().position(|&i|self.todos.items[i].text==a.body && !self.todos.items[i].done_for(&self.today)){
                                self.todo_first=index.min(order.len().saturating_sub(3));
                            }
                        }
                    }else{self.settings_tab=6;self.open_settings();}
                }
            }
            if let Some(kind)=a.health() {
                if nc.hit(0.0,0.0,175.0,44.0,"health-notice-confirm").clicked(){self.confirm_health_reminder(kind);}
            }
        }
    }
    /// v0.5.4：可互動音量條。樣式參考 oyenza-studio（MIT）的 HUD 音量條：
    /// 圓角膠囊、填充緩動、拖動時加粗並出現手柄；拖動/滾輪改音量，點喇叭靜音。
    pub(crate) fn draw_volume_hud(&mut self, nc: &Card) {
        let (v, muted) = self.volume_ui.unwrap_or((0, false));
        let v = v.min(100);
        // 喇叭：點選靜音 / 恢復
        let icon = nc.hit(12.0, 9.0, 28.0, 22.0, "volume-mute");
        if icon.clicked() && nc.live {
            self.volume_ui = Some((v, !muted));
            self.volume_quiet_until = self.t + 0.9;
            self.pending.push(Cmd::ToggleMute);
        }
        // 滑條：拖動 / 點選定位；滾輪微調
        let track = nc.hit_drag(HUD_TRACK_X, 8.0, HUD_TRACK_W, 24.0, "volume-track");
        let mut want: Option<u32> = None;
        if track.dragged() || track.clicked() {
            if let Some(p) = track.interact_pointer_pos() {
                let x = nc.to_local(p).x;
                want = Some((((x - HUD_TRACK_X) / HUD_TRACK_W) * 100.0).clamp(0.0, 100.0).round() as u32);
            }
        } else if nc.live && track.hovered() {
            let dy = nc.ui.ctx().input(|i| i.raw_scroll_delta.y);
            if dy.abs() > 0.5 {
                want = Some(((v as f32 + (dy / 120.0) * 4.0).clamp(0.0, 100.0)).round() as u32);
            }
        }
        if let Some(nv) = want {
            if self.volume_ui.map(|(o, _)| o) != Some(nv) || muted {
                self.volume_ui = Some((nv, false));
                self.volume_quiet_until = self.t + 0.9;
                self.pending.push(Cmd::SetVolume(nv));
            }
        }
        let (v, muted) = self.volume_ui.unwrap_or((v, muted));
        let pressed = track.dragged() || track.is_pointer_button_down_on() || icon.is_pointer_button_down_on();
        let target = if muted { 0.0 } else { v as f32 / 100.0 };
        self.volume_a = crate::anim::approach(self.volume_a, target, if pressed { 34.0 } else { 13.0 }, self.dt);
        let h = if pressed { 7.0 } else { 5.0 };
        let y = 19.0 - h / 2.0;
        nc.fill(HUD_TRACK_X, y, HUD_TRACK_W, h, h / 2.0, Color32::from_white_alpha(if pressed { 46 } else { 34 }));
        if self.volume_a > 0.0 {
            let w = (HUD_TRACK_W * self.volume_a).max(h);
            let col = if muted { Color32::from_gray(150) } else { theme::WHITE };
            nc.fill(HUD_TRACK_X, y, w, h, h / 2.0, col);
            nc.fill(HUD_TRACK_X, y + 0.8, (w - 1.2).max(0.0), h * 0.4, h * 0.2, Color32::from_white_alpha(64));
            if pressed {
                nc.circle(HUD_TRACK_X + w - h / 2.0, y + h / 2.0, h * 0.95, col);
            }
        }
        let label = if muted { "靜音".to_string() } else { format!("{v}%") };
        nc.centered_text(HUD_W - 26.0, 19.0, &label, 10.5, if pressed { W::Semibold } else { W::Regular }, if muted { theme::GRAY } else { theme::WHITE }, H::Center);
    }

    pub(crate) fn drink_water(&mut self) {        let hour = self.current_hour;
        let amount = self.settings.serving_ml;
        let was = self.water.percent();
        let before = self.water.drunk_ml;
        self.water.drink(amount, hour);
        let amount = self.water.drunk_ml.saturating_sub(before);
        self.last_drink = Some((amount, hour));
        self.confirm_health_reminder(HealthReminder::Water);
        self.pet.react(PetState::Satisfied);
        self.say(format!("已記錄 {amount} ml"));
        if was < 100.0 && self.water.percent() >= 100.0 {
            self.alert("今日飲水目標達成", "做得不錯，繼續保持", 2, true);
        }
    }


}

/// 將本地提醒時間規範化，拒絕不存在的日期與超出範圍的時分。
pub(crate) fn parse_due(value: &str, today: &str) -> Option<String> {
    let value = value.trim();
    let full = if value.len() == 5 {
        format!("{today} {value}")
    } else {
        value.to_string()
    };
    if !full.is_ascii() || full.len() != 16 {
        return None;
    }
    let b = full.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' {
        return None;
    }
    let year = full[0..4].parse::<u32>().ok()?;
    let month = full[5..7].parse::<usize>().ok()?;
    let day = full[8..10].parse::<u32>().ok()?;
    let hour = full[11..13].parse::<u32>().ok()?;
    let minute = full[14..16].parse::<u32>().ok()?;
    if !(2000..=9999).contains(&year) || !(1..=12).contains(&month) || hour > 23 || minute > 59 {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || day > days[month - 1] {
        return None;
    }
    Some(full)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn island_size_drag_commits_only_on_release(){
        let ctx=egui::Context::default();let mut value=100;
        let pos=egui::pos2(30.0,30.0);let end=egui::pos2(110.0,30.0);
        let button=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()};
        let frames=[vec![],vec![egui::Event::PointerMoved(pos)],vec![button(pos,true)],vec![egui::Event::PointerMoved(end)],vec![button(end,false)]];
        for (i,events) in frames.into_iter().enumerate(){
            let _=ctx.run(egui::RawInput{events,screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(320.0,80.0))),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(20)).show(ctx,|ui|{ui.spacing_mut().slider_width=180.0;island_size_slider(ui,&mut value);});
            });
            if i<4{assert_eq!(value,100);}else{assert_ne!(value,100);}
        }
    }
    #[test]
    fn number_stepper_clicks_follow_steps_and_limits() {
        for (initial,step,x,expected) in [(5u32,1,127.0,6),(5,1,13.0,4),(0,1,13.0,0),(10,1,127.0,10),(9,3,127.0,10),(1,3,13.0,0)] {
            let ctx=egui::Context::default();
            let mut value=initial;
            let pos=egui::pos2(x+20.0,34.0);
            let events=[vec![],vec![egui::Event::PointerMoved(pos)],vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}],vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]];
            for events in events {
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(200.0,80.0))),events,..Default::default()};
                let _=ctx.run(raw,|ctx|{
                    egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(20)).show(ctx,|ui|{
                        ui.horizontal(|ui|{number_stepper(ui,&mut value,0,10,"",step,76.0);});
                    });
                });
            }
            assert_eq!(value,expected,"初值 {initial}，步長 {step}，點選位置 {x}");
        }
    }

    #[test]
    fn number_stepper_fits_with_settings_button_padding() {
        let ctx=egui::Context::default();
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(340.0,240.0))),..Default::default()};
        let _=ctx.run(raw,|ctx|{
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                ui.spacing_mut().button_padding=egui::vec2(10.0,6.0);
                for mut value in [0u32,25,180] {
                    let row=ui.horizontal(|ui|{number_stepper(ui,&mut value,0,180," 分鐘",1,76.0);});
                    assert!(row.response.rect.width()<=140.1,"數值控制元件超出預留寬度：{}",row.response.rect.width());
                }
                for mut value in [25u32,5,15] {setting_number(ui,"計時時長",&mut value,1,180," 分鐘",1.0);}
                assert!(ui.min_rect().right()<=340.1,"連續數值行不應撐寬卡片");
            });
        });
    }

    #[test]
    fn settings_groups_keep_every_page_reachable() {
        let groups = ["常規", "音樂與歌詞", "專注與飲水", "專注與飲水", "待辦與置物", "待辦與置物", "通知、信箱與日程", "通知、信箱與日程", "通知、信箱與日程", "關於", "頁面管理", "AI Usage", "天氣"];
        for (tab, expected) in groups.iter().enumerate() {
            let matches: Vec<_> = SETTINGS_GROUPS.iter().filter(|(_,start,end)| (*start..=*end).contains(&tab)).collect();
            assert_eq!(matches.len(), 1, "頁面 {tab} 必須且只能屬於一個分組");
            assert_eq!(matches[0].0, *expected);
        }
    }

    #[test]
    fn bluetooth_notice_uses_real_connection_and_battery_data(){
        assert_eq!(bluetooth_notice_content("OpenMove by Shokz · 電量 80%"),("OpenMove by Shokz","已連線",Some(80)));
        assert_eq!(bluetooth_notice_content("耳機"),("耳機","已連線",None));
        assert_eq!(bluetooth_notice_content("已斷開連線"),("藍芽裝置","已斷開",None));
        assert_eq!(bluetooth_notice_content("青龍鍵盤 · 已斷開"),("青龍鍵盤","已斷開",None));
        assert_eq!(bluetooth_notice_content("耳機 · 電量 101%"),("耳機 · 電量 101%","已連線",None));
    }

    #[test]
    fn device_icons_render_as_visible_svg() {
        let icons: &[&[u8]] = &[
            include_bytes!("../assets/device-icons/headphones-round-bold.svg"),
            include_bytes!("../assets/device-icons/speaker-bold.svg"),
            include_bytes!("../assets/device-icons/keyboard-bold.svg"),
            include_bytes!("../assets/device-icons/mouse-bold.svg"),
            include_bytes!("../assets/device-icons/gamepad-bold.svg"),
            include_bytes!("../assets/device-icons/smartphone-bold.svg"),
            include_bytes!("../assets/device-icons/bluetooth-linear.svg"),
        ];
        for bytes in icons {
            let image=egui_extras::image::load_svg_bytes(bytes).unwrap();
            assert!(image.pixels.iter().any(|p|p.a()>0));
        }
    }

    #[test]
    fn due_time_checks_calendar_and_format() {
        assert_eq!(
            parse_due("09:30", "2026-10-29").as_deref(),
            Some("2026-10-29 09:30")
        );
        assert!(parse_due("2026-02-29 12:00", "").is_none());
        assert!(parse_due("2028-02-29 12:00", "").is_some());
        for value in [
            "24:00",
            "12:60",
            "明天中午",
            "2026-13-01 10:00",
            "2026-00-01 10:00",
        ] {
            assert!(parse_due(value, "2026-10-29").is_none());
        }
    }
}

pub(crate) struct TaskDraft {index:usize,original:String,text:String,due:String,error:String,focused:bool,reminder_only:bool,pub(crate) repeat:u8,pub(crate) picker:crate::task_picker::ReminderPicker}
impl App {
    pub(crate) fn open_task_dialog(&mut self,index:usize){
        self.todo_choosing=false;
        if let Some(task)=self.todos.items.get(index){
            let draft=TaskDraft{index,original:task.text.clone(),text:task.text.clone(),due:task.due.clone().unwrap_or_default(),error:String::new(),focused:false,reminder_only:false,repeat:task.repeat,picker:crate::task_picker::ReminderPicker::new(task.due.as_deref())};
            self.show_page(4);self.task_dialog=Some(draft);
        }
    }
    pub(crate) fn open_task_reminder(&mut self,index:usize){
        self.open_task_dialog(index);
        if let Some(d)=self.task_dialog.as_mut(){d.reminder_only=true;d.picker.show();}
    }
    pub(crate) fn draw_task_editor(&mut self,c:&Card){
        if self.task_dialog.as_ref().is_some_and(|draft|draft.picker.open){self.draw_task_reminder(c);return;}
        let reminder_only=self.task_dialog.as_ref().is_some_and(|d|d.reminder_only);
        c.text(15.0,25.0,if reminder_only{"設定提醒"}else{"編輯待辦"},size::TITLE,W::Bold,theme::WHITE,H::Left);
        c.text(43.0,48.0,if reminder_only{"待辦"}else{"內容"},10.5,W::Regular,theme::GRAY,H::Left);
        c.fill(91.0,33.0,275.0,20.0,6.0,theme::CARD);
        if let Some(d)=self.task_dialog.as_ref().filter(|d|d.reminder_only){
            c.text(97.0,47.0,&c.fit(&d.text,11.5,W::Regular,260.0),11.5,W::Regular,theme::WHITE,H::Left);
        }
        c.text(43.0,72.0,"提醒",10.5,W::Regular,theme::GRAY,H::Left);
        let time=self.task_dialog.as_ref().map(|d|if d.due.is_empty(){"點選選擇時間".into()}
            else if d.repeat!=0{format!("{} {}",crate::model::repeat_label(d.repeat),d.due.get(11..16).unwrap_or(&d.due))}else{d.due.clone()}).unwrap_or_default();
        if setting_button(c,91.0,58.0,275.0,&time,false,"task-select-reminder"){if let Some(d)=self.task_dialog.as_mut(){d.picker.show();}}
        // 每週重複：七個星期按鈕，全不選即一次性待辦。
        c.text(43.0,95.0,"重複",10.5,W::Regular,theme::GRAY,H::Left);
        self.task_repeat_chips(c,91.0,81.0,25.0,19.0,28.0);
        let repeat=self.task_dialog.as_ref().map(|d|d.repeat).unwrap_or(0);
        let quick=if repeat==crate::model::REPEAT_ALL{"不重複"}else{"每天"};
        if setting_button(c,289.0,81.0,77.0,quick,false,"task-repeat-all"){
            if let Some(d)=self.task_dialog.as_mut(){d.repeat=if d.repeat==crate::model::REPEAT_ALL{0}else{crate::model::REPEAT_ALL};}
        }
        if let Some(d)=&self.task_dialog {
            let summary=if d.repeat!=0{crate::model::repeat_label(d.repeat)}else{"只做一次".into()};
            let hint=if d.error.is_empty(){summary.as_str()}else{&d.error};
            c.text(43.0,117.0,&c.fit(hint,10.5,W::Regular,if reminder_only{140.0}else{154.0}),10.5,if d.error.is_empty(){W::Regular}else{W::Semibold},if d.error.is_empty(){theme::GRAY}else{theme::RED},H::Left);
        }
        if reminder_only {
            let has_reminder=self.task_dialog.as_ref().and_then(|d|self.todos.items.get(d.index)).is_some_and(|task|task.due.is_some());
            let skip=if has_reminder{"取消提醒"}else if repeat!=0{"只設重複"}else{"暫不提醒"};
            if setting_button(c,188.0,104.0,88.0,skip,false,"task-reminder-skip"){
                if let Some(d)=self.task_dialog.as_mut(){d.due.clear();}
                self.save_task_editor();
            }
        }else if setting_button(c,202.0,104.0,74.0,"取消",false,"task-cancel"){self.task_dialog=None;}
        if setting_button(c,286.0,104.0,80.0,if reminder_only{"選擇時間"}else{"儲存"},true,"task-save"){
            if reminder_only{if let Some(d)=self.task_dialog.as_mut(){d.picker.show();}}else{self.save_task_editor();}
        }
    }
    pub(crate) fn task_editor_inputs(&mut self,ui:&mut egui::Ui,origin:egui::Pos2,cmds:&mut Vec<Cmd>){
        let Some(d)=self.task_dialog.as_mut() else{return};
        if d.reminder_only || d.picker.open{return;}
        let name=ui.put(Rect::from_min_size(origin+egui::vec2(97.0,35.0),egui::vec2(260.0,17.0)),egui::TextEdit::singleline(&mut d.text).vertical_align(egui::Align::Center).id(egui::Id::new("task-name-edit")).frame(false).font(theme::font(11.5,W::Regular)));
        let first_focus=!d.focused;
        if first_focus{name.request_focus();d.focused=true;cmds.push(Cmd::Focus);}
        // 新建內容與下一步共用這一幀，避免同一次回車又提交了空提醒。
        if !first_focus && ui.input(|i|i.key_pressed(egui::Key::Enter)){self.save_task_editor();}
    }
    /// 七個星期按鈕（一…日），點一下切換；全不選即一次性待辦。
    fn task_repeat_chips(&mut self,c:&Card,x:f32,y:f32,w:f32,h:f32,step:f32){
        let repeat=self.task_dialog.as_ref().map(|d|d.repeat).unwrap_or(0);
        for (day,name) in crate::model::WEEKDAY_NAMES.iter().enumerate() {
            let on=repeat&(1<<day)!=0;
            let cx=x+day as f32*step;
            let (r,b)=crate::paint::capsule(c,cx,y,w,h,&format!("task-repeat-{day}"),if on{Color32::from_rgb(39,94,150)}else{theme::BTN},theme::BTN_HOVER);
            b.centered_text(cx+w/2.0,y+h/2.0,name,if h<18.0{9.5}else{10.5},W::Semibold,if on{theme::WHITE}else{theme::GRAY},H::Center);
            if r.on_hover_text(format!("每週{name}")).clicked(){
                if let Some(d)=self.task_dialog.as_mut(){d.repeat^=1<<day;d.error.clear();}
            }
        }
    }
    fn draw_task_reminder(&mut self,c:&Card){
        use crate::task_picker::PickerAction;
        let Some(d)=self.task_dialog.as_mut().filter(|d|d.picker.open) else{return};
        let has_reminder=self.todos.items.get(d.index).is_some_and(|task|task.due.is_some());
        c.text(15.0,25.0,"待辦",size::TITLE,W::Bold,theme::WHITE,H::Left);
        c.text(44.0,25.0,"清單",size::TITLE,W::Bold,theme::GREEN,H::Left);
        let (done,total)=self.todos.progress(&self.today);
        c.text(42.0,61.0,&format!("{done}/{total}"),29.0,W::Bold,theme::WHITE,H::Left);
        // 左側：每週重複。選了星期幾後，提醒只按時間、在這些日子到點提醒。
        d.picker.repeat_note=Some(if d.repeat!=0{format!("{} · 到點提醒",crate::model::repeat_label(d.repeat))}else{String::new()});
        let action=d.picker.draw(c,&d.text,has_reminder);
        let focus=std::mem::take(&mut d.picker.focus);
        self.task_repeat_chips(c,41.0,84.0,17.0,16.0,19.5);
        let Some(d)=self.task_dialog.as_mut() else{return};
        if focus{self.pending.push(Cmd::Focus);}
        match action{
            Some(PickerAction::Save)=>{d.due=d.picker.due();d.picker.open=false;if d.reminder_only{self.save_task_editor();}},
            Some(PickerAction::Skip)=>{d.due.clear();d.picker.open=false;if d.reminder_only{self.save_task_editor();}},
            Some(PickerAction::Cancel)=>{d.picker.open=false;if d.reminder_only{self.task_dialog=None;}},
            None=>{},
        }
    }
    fn save_task_editor(&mut self){
        let Some(mut d)=self.task_dialog.take() else{return};
        let due=if d.due.trim().is_empty(){Some(None)}else{parse_due(&d.due,&self.today).map(Some)};
        if d.text.trim().is_empty(){d.error="請填寫任務內容".into();}
        else if due.is_none(){d.error="時間格式應為 18:30 或完整日期時間".into();}
        else if self.todos.items.get(d.index).is_none_or(|t|t.text!=d.original){d.error="任務已變化，請取消後重新開啟".into();}
        else{
            let item=&mut self.todos.items[d.index];
            if item.repeat!=d.repeat{item.done_on=None;}
            item.text=d.text.trim().into();item.due=due.unwrap();item.reminded=false;item.reminded_on=None;item.repeat=d.repeat;
            self.todos.save();self.say("待辦已儲存");return;
        }
        self.task_dialog=Some(d);
    }
}

fn notice_button(c:&Card,x:f32,y:f32,w:f32,text:&str,id:&str)->bool {
    let (r,b)=crate::paint::capsule(c,x,y,w,20.0,id,Color32::from_gray(35),theme::BTN_HOVER);
    b.centered_text(x+w/2.0,y+10.0,text,10.5,W::Semibold,theme::WHITE,H::Center);
    r.clicked()
}
fn setting_button(c:&Card,x:f32,y:f32,w:f32,text:&str,on:bool,id:&str)->bool {
    let (r,b)=crate::paint::capsule(c,x,y,w,19.0,id,if on{Color32::from_rgb(39,94,150)}else{theme::BTN},theme::BTN_HOVER);
    b.text(x+w/2.0,y+13.3,text,10.5,W::Semibold,theme::WHITE,H::Center);
    r.clicked()
}
fn hhmm(m:u32)->String{format!("{:02}:{:02}",m/60%24,m%60)}
/// 島內小開關（與健康頁右上開關同款）。
fn island_switch(c:&Card,x:f32,y:f32,on:bool,id:&str)->bool{
    let r=c.hit(x-2.0,y-2.0,38.0,24.0,id);
    let (hv,_)=c.hover_t(&r);
    let track=if on{theme::SIT_GREEN}else{Color32::from_rgb(58,58,62)};
    c.fill(x,y,34.0,20.0,10.0,crate::paint::mix(track,theme::WHITE,hv*0.08));
    c.circle(if on{x+24.0}else{x+10.0},y+10.0,8.0,theme::WHITE);
    r.clicked()
}
impl App {
    fn health_time_summary(&self)->String{
        let s=&self.settings;
        let hours=format!("{:02}:00–{:02}:00",s.water_start,s.water_end);
        if s.lunch_rest{format!("{hours} · 午休 {}–{}",hhmm(s.lunch_start),hhmm(s.lunch_end))}else{hours}
    }
    /// 「提醒時段」二級頁：提醒時段起止、午休開關、午休起止（15 分鐘一檔），喝水和久坐共用。
    fn health_time_page(&mut self,c:&Card){
        let row_y=|i:usize|38.0+i as f32*22.0;
        for i in 1..4{c.fill(44.0,row_y(i)-2.0,326.0,1.0,0.5,Color32::from_rgb(28,28,31));}
        let y=row_y(0);
        c.text(44.0,y+13.0,"提醒時段",10.5,W::Regular,theme::GRAY,H::Left);
        c.text(250.0,y+13.0,"至",10.5,W::Regular,theme::GRAY,H::Center);
        for i in 0..2{
            let hour=if i==0{&mut self.settings.water_start}else{&mut self.settings.water_end};
            let value=egui::DragValue::new(hour).range(0..=if i==0{23}else{24})
                .custom_formatter(|v,_|format!("{:02}:00",v as u32))
                .custom_parser(|t|{let t=t.trim();t.strip_suffix(":00").or_else(||t.strip_suffix("：00")).unwrap_or(t).parse::<u32>().ok().map(|v|v as f64)});
            let (d,field)=stepper(c,if i==0{238.0}else{370.0},y,value,&format!("hour-{i}"));
            if field.gained_focus(){self.pending.push(Cmd::Focus);}self.settings_editing|=field.has_focus();
            if d!=0{let hour=if i==0{&mut self.settings.water_start}else{&mut self.settings.water_end};*hour=(*hour as i32+d).rem_euclid(24) as u32;}
        }
        let y=row_y(1);
        c.text(44.0,y+13.0,"午休不提醒",10.5,W::Regular,theme::GRAY,H::Left);
        if island_switch(c,336.0,y-0.5,self.settings.lunch_rest,"lunch-rest"){self.settings.lunch_rest=!self.settings.lunch_rest;}
        let y=row_y(2);
        let on=self.settings.lunch_rest;
        let lc=c.faded(if on{1.0}else{0.35});let lc=if on{lc}else{lc.ghost()};
        lc.text(44.0,y+13.0,"午休時間",10.5,W::Regular,theme::GRAY,H::Left);
        lc.text(250.0,y+13.0,"至",10.5,W::Regular,theme::GRAY,H::Center);
        for i in 0..2{
            let minute=if i==0{&mut self.settings.lunch_start}else{&mut self.settings.lunch_end};
            let value=egui::DragValue::new(minute).range(0..=1439).speed(1.0).custom_formatter(|v,_|hhmm(v as u32))
                .custom_parser(|t|{let (h,m)=t.trim().split_once([':','：'])?;Some((h.parse::<u32>().ok()?.min(23)*60+m.parse::<u32>().ok()?.min(59)) as f64)});
            let (d,field)=stepper(&lc,if i==0{238.0}else{370.0},y,value,&format!("lunch-{i}"));
            if field.gained_focus(){self.pending.push(Cmd::Focus);}self.settings_editing|=field.has_focus();
            if d!=0{let minute=if i==0{&mut self.settings.lunch_start}else{&mut self.settings.lunch_end};*minute=(*minute as i32+d*15).rem_euclid(1440) as u32;}
        }
        c.text(44.0,row_y(3)+13.0,"喝水和久坐提醒都只在提醒時段內、午休以外彈出",10.5,W::Regular,theme::GRAY,H::Left);
    }
}
fn island_number_field(c:&Card,rect:Rect,value:egui::DragValue<'_>,id:impl std::hash::Hash)->egui::Response {
    let mut ui=egui::Ui::new(c.ui.ctx().clone(),egui::Id::new(id),egui::UiBuilder::new().layer_id(c.ui.layer_id()).max_rect(rect).style(c.ui.style().clone()));
    ui.set_clip_rect(c.painter.clip_rect());ui.set_opacity(c.painter.opacity());if !c.live{ui.disable();}
    ui.spacing_mut().interact_size=rect.size();ui.spacing_mut().button_padding=egui::vec2(2.0,1.0);
    ui.style_mut().drag_value_text_style=egui::TextStyle::Body;
    ui.style_mut().text_styles.insert(egui::TextStyle::Body,c.font(11.0,W::Bold));
    ui.style_mut().explanation_tooltips=false;
    ui.visuals_mut().widgets.inactive.weak_bg_fill=Color32::TRANSPARENT;
    crate::quota_ui::bar_tooltip(ui.put(rect,value.update_while_editing(false)),"點選輸入數值".into())
}
/// 加減器：[−] 可輸入數值 [+]，右緣對齊到 right。
fn stepper(c:&Card,right:f32,y:f32,value:egui::DragValue<'_>,id:&str)->(i32,egui::Response) {
    let (w,vw)=(25.0,58.0);
    let x=right-w*2.0-vw;
    let field=island_number_field(c,c.rect(x+w,y,vw,19.0),value,("health-number",id));
    let delta=if setting_button(c,x,y,w,"−",false,&format!("{id}-minus")){-1}
        else if setting_button(c,right-w,y,w,"+",false,&format!("{id}-plus")){1}else{0};
    (delta,field)
}
impl App {
    pub(crate) fn page_settings(&mut self,c:&Card) {
        let help=match self.settings_category{0=>"外觀與系統",1=>"調整喝水、久坐與提醒時段",2=>"點選分鐘數可直接輸入，也可用加減微調",_=>"通知"};
        if self.settings_category!=1{c.text(44.0,44.0,help,10.5,W::Regular,theme::GRAY,H::Left);}
        let before=format!("{:?}",self.settings);
        self.settings_editing=false;
        match self.settings_category {
            0=>{
                let choices=[("吉祥物",self.settings.pet),("極簡劉海",self.settings.minimal),("系統提示",self.settings.system_hud),("全屏隱藏",self.settings.hide_fullscreen)];
                for (i,(name,on)) in choices.iter().enumerate(){let x=42.0+(i%2) as f32*169.0;let y=57.0+(i/2) as f32*27.0;let text=format!("{}  {}",name,if *on{"開"}else{"關"});if setting_button(c,x,y,158.0,&text,*on,&format!("general-{i}")){match i{0=>self.set_pet(!self.settings.pet),1=>self.set_minimal(!self.settings.minimal),2=>self.settings.system_hud=!self.settings.system_hud,_=>self.settings.hide_fullscreen=!self.settings.hide_fullscreen}}}
                c.text(42.0,116.0,"島仔收起時常駐；極簡模式保留細線",10.5,W::Regular,theme::GRAY,H::Left);
            }
            1 if self.health_time=>self.health_time_page(c),
            1=>{
                // 與健康頁同一套切換（標題旁的水滴 / 椅子）；四行一列，標籤靠左、加減器靠右對齊。
                // 喝水三行、久坐一行加兩句說明；最後一行「提醒時段」兩邊共用，位置不變。
                let sv=self.sit_view_t.pos.clamp(0.0,1.0);
                let row_y=|i:usize|38.0+i as f32*22.0;
                for i in 1..4{c.fill(44.0,row_y(i)-2.0,326.0,1.0,0.5,Color32::from_rgb(28,28,31));}
                if sv<0.99{
                    let wc=c.faded(1.0-sv);let wc=if self.sit_mode{wc.ghost()}else{wc};
                    for (i,label) in ["每日目標","每次記錄","提醒間隔"].iter().enumerate(){
                        wc.text(44.0,row_y(i)+13.0,label,10.5,W::Regular,theme::GRAY,H::Left);
                        let goal=self.water.goal_ml;let mut liters=goal as f32/1000.0;
                        let value=match i {
                            0=>egui::DragValue::new(&mut liters).range(0.5..=6.0).speed(0.25).suffix(" L").min_decimals(1).max_decimals(2),
                            1=>egui::DragValue::new(&mut self.settings.serving_ml).range(50..=1000).speed(50.0).suffix(" ml"),
                            _=>egui::DragValue::new(&mut self.settings.water_minutes).range(5..=180).speed(5.0).suffix(" 分鐘"),
                        };
                        let (d,field)=stepper(&wc,370.0,row_y(i),value,&format!("water-{i}"));
                        if field.gained_focus(){self.pending.push(Cmd::Focus);}self.settings_editing|=field.has_focus();
                        match i {
                            0=>{self.water.goal_ml=((liters*1000.0).round() as i32+d*250).clamp(500,6000) as u32;if self.water.goal_ml!=goal{self.water.save();}}
                            1=>self.settings.serving_ml=(self.settings.serving_ml as i32+d*50).clamp(50,1000) as u32,
                            _=>self.settings.water_minutes=(self.settings.water_minutes as i32+d*5).clamp(5,180) as u32,
                        }
                    }
                }
                if sv>0.01{
                    let sc=c.faded(sv);let sc=if self.sit_mode{sc}else{sc.ghost()};
                    sc.text(44.0,row_y(0)+13.0,"坐滿多久提醒",10.5,W::Regular,theme::GRAY,H::Left);
                    let value=egui::DragValue::new(&mut self.settings.sit_minutes).range(15..=180).speed(5.0).suffix(" 分鐘");
                    let (d,field)=stepper(&sc,370.0,row_y(0),value,"sit");
                    if field.gained_focus(){self.pending.push(Cmd::Focus);}self.settings_editing|=field.has_focus();
                    if d!=0{self.settings.sit_minutes=(self.settings.sit_minutes as i32+d*5).clamp(15,180) as u32;}
                    sc.text(44.0,row_y(1)+13.0,"專注計時進行中不打擾，專注結束後再提醒",10.5,W::Regular,theme::GRAY,H::Left);
                    sc.text(44.0,row_y(2)+13.0,"提醒後常駐，點選確認或起身後重新計時",10.5,W::Regular,theme::GRAY,H::Left);
                }
                // 最後一行兩邊共用：提醒時段 + 午休的摘要，點進二級頁修改。
                let y=row_y(3);
                let r=c.hit(40.0,y-2.0,334.0,22.0,"health-time").on_hover_text("設定提醒時段和午休免打擾");
                let (hv,_)=c.hover_t(&r);
                if hv>0.0{c.fill(40.0,y-2.0,334.0,22.0,8.0,Color32::from_white_alpha((hv*14.0) as u8));}
                c.text(44.0,y+13.0,"提醒時段",10.5,W::Regular,theme::GRAY,H::Left);
                c.text(356.0,y+13.0,&self.health_time_summary(),10.5,W::Semibold,theme::WHITE,H::Right);
                c.text(366.0,y+13.5,"›",14.0,W::Bold,theme::GRAY,H::Center);
                if r.clicked(){self.health_time=true;}
            }
            2=>{
                for (i,name) in ["專注","休息"].iter().enumerate(){
                    let y=53.0+i as f32*23.0;
                    c.text(48.0,y+13.0,name,10.5,W::Regular,theme::GRAY,H::Left);
                    let value=if i==0{&mut self.settings.focus_minutes}else{&mut self.settings.rest_minutes};
                    let max=if i==0{180}else{60};let rect=c.rect(168.0,y,76.0,19.0);
                    let field=island_number_field(c,rect,egui::DragValue::new(value).range(1..=max).suffix(" 分鐘"),("focus-duration",i));
                    if field.gained_focus(){self.pending.push(Cmd::Focus);}
                    self.settings_editing|=field.has_focus();
                    if setting_button(c,131.0,y,27.0,"−",false,&format!("focus-minus-{i}")){*value=value.saturating_sub(1).max(1);}
                    if setting_button(c,255.0,y,27.0,"+",false,&format!("focus-plus-{i}")){*value=(*value+1).min(max);}
                }
                c.text(315.0,93.0,"下次生效",10.5,W::Regular,theme::GRAY,H::Center);
            }
            _=>{
                if setting_button(c,43.0,56.0,152.0,if self.settings.notifications{"提醒 · 開"}else{"提醒 · 關"},self.settings.notifications,"set-notice"){self.settings.notifications=!self.settings.notifications;}
                if setting_button(c,212.0,56.0,153.0,if self.settings.sound{"聲音 · 開"}else{"聲音 · 關"},self.settings.sound,"set-sound"){self.settings.sound=!self.settings.sound;}
                if setting_button(c,43.0,84.0,152.0,if self.settings.read_notifications{"斷開系統通知"}else{"連線系統通知"},self.settings.read_notifications,"set-listener"){self.pending.push(if self.settings.read_notifications{Cmd::DisconnectNotifications}else{Cmd::RequestNotifications});}
                let status=c.fit(&self.listener_status,10.5,W::Regular,170.0);c.text(210.0,97.0,&status,10.5,W::Regular,theme::GRAY,H::Left);
            }
        }
        if before!=format!("{:?}",self.settings){self.settings.save();}
    }
}

fn switch(ui:&mut egui::Ui,value:&mut bool)->bool {
    let (rect,response)=ui.allocate_exact_size(egui::vec2(34.0,20.0),egui::Sense::click());
    if response.clicked(){*value=!*value;}
    let fill=if *value{Color32::from_rgb(48,184,92)}else{settings_color(&ui.ctx().clone(),Color32::from_rgb(82,82,87),Color32::from_rgb(203,207,214))};
    ui.painter().rect_filled(rect,10.0,fill);
    let x=if *value{rect.right()-10.0}else{rect.left()+10.0};
    ui.painter().circle_filled(egui::pos2(x,rect.center().y+0.5),7.5,Color32::from_black_alpha(35));
    ui.painter().circle_filled(egui::pos2(x,rect.center().y),7.0,Color32::WHITE);
    response.clicked()
}
/// 設定頁版式：標籤列寬、開關尺寸、控制元件與卡片右邊緣之間的固定留白。
pub(crate) const SETTINGS_LABEL_W: f32 = 80.0;
const SETTINGS_SWITCH_W: f32 = 34.0;
const SETTINGS_ROW_GAP: f32 = 16.0;

/// 開關行：左側標題 + 說明（寬度固定、自動換行），右側開關，長說明也不會把開關擠出卡片。
pub(crate) fn setting_row(ui:&mut egui::Ui,title:&str,description:&str,value:&mut bool){
    let ctx=&ui.ctx().clone();
    ui.add_space(2.0);
    ui.horizontal(|ui|{
        let text_w=(ui.available_width()-SETTINGS_SWITCH_W-SETTINGS_ROW_GAP).max(120.0);
        ui.vertical(|ui|{
            ui.set_width(text_w);
            ui.spacing_mut().item_spacing.y=2.0;
            ui.add(egui::Label::new(egui::RichText::new(title).size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41)))).wrap());
            if !description.is_empty(){ui.add(egui::Label::new(egui::RichText::new(description).size(11.5).color(settings_color(ctx,Color32::from_gray(175),Color32::from_gray(90)))).wrap());}
        });
        // add_space 之後還會再插入一次 item_spacing.x，這裡一併扣掉，開關才不會每行往外擠一點。
        ui.add_space((ui.available_width()-SETTINGS_SWITCH_W-ui.spacing().item_spacing.x).max(0.0));
        switch(ui,value);
    });
    ui.add_space(2.0);
}

/// 設定頁統一的分段選擇器（灰色底框 + 選中塊）。返回值為是否發生了切換。
pub(crate) fn settings_segmented<T:PartialEq+Copy>(ui:&mut egui::Ui,value:&mut T,options:&[(T,&str)],item_w:f32)->bool{
    let ctx=&ui.ctx().clone();
    let mut changed=false;
    egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(28,28,30),Color32::from_rgb(230,233,238)))
        .corner_radius(8.0).inner_margin(3).show(ui,|ui|{
            ui.spacing_mut().item_spacing.x=2.0;
            for (option,label) in options {
                let selected=*value==*option;
                let text=egui::RichText::new(*label).size(12.0).color(if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(200),Color32::from_gray(70))});
                let button=egui::Button::new(text)
                    .fill(if selected{settings_color(ctx,Color32::from_rgb(72,72,76),Color32::WHITE)}else{Color32::TRANSPARENT})
                    .stroke(egui::Stroke::NONE).corner_radius(6.0);
                let width=item_w.max(ctx.fonts(|f|f.layout_no_wrap(label.to_string(),egui::FontId::proportional(12.0),Color32::WHITE).size().x)+20.0);
                if ui.add_sized([width,26.0],button).clicked() && !selected {*value=*option;changed=true;}
            }
        });
    changed
}

/*
/// 分段選擇器，可依每行上限分成多行，確保多選項在小視窗寬度下不截斷或溢位。
pub(crate) fn settings_segmented_rows<T:PartialEq+Copy>(ui:&mut egui::Ui,value:&mut T,options:&[(T,&str)],per_row:usize,item_w:f32)->bool{
    let per_row=per_row.max(1);
    if options.len()<=per_row { return settings_segmented(ui,value,options,item_w); }
    let ctx=&ui.ctx().clone();
    let mut changed=false;
    egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(28,28,30),Color32::from_rgb(230,233,238)))
        .corner_radius(8.0).inner_margin(3).show(ui,|ui|{
            ui.vertical(|ui|{
                ui.spacing_mut().item_spacing=egui::vec2(2.0,2.0);
                for row in options.chunks(per_row) {
                    ui.horizontal(|ui|{
                        ui.spacing_mut().item_spacing.x=2.0;
                        for (option,label) in row {
                            let selected=*value==*option;
                            let text=egui::RichText::new(*label).size(12.0).color(if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(200),Color32::from_gray(70))});
                            let button=egui::Button::new(text)
                                .fill(if selected{settings_color(ctx,Color32::from_rgb(72,72,76),Color32::WHITE)}else{Color32::TRANSPARENT})
                                .stroke(egui::Stroke::NONE).corner_radius(6.0);
                            let width=item_w.max(ctx.fonts(|f|f.layout_no_wrap(label.to_string(),egui::FontId::proportional(12.0),Color32::WHITE).size().x)+20.0);
                            if ui.add_sized([width,26.0],button).clicked() && !selected {*value=*option;changed=true;}
                        }
                    });
                }
            });
        });
    changed
}
*/

/// 選項行：標籤列之後緊跟控制元件，同一卡片裡所有控制元件左緣對齊成一條豎線。
pub(crate) fn settings_control_row_right(ui:&mut egui::Ui,title:&str,controls:impl FnOnce(&mut egui::Ui))->egui::Response{
    settings_control_row(ui,title,SETTINGS_LABEL_W,|ui|ui.horizontal(|ui|controls(ui)).response)
}

/// 設定項下方的說明文字。
fn island_size_slider(ui:&mut egui::Ui,value:&mut u32)->egui::Response {
    let id=ui.make_persistent_id("island-size-draft");
    let mut draft=ui.data(|d|d.get_temp::<u32>(id)).unwrap_or(*value);
    let response=ui.add(egui::Slider::new(&mut draft,crate::model::ISLAND_SIZE_MIN..=crate::model::ISLAND_SIZE_MAX)
        .step_by(crate::model::ISLAND_SIZE_STEP as f64).suffix("%"));
    if ui.input(|i|i.pointer.primary_down()) {
        if response.changed(){ui.data_mut(|d|d.insert_temp(id,draft));}
    }else{
        *value=crate::model::island_size_value(draft);
        ui.data_mut(|d|d.remove::<u32>(id));
    }
    response
}

pub(crate) fn settings_hint(ui:&mut egui::Ui,text:&str){
    let ctx=&ui.ctx().clone();
    ui.add(egui::Label::new(egui::RichText::new(text).size(11.5).color(settings_color(ctx,Color32::from_gray(160),Color32::from_gray(105)))).wrap());
}
pub(crate) fn settings_card(ui:&mut egui::Ui,add:impl FnOnce(&mut egui::Ui)) {
    let ctx=&ui.ctx().clone();
    egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(35,35,38),Color32::from_rgb(255,255,255)))
        .corner_radius(10.0).inner_margin(egui::Margin::symmetric(16,12))
        .show(ui,|ui|{ui.set_min_width(ui.available_width());ui.spacing_mut().item_spacing.y=8.0;add(ui);});
}
/// 設定分頁對應的功能模組（頁面序號）；常規、通知、關於、頁面管理不屬於任何模組。
pub(crate) fn settings_tab_module(tab:usize)->Option<usize>{
    use crate::app::*;
    match tab {1=>Some(MUSIC_PAGE),2=>Some(FOCUS_PAGE),3=>Some(WATER_PAGE),4=>Some(TODO_PAGE),5=>Some(SHELF_PAGE),7=>Some(MAIL_PAGE),8=>Some(CALENDAR_PAGE),11=>Some(QUOTA_PAGE),12=>Some(WEATHER_PAGE),_=>None}
}

pub(crate) fn settings_label(ui:&mut egui::Ui,text:&str){
    let ctx=&ui.ctx().clone();
    ui.add_space(9.0);
    ui.label(egui::RichText::new(text).size(11.5).color(settings_color(ctx,Color32::from_gray(185),Color32::from_gray(70))));
    ui.add_space(2.0);
}
pub(crate) fn settings_control_row(ui:&mut egui::Ui,title:&str,label_width:f32,controls:impl FnOnce(&mut egui::Ui)->egui::Response)->egui::Response{
    ui.horizontal(|ui|{
        // 同一張卡片裡的控制元件左緣對齊：標籤列至少 SETTINGS_LABEL_W。
        let (label_rect,_)=ui.allocate_exact_size(egui::vec2(label_width.max(SETTINGS_LABEL_W),30.0),egui::Sense::hover());
        let content=controls(ui);
        ui.painter().text(egui::pos2(label_rect.left(),content.rect.center().y),egui::Align2::LEFT_CENTER,title,egui::TextStyle::Body.resolve(ui.style()),ui.visuals().text_color());
        content
    }).inner
}
fn setting_number<N:egui::emath::Numeric>(ui:&mut egui::Ui,title:&str,value:&mut N,min:N,max:N,suffix:&str,speed:f64)->bool{
    let mut changed=false;
    settings_control_row(ui,title,80.0,|ui|{
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
            ui.allocate_ui_with_layout(egui::vec2(140.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                changed=number_stepper(ui,value,min,max,suffix,N::from_f64(speed),76.0);
            });
        }).response
    });
    changed
}
fn number_stepper<N:egui::emath::Numeric>(ui:&mut egui::Ui,value:&mut N,min:N,max:N,suffix:&str,step:N,field_width:f32)->bool{
    let ctx=&ui.ctx().clone();
    let mut changed=false;
    ui.scope(|ui|{
        ui.spacing_mut().item_spacing.x=6.0;
        ui.spacing_mut().button_padding=egui::vec2(4.0,2.0);
        let button=|text|egui::Button::new(egui::RichText::new(text).size(16.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))))
            .fill(settings_color(ctx,Color32::from_rgb(48,48,51),Color32::from_rgb(229,232,238))).stroke(egui::Stroke::NONE).corner_radius(13.0);
        let decrease=ui.add_enabled(*value>min,button("−").min_size(egui::vec2(26.0,26.0))).on_hover_text("減少");
        if decrease.clicked(){
            *value=N::from_f64((value.to_f64()-step.to_f64()).max(min.to_f64()));changed=true;
        }
        let field=ui.add_sized([field_width,28.0],egui::DragValue::new(value).range(min..=max).speed(step.to_f64()).suffix(suffix).fixed_decimals(if N::INTEGRAL{0}else{2}));
        changed|=field.changed();
        if ui.add_enabled(*value<max,button("+").min_size(egui::vec2(26.0,26.0))).on_hover_text("增加").clicked(){
            *value=N::from_f64((value.to_f64()+step.to_f64()).min(max.to_f64()));changed=true;
        }
    });
    changed
}
/// 時刻加減器（一天中的分鐘數，15 分鐘一檔，跨午夜迴圈），中間可直接輸入 12:30。
fn time_stepper(ui:&mut egui::Ui,value:&mut u32,id:&str)->bool{
    let ctx=&ui.ctx().clone();
    let mut changed=false;
    ui.push_id(id,|ui|{
        ui.spacing_mut().item_spacing.x=6.0;
        ui.spacing_mut().button_padding=egui::vec2(4.0,2.0);
        let button=|text|egui::Button::new(egui::RichText::new(text).size(16.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))))
            .fill(settings_color(ctx,Color32::from_rgb(48,48,51),Color32::from_rgb(229,232,238))).stroke(egui::Stroke::NONE).corner_radius(13.0);
        if ui.add(button("−").min_size(egui::vec2(26.0,26.0))).on_hover_text("提前 15 分鐘").clicked(){*value=(*value+1440-15)%1440;changed=true;}
        let field=ui.add_sized([56.0,28.0],egui::DragValue::new(value).range(0..=1439).speed(1.0)
            .custom_formatter(|v,_|{let m=v as u32;format!("{:02}:{:02}",m/60,m%60)})
            .custom_parser(|t|{let (h,m)=t.trim().split_once([':','：'])?;Some((h.parse::<u32>().ok()?.min(23)*60+m.parse::<u32>().ok()?.min(59)) as f64)}));
        changed|=field.changed();
        if ui.add(button("+").min_size(egui::vec2(26.0,26.0))).on_hover_text("推後 15 分鐘").clicked(){*value=(*value+15)%1440;changed=true;}
    });
    changed
}
fn setting_choices(ui:&mut egui::Ui,title:&str,value:&mut u32,values:[u32;3],unit:&str){
    let labels=values.map(|v|format!("{v} {unit}"));
    let options:Vec<(u32,&str)>=values.iter().zip(labels.iter()).map(|(v,l)|(*v,l.as_str())).collect();
    settings_control_row_right(ui,title,|ui|{settings_segmented(ui,value,&options,62.0);});
}
const SETTINGS_GROUPS: [(&str,usize,usize);9] = [
    ("常規",0,0), ("頁面管理",10,10), ("音樂與歌詞",1,1), ("專注與飲水",2,3),
    ("待辦與置物",4,5), ("通知、信箱與日程",6,8), ("AI Usage",11,11), ("天氣",12,12), ("關於",9,9),
];

impl App {
    pub fn settings_ui(&mut self,ctx:&egui::Context,autostart:bool){
        if !self.settings_open{self.window_layout().close();self.weather_source.close();}
        self.updates.poll(ctx);
        self.updates.dialog_ui(ctx);
        if !self.settings_open{return;}
        let id=egui::ViewportId::from_hash_of("daozai-settings");
        let focus_requested=std::mem::take(&mut self.settings_focus_requested);
        let opening=self.window_layout().open();
        ctx.show_viewport_immediate(id,egui::ViewportBuilder::default().with_title("島仔設定").with_icon(theme::app_icon()).with_inner_size([690.0,560.0]).with_min_inner_size([600.0,450.0]).with_visible(false).with_active(false).with_window_level(egui::WindowLevel::Normal),|ctx,_|{
            let restored=opening && self.window_layout().restore(ctx);
            if !restored{self.window_layout().observe(ctx);}
            let dark=self.settings.settings_theme.dark(crate::settings_theme::system_dark(ctx));
            let original_style=ctx.style();
            let mut style=(*original_style).clone();
            style.visuals=if dark{egui::Visuals::dark()}else{egui::Visuals::light()};
            ctx.set_style(style);
            ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if dark{egui::SystemTheme::Dark}else{egui::SystemTheme::Light}));
            if self.settings.settings_theme==crate::settings_theme::SettingsTheme::System {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            if focus_requested {
                // Glow 在首幀交換緩衝後執行視窗命令，避免先顯示白色空視窗。
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if ctx.input(|i|i.viewport().close_requested()){self.settings_open=false;self.quota_close_connection();}
            let before=format!("{:?}",self.settings);
            egui::SidePanel::left("settings-sidebar").resizable(false).exact_width(180.0).frame(egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(29,29,32),Color32::from_rgb(237,239,243))).stroke(egui::Stroke::new(1.0,settings_color(ctx,Color32::from_rgb(42,42,45),Color32::from_rgb(216,219,225)))).inner_margin(egui::Margin::symmetric(10,12))).show(ctx,|ui|{
                ui.add_space(5.0);
                ui.label(egui::RichText::new("設定").size(11.0).color(settings_color(ctx,Color32::from_gray(150),Color32::from_gray(105))));
                ui.add_space(7.0);
                for (name,start,end) in SETTINGS_GROUPS {
                    if start==9 {ui.add_space((ui.available_height()-38.0).max(0.0));}
                    let selected=(start..=end).contains(&self.settings_tab);
                    let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),36.0),egui::Sense::click());
                    if response.clicked() && !selected{self.settings_tab=start;}
                    if selected || response.hovered(){
                        ui.painter().rect_filled(rect,7.0,if selected{settings_color(ctx,Color32::from_rgb(43,43,46),Color32::from_rgb(214,221,232))}else{settings_color(ctx,Color32::from_rgb(30,30,33),Color32::from_rgb(225,230,238))});
                    }
                    let icon=match start {
                        0=>egui::include_image!("../assets/settings-icons/system/general.svg"),
                        11=>egui::include_image!("../assets/settings-icons/system/quota.svg"),
                        10=>egui::include_image!("../assets/settings-icons/system/pages.svg"),
                        1=>egui::include_image!("../assets/settings-icons/system/music.svg"),
                        2=>egui::include_image!("../assets/settings-icons/system/focus.svg"),
                        4=>egui::include_image!("../assets/settings-icons/system/tasks.svg"),
                        6=>egui::include_image!("../assets/settings-icons/system/notifications.svg"),
                        12=>egui::include_image!("../assets/settings-icons/system/weather.svg"),
                        _=>egui::include_image!("../assets/settings-icons/system/about.svg"),
                    };
                    let icon_bg=egui::Rect::from_center_size(egui::pos2(rect.left()+21.0,rect.center().y),egui::vec2(25.0,25.0));
                    egui::Image::new(icon).paint_at(ui,icon_bg);
                    ui.painter().text(egui::pos2(rect.left()+40.0,rect.center().y),egui::Align2::LEFT_CENTER,name,egui::FontId::proportional(13.0),if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(225),Color32::from_gray(55))});
                    ui.add_space(3.0);
                }
            });
            egui::CentralPanel::default().frame(egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(25,25,27),Color32::from_rgb(246,247,249))).inner_margin(egui::Margin::symmetric(22,16))).show(ctx,|ui|{
                ui.spacing_mut().item_spacing=egui::vec2(8.0,5.0);
                ui.spacing_mut().button_padding=egui::vec2(10.0,6.0);
                ui.style_mut().text_styles.insert(egui::TextStyle::Small,egui::FontId::proportional(12.0));
                ui.visuals_mut().override_text_color=Some(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41)));
                ui.visuals_mut().extreme_bg_color=settings_color(ctx,Color32::from_rgb(25,25,28),Color32::from_rgb(244,246,249));
                ui.visuals_mut().widgets.inactive.bg_fill=settings_color(ctx,Color32::from_rgb(50,50,54),Color32::from_rgb(228,231,236));
                ui.visuals_mut().widgets.hovered.bg_fill=settings_color(ctx,Color32::from_rgb(65,65,71),Color32::from_rgb(216,222,231));
                // 統一按鈕：高度 28、圓角 7、內邊距 12×5，無描邊；預設按鈕也套用這套樣式。
                ui.spacing_mut().button_padding=egui::vec2(12.0,5.0);
                ui.spacing_mut().interact_size.y=28.0;
                {
                    let fill=settings_color(ctx,Color32::from_rgb(50,50,54),Color32::from_rgb(228,231,236));
                    let hover=settings_color(ctx,Color32::from_rgb(65,65,71),Color32::from_rgb(216,222,231));
                    let widgets=&mut ui.visuals_mut().widgets;
                    for (w,f) in [(&mut widgets.inactive,fill),(&mut widgets.hovered,hover),(&mut widgets.active,hover)] {
                        w.corner_radius=egui::CornerRadius::same(7);
                        w.weak_bg_fill=f;
                        w.bg_stroke=egui::Stroke::NONE;
                    }
                }
                ui.visuals_mut().selection.bg_fill=settings_color(ctx,Color32::from_rgb(45,105,165),Color32::from_rgb(209,226,248));
                let titles=["常規","音樂與歌詞","專注","健康提醒","待辦","置物","通知","信箱","日程","關於","頁面管理","AI Usage","天氣"];
                ui.label(egui::RichText::new(titles[self.settings_tab.min(12)]).size(18.0).strong().color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                if let Some((_,start,end))=SETTINGS_GROUPS.iter().find(|(_,start,end)|(*start..=*end).contains(&self.settings_tab)) {
                    if start!=end {
                        ui.add_space(7.0);
                        egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(33,33,36),Color32::from_rgb(232,235,240))).corner_radius(8.0).inner_margin(3).show(ui,|ui|{
                            ui.spacing_mut().item_spacing.x=2.0;
                            ui.horizontal(|ui|{
                                for index in *start..=*end {
                                    let selected=self.settings_tab==index;
                                    let button=egui::Button::new(egui::RichText::new(titles[index]).size(12.0).color(if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))}))
                                        .fill(if selected{settings_color(ctx,Color32::from_rgb(70,70,74),Color32::from_rgb(205,215,229))}else{Color32::TRANSPARENT})
                                        .stroke(egui::Stroke::NONE).corner_radius(6.0);
                                    if ui.add_sized([68.0,26.0],button).clicked(){self.settings_tab=index;}
                                }
                            });
                        });
                    }
                }
                ui.add_space(6.0);
                egui::ScrollArea::vertical().id_salt(("settings-scroll",self.settings_tab)).auto_shrink([false,false]).show(ui,|ui|{
                    // 右側給捲軸留出空隙，卡片和開關不會被它壓住。
                    ui.set_width((ui.available_width()-12.0).max(240.0));
                    // 模組已關閉：頂部說明並提供開啟入口，下面的設定置灰。
                    if let Some(page)=settings_tab_module(self.settings_tab).filter(|page|!self.module_on(*page)) {
                        let mut enable=false;
                        settings_card(ui,|ui|{
                            ui.horizontal(|ui|{
                                ui.label(egui::RichText::new(format!("「{}」已關閉，不會在後臺同步或提醒",PAGE_NAMES[page])).size(12.5));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{enable=ui.button("開啟").clicked();});
                            });
                        });
                        if enable {self.set_page_visible(page,true);}
                        ui.add_space(6.0);
                        if !self.module_on(page) {ui.disable();}
                    }
                    match self.settings_tab {
                    12=>self.weather_settings(ui),
                    11=>self.quota_settings(ui),
                    10=>{
                        ui.label(egui::RichText::new("選擇需要的頁面和狀態元件。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                        settings_label(ui,"常駐排列");
                        settings_card(ui,|ui|self.resident_order_settings(ui));
                        settings_label(ui,"狀態元件");
                        settings_card(ui,|ui|{
                            setting_row(ui,"時間","收起時顯示時鐘；關閉後仍可在展開頂欄檢視",&mut self.settings.show_clock);
                            ui.separator();
                            setting_row(ui,"電量","本機電量、充電狀態與提醒；無電池裝置自動隱藏",&mut self.settings.show_battery);
                            ui.separator();
                            setting_row(ui,"天氣","收起時顯示天氣圖示和溫度，點選開啟天氣頁",&mut self.settings.show_weather);
                            ui.separator();
                            setting_row(ui,"島上歌詞","收起時顯示歌詞，無歌詞時顯示封面和歌名",&mut self.settings.island_lyrics);
                        });
                        settings_label(ui,"功能頁面");
                        settings_card(ui,|ui|{
                            let descriptions=["播放控制與同步歌詞","番茄鍾與休息計時","喝水與久坐提醒","臨時收納檔案位置","任務清單與到期提醒","郵件摘要與驗證碼","Agent 剩餘額度和帳號切換","月曆、郵件邀請與工作日程","當地天氣與七日預報"];
                            ui.label(egui::RichText::new("拖動左側手柄調整順序；關閉的功能不在後臺執行，不同步也不提醒，已有資料保留。").size(11.5).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            #[derive(Clone)]
                            struct PageDrag(usize);
                            let mut movement=None;
                            let row_w=ui.available_width();
                            for (index,page) in self.settings.page_order.into_iter().enumerate() {
                                if index>0 {ui.separator();}
                                let mut visible=self.settings.visible_pages[page];
                                let can_change=!visible || self.settings.visible_pages.iter().filter(|enabled|**enabled).count()>1;
                                let row=ui.push_id(("page-order",page),|ui|ui.horizontal(|ui|{
                                    ui.set_max_width(row_w);
                                    let (rect,handle)=ui.allocate_exact_size(egui::vec2(20.0,40.0),egui::Sense::drag());
                                    for x in [-3.0,3.0] {for y in [-6.0,0.0,6.0] {
                                        ui.painter().circle_filled(rect.center()+egui::vec2(x,y),1.4,settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)));
                                    }}
                                    handle.on_hover_cursor(egui::CursorIcon::Grab).dnd_set_drag_payload(PageDrag(page));
                                    ui.add_enabled_ui(can_change,|ui|{setting_row(ui,PAGE_NAMES[page],descriptions[page],&mut visible);});
                                })).inner.response;
                                if let Some(payload)=row.dnd_hover_payload::<PageDrag>() {
                                    if payload.0!=page {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                        let y=if self.settings.page_order.iter().position(|p|*p==payload.0).unwrap()<index {row.rect.bottom()}else{row.rect.top()};
                                        ui.painter().line_segment([egui::pos2(row.rect.left(),y),egui::pos2(row.rect.right(),y)],egui::Stroke::new(2.0,Color32::from_rgb(70,150,255)));
                                    }
                                }
                                if let Some(payload)=row.dnd_release_payload::<PageDrag>() {movement=Some((payload.0,index));}
                                if visible!=self.settings.visible_pages[page] {self.set_page_visible(page,visible);}
                            }
                            if let Some((page,to))=movement {
                                let from=self.settings.page_order.iter().position(|p|*p==page).unwrap();
                                if from<to {self.settings.page_order[from..=to].rotate_left(1);}
                                else {self.settings.page_order[to..=from].rotate_right(1);}
                            }
                            if self.settings.visible_pages.iter().filter(|enabled|**enabled).count()==1 {
                                ui.add_space(5.0);
                                ui.label(egui::RichText::new("至少保留一個頁面。").size(11.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            }
                        });
                    }
                    0=>{
                        settings_label(ui,"設定頁外觀");
                        settings_card(ui,|ui|{
                            use crate::settings_theme::SettingsTheme;
                            settings_control_row_right(ui,"主題",|ui|{
                                if settings_segmented(ui,&mut self.settings.settings_theme,&[(SettingsTheme::System,"跟隨系統"),(SettingsTheme::Light,"淺色"),(SettingsTheme::Dark,"深色")],52.0) {ctx.request_repaint();}
                            });
                            settings_hint(ui,"僅影響設定頁，島的外觀保持不變。");
                        });
                        settings_label(ui,"啟動");
                        settings_card(ui,|ui|{
                            let mut a=autostart;
                            setting_row(ui,"開機啟動","登入 Windows 時自動開啟島仔",&mut a);
                            if a!=autostart{self.pending.push(Cmd::ToggleAutostart);}
                        });
                        settings_label(ui,"圖形渲染");
                        settings_card(ui,|ui|{
                            let before=self.settings.render_compat;
                            setting_row(ui,"相容渲染（Direct3D 12）","島周圍出現黑框、畫面閃爍或錯亂，或開啟設定後崩潰退出時開啟；切換後重啟島仔生效",&mut self.settings.render_compat);
                            // 手動關閉後不再顯示自動切換的原因；再次開啟視為手動開啟。
                            if before!=self.settings.render_compat {self.settings.render_compat_reason.clear();}
                            if !self.settings.render_compat_reason.is_empty() {settings_hint(ui,&self.settings.render_compat_reason);}
                            if self.settings.render_compat!=self.render_compat_running {
                                ui.horizontal(|ui|{
                                    ui.label(egui::RichText::new("重啟島仔後生效").size(12.0).color(Color32::from_rgb(230,150,40)));
                                    if ui.button("立即重啟").clicked(){self.pending.push(Cmd::RestartApp);}
                                });
                            }
                        });
                        settings_label(ui,"島體外觀");
                        settings_card(ui,|ui|{
                            use crate::model::{self as model,Skin};
                            let skins:Vec<(Skin,&str)>=Skin::ALL.iter().map(|s|(*s,s.label())).collect();
                            settings_control_row_right(ui,"皮膚",|ui|{settings_segmented(ui,&mut self.settings.skin,&skins,64.0);});
                            settings_hint(ui,self.settings.skin.description());
                            ui.separator();
                            let shapes:Vec<(model::IslandShape,&str)>=model::IslandShape::ALL.iter().map(|s|(*s,s.label())).collect();
                            settings_control_row_right(ui,"形狀",|ui|{settings_segmented(ui,&mut self.settings.island_shape,&shapes,64.0);});
                            settings_hint(ui,self.settings.island_shape.description());
                            if self.settings.skin.is_glass() {
                                ui.separator();
                                let mut blur=self.settings.glass_blur;
                                setting_row(ui,"背景模糊","使用 Windows 合成引擎的實時模糊，嚴格貼合島體形狀（需開啟系統“透明效果”）；關閉後改為煙燻半透明",&mut blur);
                                self.settings.glass_blur=blur;
                            }
                            ui.separator();
                            settings_control_row(ui,"大小",56.0,|ui|{
                                ui.horizontal(|ui|{
                                    ui.spacing_mut().slider_width=180.0;
                                    island_size_slider(ui,&mut self.settings.island_size);
                                    if ui.add_enabled(self.settings.island_size!=100,egui::Button::new("恢復預設")).clicked(){self.settings.island_size=100;}
                                }).response
                            });
                            settings_hint(ui,"文字、圖示和吉祥物按實際畫素重新繪製，放大後依舊清晰；隻影響島體，不影響設定視窗。");
                        });
                        settings_label(ui,"島體顯示");
                        settings_card(ui,|ui|{
                            let selected=self.displays.iter().find(|d|d.id==self.settings.display_id);
                            let label=if self.settings.display_id.is_empty(){"跟隨主屏"}else{selected.map_or("所選螢幕未連線",|d|d.label.as_str())};
                            settings_control_row(ui,"顯示屏",56.0,|ui|{
                                egui::ComboBox::from_id_salt("island-display").selected_text(label).width(ui.available_width().min(300.0)).truncate().show_ui(ui,|ui|{
                                    ui.selectable_value(&mut self.settings.display_id,String::new(),"跟隨主屏");
                                    for display in &self.displays {ui.selectable_value(&mut self.settings.display_id,display.id.clone(),&display.label);}
                                }).response
                            });
                            let edge=self.settings.dock.label();
                            ui.label(egui::RichText::new(if !self.settings.display_id.is_empty()&&selected.is_none(){"暫時顯示在主屏，重新連線所選螢幕後自動恢復。".to_string()}else{format!("顯示在所選螢幕{edge}；斷開時暫時回到主屏。")}).size(11.0).color(ui.visuals().weak_text_color()));
                            ui.add_space(4.0);
                            // 停靠邊緣：頂部為橫向；左右為豎向排列，展開面板從邊緣滑出。
                            let docks:Vec<(crate::model::Dock,&str)>=crate::model::Dock::ALL.iter().map(|d|(*d,d.label())).collect();
                            settings_control_row_right(ui,"停靠",|ui|{
                                // 換邊後偏移的含義變了（水平 ↔ 上下），回到居中。
                                if settings_segmented(ui,&mut self.settings.dock,&docks,64.0) {self.settings.island_offset=0;}
                            });
                            if self.settings.dock.is_side() {
                                ui.label(egui::RichText::new("收起時豎向排列時間、電池、播放、專注和 AI Usage；展開面板從螢幕邊緣滑出。吉祥物和島上歌詞只在頂部停靠時顯示。").size(11.0).color(ui.visuals().weak_text_color()));
                            }
                            ui.add_space(4.0);
                            let side=self.settings.dock.is_side();
                            settings_control_row(ui,if side{"上下位置"}else{"水平位置"},56.0,|ui|{
                                ui.horizontal(|ui|{
                                    let limit=crate::model::ISLAND_OFFSET_MAX;
                                    let offset=&mut self.settings.island_offset;
                                    let (less,more,less_tip,more_tip)=if side{("↑","↓","向上微調","向下微調")}else{("←","→","向左微調","向右微調")};
                                    if ui.add_enabled(*offset>-limit,egui::Button::new(less).min_size(egui::vec2(26.0,22.0))).on_hover_text(less_tip).clicked(){*offset=(*offset-5).max(-limit);}
                                    ui.spacing_mut().slider_width=150.0;
                                    ui.add(egui::Slider::new(&mut *offset,-limit..=limit).show_value(false));
                                    if ui.add_enabled(*offset<limit,egui::Button::new(more).min_size(egui::vec2(26.0,22.0))).on_hover_text(more_tip).clicked(){*offset=(*offset+5).min(limit);}
                                    if ui.add_enabled(*offset!=0,egui::Button::new("恢復居中")).clicked(){*offset=0;}
                                }).response
                            });
                            let offset=self.settings.island_offset;
                            let (less,more)=if side{("偏上","偏下")}else{("偏左","偏右")};
                            let position=match offset {0=>"居中".to_string(),o if o<0=>format!("{less} {}%",-o),o=>format!("{more} {o}%")};
                            ui.label(egui::RichText::new(format!("當前：{position}。也可以展開後按住日期時間，拖到螢幕頂部、左側或右側；展開面板和通知始終留在螢幕內，換屏或調整大小後保持相對位置。")).size(11.0).color(ui.visuals().weak_text_color()));
                            ui.add_space(4.0);
                            setting_row(ui,"OLED 畫素偏移","收起靜止時讓島體沿螢幕邊緣緩慢移動幾個畫素，幫助減輕 OLED 靜態顯示留下的痕跡；不會改動你儲存的位置",&mut self.settings.oled_shift);
                            if self.settings.oled_shift {
                                settings_hint(ui,"每 45 秒移動 1 畫素、單方向最多 3 畫素；頂部停靠左右移動，左右停靠上下移動。展開、點選或拖拽時保持當前位置，位置拉滿或螢幕太窄時少挪或不挪。");
                            }
                            ui.add_space(4.0);
                            setting_row(ui,"頂欄適配","頂部有選單欄類軟體或工作列時，選擇島體的顯示方式",&mut self.settings.avoid_top_bar);
                            if self.settings.avoid_top_bar {
                                settings_control_row_right(ui,"顯示方式",|ui|{
                                    settings_segmented(ui,&mut self.settings.top_bar_on_top,&[(false,"避開頂欄"),(true,"島仔置頂")],90.0);
                                });
                                if self.settings.top_bar_on_top {
                                    settings_hint(ui,"頂部停靠時，島體保持在螢幕頂邊，顯示在頂欄上方。");
                                } else {
                                    let reserved=crate::display_layout::selected(&self.displays,&self.settings.display_id).map_or(0,|d|{
                                        let b=crate::display_layout::placement_bounds(d,true);(b.top-d.bounds.top).max(0)
                                    });
                                    settings_hint(ui,&if reserved>0 {format!("已檢測到頂欄，島體下移 {reserved} 畫素。")}else{"未檢測到頂欄；未向系統登記佔用區域的頂欄可嘗試“島仔置頂”。".to_string()});
                                }
                            }
                            ui.separator();
                            settings_control_row_right(ui,"時間制式",|ui|{settings_segmented(ui,&mut self.settings.clock_12h,&[(false,"24 小時"),(true,"12 小時")],72.0);});
                            settings_control_row_right(ui,"時段文字",|ui|{settings_segmented(ui,&mut self.settings.clock_period,&[(0,"不顯示"),(1,"上午 / 下午"),(2,"詳細時段")],72.0);});
                            settings_hint(ui,"詳細時段：凌晨、上午、中午、下午、傍晚、晚上");
                            setting_row(ui,"顯示秒數","時間顯示為 14:32:08",&mut self.settings.clock_seconds);
                            ui.separator();
                            let mut minimal=self.settings.minimal;
                            setting_row(ui,"極簡劉海","收起時顯示一條細線",&mut minimal);
                            if minimal!=self.settings.minimal {self.set_minimal(minimal);}
                            ui.separator();

                            let mut pet=self.settings.pet;
                            setting_row(ui,"島仔吉祥物","收起時常駐，播放音樂時一起唱歌",&mut pet);
                            if pet!=self.settings.pet {self.set_pet(pet);}
                        });
                        self.avoidance.settings(ui,&mut self.settings);
                        settings_label(ui,"聲音與顯示");
                        settings_card(ui,|ui|{
                            setting_row(ui,"提醒聲音","喝水、久坐、待辦和專注提醒",&mut self.settings.sound);
                            ui.separator();
                            setting_row(ui,"全屏時隱藏","全屏隱藏島仔，健康提醒靜默保留",&mut self.settings.hide_fullscreen);
                            if self.settings.hide_fullscreen {
                                ui.separator();
                                setting_row(ui,"全螢幕隱藏時仍彈出通知","全螢幕看影片或遊戲時收起島體，但收到 LINE、Discord、郵件或行事曆等新通知時暫時浮現，顯示完畢後自動收起",&mut self.settings.fullscreen_show_notices);
                            }
                        });
                    }
                    1=>{
                        settings_label(ui,"歌詞");
                        settings_card(ui,|ui|{
                            setting_row(ui,"展開時顯示歌詞","在播放器右側同步滾動顯示歌詞",&mut self.settings.lyrics_enabled);
                            self.show_lyrics=self.settings.lyrics_enabled;
                            ui.separator();
                            setting_row(ui,"島上歌詞","收起時顯示當前句；無同步歌詞時顯示封面和歌名",&mut self.settings.island_lyrics);
                        });
                        settings_label(ui,"歌詞顯示時間");
                        settings_card(ui,|ui|{
                            let mut advance=-self.settings.lyric_delay_ms as f64/1000.0;
                            if setting_number(ui,"時間偏移",&mut advance,-5.0,5.0," 秒",0.1){self.settings.lyric_delay_ms=(-advance*1000.0).round()as i32;}
                            ui.add_space(3.0);
                            ui.small("正數提前，負數延後；預設提前 0.35 秒。");
                        });
                    }
                    2=>{
                        settings_label(ui,"計時時長");
                        settings_card(ui,|ui|{
                            setting_number(ui,"專注",&mut self.settings.focus_minutes,1,180," 分鐘",1.0);
                            ui.separator();
                            setting_number(ui,"休息",&mut self.settings.rest_minutes,1,60," 分鐘",1.0);
                            ui.add_space(3.0);
                            ui.small("調整後從下一階段開始生效。");
                        });
                    }
                    3=>{
                        settings_label(ui,"喝水提醒");
                        settings_card(ui,|ui|{
                            let old_remind=self.water.remind;
                            setting_row(ui,"定時提醒","確認或喝水後重新計時；達標後停止提醒",&mut self.water.remind);
                            if self.water.remind!=old_remind{self.water.save();self.confirm_health_reminder(HealthReminder::Water);}
                            ui.separator();
                            if setting_number(ui,"每日目標",&mut self.water.goal_ml,500,6000," ml",50.0){self.water.save();}
                            setting_number(ui,"每次記錄",&mut self.settings.serving_ml,50,1000," ml",25.0);
                            setting_number(ui,"提醒間隔",&mut self.settings.water_minutes,5,180," 分鐘",1.0);
                        });
                        settings_label(ui,"久坐提醒");
                        settings_card(ui,|ui|{
                            let old_sit=self.settings.sit_remind;
                            setting_row(ui,"定時提醒","確認提醒或點「起身了」後重新計時",&mut self.settings.sit_remind);
                            if self.settings.sit_remind!=old_sit {self.confirm_health_reminder(HealthReminder::Sit);}
                            ui.separator();
                            setting_number(ui,"久坐時長",&mut self.settings.sit_minutes,15,180," 分鐘",1.0);
                            ui.add_space(3.0);
                            ui.small("專注計時進行中不打擾，專注結束後再提醒。");
                        });
                        settings_label(ui,"提醒時段");
                        settings_card(ui,|ui|{
                            settings_control_row(ui,"提醒時段",80.0,|ui|{
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                    ui.allocate_ui_with_layout(egui::vec2(252.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                                        number_stepper(ui,&mut self.settings.water_start,0,23," 時",1,48.0);
                                        ui.label("至");
                                        number_stepper(ui,&mut self.settings.water_end,0,24," 時",1,48.0);
                                    });
                                }).response
                            });
                            ui.separator();
                            setting_row(ui,"午休不提醒","午休時間內喝水和久坐都不彈提醒",&mut self.settings.lunch_rest);
                            if self.settings.lunch_rest {
                                settings_control_row(ui,"午休時間",80.0,|ui|{
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                        ui.allocate_ui_with_layout(egui::vec2(252.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                                            time_stepper(ui,&mut self.settings.lunch_start,"lunch-start");
                                            ui.label("至");
                                            time_stepper(ui,&mut self.settings.lunch_end,"lunch-end");
                                        });
                                    }).response
                                });
                            }
                            ui.add_space(3.0);
                            ui.small("喝水和久坐提醒只在提醒時段內、午休以外彈出。");
                        });
                    }
                    4=>{
                        settings_label(ui,"待辦清單");
                        settings_card(ui,|ui|{
                            ui.label(egui::RichText::new("在待辦頁新增、編輯任務並設定提醒時間。").size(13.0));
                            ui.add_space(5.0);
                            if ui.add_sized([120.0,32.0],egui::Button::new("開啟待辦頁")).clicked(){self.settings_open=false;self.show_page(4);}
                        });
                    }
                    5=>{
                        settings_label(ui,"置物架");
                        settings_card(ui,|ui|{
                            ui.label(egui::RichText::new("儲存檔案位置，移除條目不會刪除原檔案。").size(13.0));
                            ui.label(egui::RichText::new(format!("當前 {} 項",self.shelf.items.len())).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            ui.add_space(5.0);
                            if ui.add_sized([120.0,32.0],egui::Button::new("開啟置物架")).clicked(){self.settings_open=false;self.shelf_tab=0;self.show_page(3);}
                        });
                        settings_label(ui,"剪貼簿");
                        settings_card(ui,|ui|{
                            setting_row(ui,"記錄剪貼簿","在置物架的「剪貼簿」裡檢視最近複製的文字，點一下再次複製",&mut self.settings.clipboard_history);
                            ui.label(egui::RichText::new(format!("本機儲存最近 {} 條，重新開啟後可繼續使用。關閉記錄會暫停收集，手動清空會保留固定條目。密碼管理器標記為敏感的內容不會記錄。",crate::clipboard::MAX_RECENT)).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            ui.add_space(5.0);
                            if ui.add_sized([120.0,32.0],egui::Button::new("開啟剪貼簿")).clicked(){self.settings_open=false;self.shelf_tab=1;self.show_page(3);}
                        });
                    }
                    6=>{
                        settings_label(ui,"系統提示");
                        settings_card(ui,|ui|{
                            setting_row(ui,"系統狀態","音量、充電、網路和鍵盤狀態",&mut self.settings.system_hud);
                            ui.separator();
                            setting_row(ui,"藍芽通知","裝置連線與斷開提醒；關閉後停止藍芽監聽",&mut self.settings.bluetooth_notifications);
                            ui.separator();
                            setting_row(ui,"提醒總開關","關閉後飲水、專注、待辦及聊天軟體提醒都不顯示",&mut self.settings.notifications);
                        });
                        // 總開關關閉時，微信與 QQ 兩欄置灰並說明原因，避免「開了也不提醒」。
                        let master=self.settings.notifications;
                        let master_hint=|ui:&mut egui::Ui|if !master {
                            ui.label(egui::RichText::new("提醒總開關已關閉（上方「系統提示」），開啟後這裡才會生效").size(12.0).color(Color32::from_rgb(230,150,40)));
                        };
                        // 舊版中國 IM 的提醒只在 cn-messengers 功能開啟時顯示；預設只有 Discord。
                        if cfg!(feature="cn-messengers") {
                        settings_label(ui,"微信提醒");
                        settings_card(ui,|ui|{
                            master_hint(ui);
                            ui.add_enabled_ui(master,|ui|{
                            setting_row(ui,"微信新訊息提醒","根據工作列閃動提醒，不讀取聯絡人或聊天正文",&mut self.settings.wechat_notifications);
                            if self.settings.wechat_notifications {
                                ui.separator();
                                setting_row(ui,"系統匣一鍵喚起微信","微信縮在系統匣時，點提醒由系統按下微信系統匣圖示；圖示收在摺疊區時系統匣選單會短暫彈出",&mut self.settings.wechat_tray_open);
                            }
                            if master && !self.messenger_status(crate::app::Messenger::Wechat).is_empty(){ui.label(egui::RichText::new(self.messenger_status(crate::app::Messenger::Wechat)).size(12.0));}
                            });
                        });
                        settings_label(ui,"辦公軟體提醒");
                        settings_card(ui,|ui|{
                            master_hint(ui);
                            ui.add_enabled_ui(master,|ui|{
                                setting_row(ui,"企業微信","根據工作列閃動提醒",&mut self.settings.wecom_notifications);
                                setting_row(ui,"飛書","根據工作列閃動提醒",&mut self.settings.feishu_notifications);
                                setting_row(ui,"釘釘","根據工作列閃動提醒",&mut self.settings.dingtalk_notifications);
                                ui.label(egui::RichText::new("需在客戶端開啟工作列閃動，不讀取聯絡人或聊天正文。點選提醒開啟對應視窗；帳號無法確認時可手動關閉提醒。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                                for client in crate::messenger::Messenger::OFFICE {
                                    if client.enabled(&self.settings) && !self.messenger_status(client).is_empty(){ui.label(egui::RichText::new(self.messenger_status(client)).size(12.0));}
                                }
                            });
                        });
                        }
                        settings_label(ui,"通訊軟體與 Windows 通知");
                        settings_card(ui,|ui|{
                            master_hint(ui);
                            ui.add_enabled_ui(master,|ui|{
                            setting_row(ui,"LINE 新訊息提醒","同時監聽 LINE 工作列閃動與 Windows 通知；收到訊息時在島上提醒，點選喚起 LINE 主視窗",&mut self.settings.line_notifications);
                            if master && self.settings.line_notifications && !self.messenger_status(crate::app::Messenger::Line).is_empty(){
                                ui.label(egui::RichText::new(self.messenger_status(crate::app::Messenger::Line)).size(12.0));
                            }
                            ui.separator();
                            ui.label(egui::RichText::new(&self.listener_status).size(13.0));
                            if self.settings.read_notifications {
                                if ui.button("斷開系統通知").clicked(){self.pending.push(Cmd::DisconnectNotifications);}
                            }else{
                                ui.horizontal_wrapped(|ui|{
                                    if ui.button("連線系統通知").clicked(){self.pending.push(Cmd::RequestNotifications);}
                                });
                            }
                            ui.label(egui::RichText::new("Discord 與 LINE 系統通知需開啟「連線系統通知」與桌面通知；新通知在島上顯示約 4.5 秒，預設隱藏正文。點選提醒可喚起對應主視窗。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            ui.separator();
                            setting_row(ui,"顯示通知正文","可能包含私密訊息；螢幕共享時建議關閉",&mut self.settings.notification_preview);
                            ui.label(egui::RichText::new("只接收 Windows 通知中心可見的內容；應用會在收到通知後出現在下方。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            let before_apps=self.notification_apps.items.clone();
                            for (_,name,enabled) in &mut self.notification_apps.items {setting_row(ui,name,"",enabled);}
                            if before_apps!=self.notification_apps.items{self.notification_apps.save();}
                            });
                        });
                    }
                    7=>{
                        settings_label(ui,"郵件提醒");
                        settings_card(ui,|ui|{
                            setting_row(ui,"新郵件提醒","新郵件到達時顯示提醒；島上歌詞模式顯示右側圖示",&mut self.settings.mail_enabled);
                            ui.separator();
                            let mut filter_changed=false;
                            settings_control_row_right(ui,"新郵件分類",|ui|{
                                filter_changed=settings_segmented(ui,&mut self.settings.mail_filter,&[(0,"重要與主要"),(1,"僅重要"),(2,"全部")],72.0);
                            });
                            if filter_changed {
                                self.mail.manage.data.clear();
                                self.mail.manage.next_refresh=0.0;
                                self.pending.push(Cmd::MailSync);
                            }
                            ui.label(egui::RichText::new("依 Gmail 內建分類自動排除廣告（促銷）、社交與論壇信件，僅彈出重要／主要信件、驗證碼與會議邀請。").size(11.5).color(settings_color(ctx,Color32::from_gray(195),Color32::from_gray(60))));
                            ui.separator();
                            let sync_before=self.settings.mail_sync_operations;
                            ui.add_enabled_ui(self.mail.manage.pending.is_empty(),|ui|{
                                setting_row(ui,"同步郵件操作到信箱","開啟後同步已讀、刪除和恢復；關閉時僅整理島仔中的記錄",&mut self.settings.mail_sync_operations);
                            });
                            if sync_before!=self.settings.mail_sync_operations{self.mail_manage_mode_changed();}
                            ui.label(egui::RichText::new("切換開關不會補交之前的本機操作。").size(11.5).color(settings_color(ctx,Color32::from_gray(195),Color32::from_gray(60))));
                            ui.separator();
                            setting_row(ui,"驗證碼高亮","識別到驗證碼時在島上放大顯示，點選即複製",&mut self.settings.mail_code);
                            ui.separator();
                            setting_choices(ui,"驗證碼隱藏",&mut self.settings.mail_code_hide_seconds,[30,60,300],"秒");
                            ui.label("預設 60 秒。明確有效時長更短時提前隱藏；歷史郵件預設隱藏。隱藏不代表失效，主動檢視後 15 秒再次隱藏。");
                            setting_choices(ui,"檢查間隔",&mut self.settings.mail_interval,[60,120,300],"秒");
                            ui.label(egui::RichText::new("支援推送時實時收信，並每 10 秒補查；以上間隔僅用於不支援推送的信箱。").size(11.5).color(settings_color(ctx,Color32::from_gray(195),Color32::from_gray(60))));
                            ui.separator();
                            setting_choices(ui,"會議提前",&mut self.settings.mail_ics,[5,15,30],"分鐘");
                            ui.label(egui::RichText::new("根據郵件中的日曆邀請提醒，與日程設定共用。").size(11.5).color(settings_color(ctx,Color32::from_gray(195),Color32::from_gray(60))));
                        });
                        settings_label(ui,"已新增的信箱");
                        settings_card(ui,|ui|{
                            if self.mail.accounts.is_empty(){ui.label(egui::RichText::new("還沒有信箱。在下面新增，儲存後自動連線。").size(12.0).color(settings_color(ctx,Color32::from_gray(195),Color32::from_gray(60))));}
                            let total=self.mail.accounts.len();
                            let(mut changed,mut remove,mut test)=(false,None,None);
                            for i in 0..total{
                                let(email,provider)=(self.mail.accounts[i].email.clone(),self.mail.accounts[i].provider);
                                let status=self.mail.status_of(&email).label();
                                let mut on=self.mail.accounts[i].enabled;
                                if i>0{ui.separator();}
                                ui.label(egui::RichText::new(&email).size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                                ui.label(egui::RichText::new(format!("{} · {}",provider.label(),status)).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                                ui.horizontal(|ui|{
                                    if ui.add_sized([66.0,30.0],egui::Button::new("測試")).clicked(){test=Some(i);}
                                    if ui.add_sized([66.0,30.0],egui::Button::new("刪除")).clicked(){remove=Some(i);}
                                    ui.add_space(8.0);
                                    ui.label(egui::RichText::new("啟用").size(12.0).color(settings_color(ctx,Color32::from_gray(190),Color32::from_gray(65))));
                                    if switch(ui,&mut on){self.mail.accounts[i].enabled=on;changed=true;}
                                });
                            }
                            if changed{self.mail_accounts_changed();}
                            if let Some(i)=remove{
                                let account=self.mail.accounts.remove(i);
                                let email=account.email.clone();
                                self.mail.status.remove(&email.to_ascii_lowercase());
                                if account.provider==crate::mail::Provider::GoogleWorkspace{
                                    self.mail.note=format!("{email}：已移除（本機 gws CLI 登入狀態未變更）");
                                }else{
                                    let credential=account.credential_key();
                                    crate::mail::secrets::delete(&credential);
                                    self.mail.note=format!("{email}：已移除，憑據已從系統憑據管理器刪除");
                                }
                                self.mail_accounts_changed();
                            }
                            if let Some(i)=test{
                                let account=self.mail.accounts[i].clone();
                                if let Err(error)=account.endpoint(){
                                    self.mail.note=error;
                                }else if account.provider==crate::mail::Provider::GoogleWorkspace{
                                    self.mail.testing=Some(account.email.clone());self.mail.form_test=None;self.mail.note=format!("{}：正在測試連線…",account.email);self.pending.push(Cmd::MailTest(account,String::new()));
                                }else{
                                match crate::mail::secrets::load(&account.credential_key()){
                                    Ok(pass)=>{self.mail.testing=Some(account.email.clone());self.mail.form_test=None;self.mail.note=format!("{}：正在測試連線…",account.email);self.pending.push(Cmd::MailTest(account,pass));}
                                    Err(e)=>self.mail.note=format!("{}：{e}",account.email),
                                }
                                }
                            }
                        });
                        settings_label(ui,"新增信箱");
                        settings_card(ui,|ui|{
                            settings_control_row(ui,"信箱服務",56.0,|ui|{
                                egui::ComboBox::from_id_salt("mail-provider").width(160.0).selected_text(self.mail.form.provider.label()).show_ui(ui,|ui|{
                                    for p in crate::mail::Provider::ALL{if ui.selectable_label(self.mail.form.provider==p,p.label()).clicked(){self.mail.form.provider=p;}}
                                }).response
                            });
                            ui.add_space(5.0);
                            let microsoft=self.mail.form.provider==crate::mail::Provider::Microsoft;
                            ui.label(egui::RichText::new(if microsoft{"連線微軟信箱"}else{"獲取授權碼或專用密碼"}).size(12.5).strong());
                            ui.add(egui::Label::new(egui::RichText::new(self.mail.form.provider.hint()).size(12.0).color(settings_color(ctx,Color32::from_gray(200),Color32::from_gray(55)))).wrap());
                            let (label,url)=self.mail.form.provider.setup_link();
                            if !url.is_empty() && ui.link(label).clicked(){self.pending.push(Cmd::OpenUrl(url.into()));}
                            ui.add_space(8.0);
                            if self.mail.form.provider==crate::mail::Provider::Custom {
                                settings_control_row(ui,"IMAP 主機",80.0,|ui|{ui.text_edit_singleline(&mut self.mail.form.custom.host)});
                                settings_control_row(ui,"TLS 埠",80.0,|ui|{ui.add(egui::DragValue::new(&mut self.mail.form.custom.port).range(1..=65535))});
                                settings_control_row(ui,"登入使用者名稱",80.0,|ui|{ui.add(egui::TextEdit::singleline(&mut self.mail.form.custom.username).vertical_align(egui::Align::Center).hint_text("留空使用信箱地址"))});
                            }
                            if !microsoft{
                            let input_width=(ui.available_width()-56.0-48.0-2.0*ui.spacing().item_spacing.x).max(100.0);
                            settings_control_row(ui,"信箱",56.0,|ui|{
                                ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.mail.form.email).vertical_align(egui::Align::Center).hint_text("輸入信箱地址"))
                            });
                            let target=crate::mail::Account{custom:(self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()),email:self.mail.form.email.clone(),provider:self.mail.form.provider,enabled:true};
                            let unsupported=target.endpoint().err().as_deref()==Some(crate::mail::MICROSOFT_UNSUPPORTED);
                            if unsupported{ui.add(egui::Label::new(crate::mail::MICROSOFT_UNSUPPORTED).wrap());}
                            ui.add_enabled_ui(!unsupported,|ui|{
                            settings_control_row(ui,"授權碼",56.0,|ui|{
                                let field=ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.mail.form.password).vertical_align(egui::Align::Center).password(!self.mail.form.show).hint_text("輸入信箱授權碼"));
                                if ui.add_sized([48.0,30.0],egui::Button::new(if self.mail.form.show{"隱藏"}else{"顯示"})).clicked(){self.mail.form.show=!self.mail.form.show;}
                                field
                            });
                            ui.horizontal(|ui|{
                                if ui.button("測試連線").clicked(){
                                    let email=self.mail.form.email.trim().to_string();
                                    let target=crate::mail::Account{custom:(self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()),email:email.clone(),provider:self.mail.form.provider,enabled:true};
                                    if !email.contains('@'){self.mail.note="請先填信箱地址".into();}
                                    else if let Err(error)=target.endpoint(){self.mail.note=error;}
                                    else if self.mail.form.password.is_empty(){self.mail.note="請先填授權碼（不是信箱登入密碼）".into();}
                                    else{
                                        // 測試成功就直接儲存並開始監控（和日曆的「測試並儲存」一致），用測試當下的帳號與授權碼。
                                        let password=self.mail.form.password.clone();
                                        self.mail.testing=Some(email.clone());
                                        self.mail.form_test=Some((target.clone(),password.clone()));
                                        self.mail.note=format!("{email}：正在測試連線…");
                                        self.pending.push(Cmd::MailTest(target,password));
                                    }
                                }
                                if ui.button("儲存並開始監控").clicked(){
                                    let email=self.mail.form.email.trim().to_string();
                                    let target=crate::mail::Account{custom:(self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()),email:email.clone(),provider:self.mail.form.provider,enabled:true};
                                    if !email.contains('@'){self.mail.note="請先填信箱地址".into();}
                                    else if let Err(error)=target.endpoint(){self.mail.note=error;}
                                    else if self.mail.form.password.is_empty(){self.mail.note="請先填授權碼（不是信箱登入密碼）".into();}
                                    else{
                                        let password=self.mail.form.password.clone();
                                        match self.save_mail_account(target,&password){
                                            Ok(())=>{
                                                self.mail.form.password.clear();
                                                self.mail.note=format!("{email}：已儲存，正在連線…");
                                            }
                                            Err(e)=>self.mail.note=e,
                                        }
                                    }
                                }
                            });
                            ui.label(egui::RichText::new("授權碼儲存在 Windows 憑據管理器中。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            });
                            }
                            ui.label(egui::RichText::new("圖片中的驗證碼請在信箱檢視。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            if !self.mail.note.is_empty(){ui.add_space(3.0);ui.add(egui::Label::new(egui::RichText::new(&self.mail.note).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75)))).wrap());}
                        });
                        self.gws_settings_card(ui);
                    }
                    8=>{
                        ui.spacing_mut().item_spacing.y=4.0;
                        // v0.5.6 日程：狀態在最上面，後面才是平臺、賬號和提醒。
                        // 只讀；伺服器地址由平臺決定，使用者不用抄。
                        // 已連線的日曆帳號：每個帳號一行，點名稱切換到它的設定；「斷開」只影響這一行。
                        if !self.calendar.feeds.is_empty(){
                            settings_label(ui,"已連線的日曆");
                            let rows:Vec<(crate::calendar::Config,bool,bool)>=self.calendar.feeds.iter()
                                .map(|feed|(feed.config.clone(),feed.connected,!feed.error.is_empty())).collect();
                            settings_card(ui,|ui|{
                                for (cfg,connected,failed) in &rows{
                                    let selected=self.calendar.config.as_ref()==Some(cfg);
                                    ui.horizontal(|ui|{
                                        if ui.selectable_label(selected,egui::RichText::new(cfg.display_name()).size(13.0)).clicked(){self.select_calendar_account(cfg.clone());}
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                            if ui.add_enabled(!self.calendar.testing,egui::Button::new("斷開").small()).clicked(){self.select_calendar_account(cfg.clone());self.pending.push(Cmd::CalendarForget);}
                                            let (text,ok)=if *connected{("● 已連線",true)}else if *failed{("● 連線失敗",false)}else{("● 正在連線…",false)};
                                            ui.label(egui::RichText::new(text).size(12.0).color(if ok{settings_color(ctx,Color32::from_rgb(120,220,160),Color32::from_rgb(24,121,64))}else{settings_color(ctx,Color32::from_rgb(240,180,110),Color32::from_rgb(145,89,12))}));
                                        });
                                    });
                                }
                            });
                        }
                        if let Some(cfg)=self.calendar.config.clone(){
                            settings_card(ui,|ui|{
                                let n=self.calendar.events.len();
                                ui.horizontal(|ui|{
                                    ui.label(egui::RichText::new(if self.calendar.connected{format!("● 已連線 · 未來 24 小時 {n} 個日程")}else if self.calendar.failures==0{"● 正在連線…".into()}else{format!("● 上次連線失敗 · 已知 {n} 個日程")}).size(13.0).strong().color(if self.calendar.connected{settings_color(ctx,Color32::from_rgb(120,220,160),Color32::from_rgb(24,121,64))}else{settings_color(ctx,Color32::from_rgb(240,180,110),Color32::from_rgb(145,89,12))}));
                                });
                                ui.horizontal(|ui|{
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                        if ui.add_enabled(!self.calendar.testing,egui::Button::new("斷開").small()).clicked(){self.pending.push(Cmd::CalendarForget);}
                                        if ui.small_button("立即同步").clicked(){self.calendar.note="正在同步…".into();self.calendar.note_error=false;self.pending.push(Cmd::CalendarSync);}
                                    });
                                });
                                ui.label(egui::RichText::new(format!("{} · 最後同步 {}",cfg.provider.label(),if self.calendar.last_sync.is_empty(){"—".to_string()}else{self.calendar.last_sync.clone()})).size(11.0).color(settings_color(ctx,Color32::from_gray(150),Color32::from_gray(105))));
                                if let Some(feed)=self.calendar.feeds.iter().find(|feed|feed.config==cfg) {
                                    let status=if feed.month.loading {"正在同步本月日程…".to_owned()}
                                        else if !feed.month.error.is_empty(){format!("本月同步失敗：{}",feed.month.error)}
                                        else if feed.month.loaded==Some(self.calendar.view.month){format!("{} {} {} 個日程",self.calendar.view.month.format("%Y-%m"),if feed.month.warning.is_empty(){"已同步"}else{"已讀取"},feed.month.events.len())}
                                        else {String::new()};
                                    if !status.is_empty(){ui.add(egui::Label::new(egui::RichText::new(status).size(12.0)).wrap());}
                                    if !feed.month.warning.is_empty(){ui.add(egui::Label::new(egui::RichText::new(format!("本月部分日程未同步：{}",feed.month.warning)).size(12.0).color(settings_color(ctx,Color32::from_rgb(238,181,91),Color32::from_rgb(139,88,12)))).wrap());}
                                }
                                // 帳號下有發現到日曆時，預設全部同步，讓使用者自己取消勾選不要的。
                                let choices=self.calendar.feeds.iter().find(|feed|feed.config==cfg).filter(|feed|!feed.calendars.is_empty())
                                    .map(|feed|(feed.calendars.clone(),feed.skipped.clone()));
                                if let Some((calendars,skipped))=choices{
                                    let muted=settings_color(ctx,Color32::from_gray(150),Color32::from_gray(105));
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new(format!("要同步的日曆（已選 {} / {}）",calendars.iter().filter(|info|!skipped.contains(&info.href)).count(),calendars.len())).size(13.0).strong());
                                    ui.label(egui::RichText::new("預設匯入帳號下所有日曆；取消勾選的日曆不會出現在日程裡。").size(11.0).color(muted));
                                    let mut change=None;
                                    for info in &calendars{
                                        let mut on=!skipped.contains(&info.href);
                                        let title:String=info.title().chars().take(40).collect();
                                        if ui.checkbox(&mut on,egui::RichText::new(title).size(12.5)).changed(){change=Some((info.href.clone(),on));}
                                    }
                                    if calendars.iter().all(|info|skipped.contains(&info.href)){
                                        ui.label(egui::RichText::new("目前沒有選取任何日曆，這個帳號不會同步日程。").size(11.5).color(settings_color(ctx,Color32::from_rgb(238,181,91),Color32::from_rgb(139,88,12))));
                                    }
                                    if !skipped.is_empty() && ui.small_button("全部選取").clicked(){self.set_calendar_choice(&cfg,None,true);}
                                    if let Some((href,on))=change{self.set_calendar_choice(&cfg,Some(&href),on);}
                                    ui.add_space(2.0);
                                }
                                setting_row(ui,"自動同步","每 5 分鐘更新日程；關閉後可點「立即同步」",&mut self.settings.calendar_auto_sync);
                                if !self.calendar.warning.is_empty(){ui.add(egui::Label::new(egui::RichText::new(format!("同步提示：{}",self.calendar.warning)).size(12.0).color(settings_color(ctx,Color32::from_rgb(238,181,91),Color32::from_rgb(139,88,12)))).wrap());}
                                if !self.calendar.error.is_empty(){
                                    ui.add(egui::Label::new(egui::RichText::new(&self.calendar.error).size(12.0).color(settings_color(ctx,Color32::from_rgb(240,96,96),Color32::from_rgb(184,34,34)))).wrap());
                                }
                            });
                        }
                        if ui.add_sized([ui.available_width(),34.0],egui::Button::new("開啟日程")).clicked(){self.open_calendar(None);}
                        settings_label(ui,"新增日曆平臺");
                        settings_card(ui,|ui|{
                            egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(29,29,31),Color32::from_rgb(236,239,244))).corner_radius(8.0).inner_margin(3).show(ui,|ui|{
                                ui.set_min_width(ui.available_width());
                                ui.spacing_mut().item_spacing.x=2.0;
                                // 六個平臺分兩行：飛書 / 釘釘 / 企業微信，iCloud / CalDAV / Exchange。
                                let width=((ui.available_width()-4.0)/3.0).max(80.0);
                                for row in crate::calendar::Provider::ALL.chunks(3){
                                    ui.horizontal(|ui|{
                                        for &p in row{
                                            let selected=self.calendar.form.provider==p;
                                            let button=egui::Button::new(egui::RichText::new(p.label()).size(13.0).color(if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(190),Color32::from_gray(65))}))
                                                .fill(if selected{settings_color(ctx,Color32::from_rgb(81,81,85),Color32::from_rgb(208,217,231))}else{Color32::TRANSPARENT})
                                                .stroke(egui::Stroke::NONE).corner_radius(6.0);
                                            let button=ui.add_enabled_ui(!self.calendar.testing,|ui|ui.add_sized([width,30.0],button)).inner;
                                            let button=match p{
                                                crate::calendar::Provider::Google=>button.on_hover_text("Google 日曆：支援貼上 .ics 私密網址或 CalDAV 同步"),
                                                crate::calendar::Provider::ICloud=>button.on_hover_text("Apple 日曆：Apple ID + App 專用密碼"),
                                                crate::calendar::Provider::Custom=>button.on_hover_text("CalDAV / iCal 日曆：Nextcloud、群暉、Fastmail 或 .ics 網址"),
                                                crate::calendar::Provider::Exchange=>button.on_hover_text("公司自建或第三方 Exchange 日曆（ActiveSync），唯讀同步"),
                                                _=>button,
                                            };
                                            if button.clicked(){
                                                self.select_calendar_provider(p);
                                            }
                                        }
                                    });
                                }
                            });
                            ui.add_space(7.0);
                            ui.horizontal_top(|ui|{
                                ui.allocate_ui_with_layout(egui::vec2(50.0,18.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_size(egui::vec2(50.0,18.0));ui.label(egui::RichText::new("伺服器").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));});
                                if self.calendar.form.provider.custom_server(){
                                    ui.add_sized([ui.available_width(),22.0],egui::TextEdit::singleline(&mut self.calendar.form.server).vertical_align(egui::Align::Center)
                                        .hint_text(egui::RichText::new(self.calendar.form.provider.server_hint()).color(settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)))));
                                }else{
                                    ui.add(egui::Label::new(egui::RichText::new(self.calendar.form.provider.server()).size(12.0).color(settings_color(ctx,Color32::from_gray(205),Color32::from_gray(55)))).wrap());
                                }
                            });
                        });
                        settings_label(ui,"連線帳號");
                        settings_card(ui,|ui|{
                            let form_is_ics=crate::calendar::is_ics_url(&self.calendar.form.server) || crate::calendar::is_ics_url(&self.calendar.form.username);
                            ui.scope(|ui|{
                                ui.visuals_mut().extreme_bg_color=settings_color(ctx,Color32::from_rgb(25,25,28),Color32::from_rgb(244,246,249));
                            let input_width=(ui.available_width()-50.0-48.0-2.0*ui.spacing().item_spacing.x).max(80.0);
                                ui.horizontal(|ui|{
                                    ui.allocate_ui_with_layout(egui::vec2(50.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_size(egui::vec2(50.0,30.0));ui.label(egui::RichText::new("使用者名稱").size(13.0).color(settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))));});
                                    let user_hint=if form_is_ics{"選填日曆名稱（留空自動命名）"}else{"輸入使用者名稱或貼上帳號訊息"};
                                    let user=ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.calendar.form.username).vertical_align(egui::Align::Center)
                                        .hint_text(egui::RichText::new(user_hint).color(settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)))));
                                    if user.changed(){
                                        let pasted=self.calendar.form.username.clone();
                                        if let Some((u,p))=crate::calendar::parse_paste(&pasted){
                                            self.calendar.form.username=u;
                                            self.calendar.form.password=p;
                                            self.calendar.note="已從貼上內容中填入使用者名稱和密碼".into();
                                            self.calendar.note_error=false;
                                        }
                                        self.refresh_calendar_shared();
                                    }
                                });
                                ui.add_space(5.0);
                                ui.horizontal(|ui|{
                                    ui.allocate_ui_with_layout(egui::vec2(50.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_size(egui::vec2(50.0,30.0));ui.label(egui::RichText::new("密碼").size(13.0).color(settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))));});
                                    let pass_hint=if form_is_ics{"訂閱 .ics 網址免填密碼"}else if self.calendar.form.mail_shared{"留空即使用信箱的密碼"}else{"輸入平臺生成的密碼"};
                                    ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.calendar.form.password).vertical_align(egui::Align::Center)
                                        .password(!self.calendar.form.show).hint_text(egui::RichText::new(pass_hint).color(settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)))));
                                    if ui.add_sized([48.0,30.0],egui::Button::new(if self.calendar.form.show{"隱藏"}else{"顯示"})).clicked(){self.calendar.form.show=!self.calendar.form.show;}
                                });
                                // 密碼從網頁或文件貼上時常帶換行，這裡直接去掉，不讓它進到連線與儲存。
                                if self.calendar.form.password.contains(['\r','\n']){self.calendar.form.password.retain(|c|c!='\r'&&c!='\n');}
                                if self.calendar.form.mail_shared && !form_is_ics{
                                    ui.add_space(3.0);
                                    ui.label(egui::RichText::new("信箱已儲存這個帳號的密碼，留空即可直接連線，不必重新輸入。").size(11.5).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                                }
                            });
                            ui.add_space(7.0);
                            let guide=if self.calendar.form.guide_open{"收起獲取方法"}else{"如何獲取使用者名稱和密碼？"};
                            if ui.link(egui::RichText::new(guide).color(settings_color(ctx,Color32::from_rgb(120,185,245),Color32::from_rgb(26,100,177)))).clicked(){self.calendar.form.guide_open=!self.calendar.form.guide_open;}
                            if self.calendar.form.guide_open{
                                ui.add_space(4.0);
                                for(i,step)in self.calendar.form.provider.guide().iter().enumerate(){
                                    ui.label(egui::RichText::new(format!("{}. {}",i+1,step)).size(12.0).color(settings_color(ctx,Color32::from_gray(200),Color32::from_gray(55))));
                                }
                            }
                            ui.add_space(9.0);
                            let busy=self.calendar.testing;
                            let label=if busy{"正在測試連線…"}else{"測試並儲存"};
                            let btn=egui::Button::new(egui::RichText::new(label).size(13.0).strong().color(Color32::WHITE))
                                .fill(if busy{settings_color(ctx,Color32::from_rgb(70,70,76),Color32::from_rgb(213,219,227))}else{Color32::from_rgb(45,105,165)})
                                .corner_radius(7.0);
                            if ui.add_enabled_ui(!busy,|ui|ui.add_sized([130.0,32.0],btn)).inner.clicked(){
                                let user=self.calendar.form.username.trim().to_string();
                                let custom=self.calendar.form.provider.custom_server();
                                let server_raw=self.calendar.form.server.trim();
                                let server=crate::calendar::normalize_server(server_raw);
                                let is_ics=crate::calendar::is_ics_url(server_raw) || crate::calendar::is_ics_url(&user);
                                if is_ics {
                                    let ics_url=if crate::calendar::is_ics_url(server_raw){server}else{crate::calendar::normalize_server(&user)};
                                    if let Some(url)=ics_url {
                                        let name=if user.is_empty() || crate::calendar::is_ics_url(&user) {
                                            crate::calendar::ics_default_name(&url)
                                        } else {
                                            user
                                        };
                                        let cfg=crate::calendar::Config{provider:self.calendar.form.provider,username:name,href:None,server:Some(url)};
                                        self.calendar.testing=true;
                                        self.calendar.test_password.clear();
                                        self.calendar.note="正在連線日曆…".into();
                                        self.calendar.note_error=false;
                                        self.pending.push(Cmd::CalendarTest(cfg,String::new()));
                                    } else {
                                        self.calendar.note="請填寫以 https:// 或 webcal:// 開頭的 .ics 日曆網址".into();
                                        self.calendar.note_error=true;
                                    }
                                } else {
                                    let typed=crate::mail::clean_secret(&self.calendar.form.password);
                                    let shared=crate::calendar::secrets::shared_mail_password(&crate::calendar::Config{provider:self.calendar.form.provider,username:user.clone(),href:None,server:None});
                                    let google=self.calendar.form.provider==crate::calendar::Provider::Google;
                                    let bad_server=if google { !server_raw.is_empty() && server.is_none() } else { custom && server.is_none() };
                                    if bad_server {
                                        self.calendar.note=if google {
                                            "請填寫以 https:// 或 webcal:// 開頭的日曆網址，或留空使用 CalDAV".into()
                                        } else {
                                            "請填寫以 https:// 開頭的 CalDAV 伺服器地址".into()
                                        };
                                        self.calendar.note_error=true;
                                    } else if user.is_empty() {
                                        self.calendar.note="請先填使用者名稱".into();
                                        self.calendar.note_error=true;
                                    } else if typed.is_empty() && shared.is_none() {
                                        self.calendar.note="請先填密碼".into();
                                        self.calendar.note_error=true;
                                    } else {
                                        let password=if typed.is_empty(){shared.unwrap_or_default()}else{typed};
                                        let cfg=crate::calendar::Config{provider:self.calendar.form.provider,username:user,href:None,server:if custom{server}else{None}};
                                        self.calendar.testing=true;
                                        self.calendar.test_password=password.clone();
                                        self.calendar.note="正在連線日曆…".into();
                                        self.calendar.note_error=false;
                                        self.pending.push(Cmd::CalendarTest(cfg,password));
                                    }
                                }

                            }
                            ui.add_space(5.0);
                            ui.label(egui::RichText::new("連線成功後儲存，密碼儲存在 Windows 憑據管理器中。").size(11.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            if !self.calendar.note.is_empty(){
                                ui.add_space(5.0);
                                let color=if self.calendar.note_error{settings_color(ctx,Color32::from_rgb(255,125,125),Color32::from_rgb(184,34,34))}else{settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))};
                                ui.add(egui::Label::new(egui::RichText::new(&self.calendar.note).size(12.0).color(color)).wrap());
                            }
                        });
                        settings_label(ui,"匯入日曆檔案");
                        settings_card(ui,|ui|{
                            ui.add(egui::Label::new(egui::RichText::new("把 .ics 檔案裡的日程匯入到本機日程，例如蘋果「日曆」App 裡「檔案 → 匯出」得到的檔案。重複日程會按未來一年展開；再次匯入同一檔案不會重複新增。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75)))).wrap());
                            ui.add_space(5.0);
                            if ui.add_sized([150.0,30.0],egui::Button::new("選擇 .ics 檔案…")).clicked(){self.pending.push(Cmd::CalendarImport);}
                            if !self.calendar.import_note.is_empty(){
                                ui.add_space(3.0);
                                ui.add(egui::Label::new(egui::RichText::new(&self.calendar.import_note).size(12.0).color(settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55)))).wrap());
                            }
                        });
                        settings_label(ui,"會議提醒");
                        settings_card(ui,|ui|{
                            ui.horizontal(|ui|{
                                ui.allocate_ui_with_layout(egui::vec2(50.0,32.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_size(egui::vec2(50.0,32.0));ui.label(egui::RichText::new("提前").size(13.0));});
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                    for minutes in [30,15,5]{let selected=self.settings.mail_ics==minutes;
                                        if ui.add_sized([62.0,32.0],egui::Button::new(format!("{minutes} 分鐘")).selected(selected)).clicked(){self.settings.mail_ics=minutes;}
                                    }
                                });
                            });
                            ui.add_space(5.0);
                            ui.label(egui::RichText::new("與郵件中的會議邀請共用提醒，同一場會議只提醒一次。").size(11.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                        });
                        self.gws_settings_card(ui);
                    }
                    _=>{
                        settings_label(ui,"島仔 Daozai");
                        settings_card(ui,|ui|{
                            ui.horizontal(|ui|{
                                ui.label(egui::RichText::new(format!("版本 {}",env!("CARGO_PKG_VERSION"))).size(13.0));
                                ui.add_space(8.0);
                                ui.vertical(|ui|self.updates.ui(ui));
                            });
                            ui.label(egui::RichText::new("桌面頂部的音樂、專注、提醒與日程小助手。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            ui.horizontal_wrapped(|ui|{
                                if ui.button("開啟本地資料目錄").clicked(){self.pending.push(Cmd::Open(crate::model::config_dir()));}
                                if ui.button("收集診斷").clicked(){
                                    // 另起 `daozai.exe --collect-diagnostics`：在新視窗裡收集，診斷包儲存到桌面並自動選中。
                                    if crate::diagnostics_tool::launch().is_err(){self.say("無法啟動診斷收集，請從開始選單開啟「收集島仔診斷」");}
                                }
                                ui.label(egui::RichText::new("裝置圖示：Solar Icons · 480 Design").size(11.0).color(settings_color(ctx,Color32::from_gray(150),Color32::from_gray(105))));
                                ui.hyperlink_to("CC BY 4.0", "https://creativecommons.org/licenses/by/4.0/");
                            });
                        });
                    }
                    }
                });
            });
            self.avoidance.dialog(ctx,&mut self.settings);
            if before!=format!("{:?}",self.settings){self.settings.save();}
            ctx.set_style(original_style);
        });
        if !self.settings_open{self.window_layout().close();self.weather_source.close();}
    }

    fn gws_settings_card(&mut self, ui: &mut egui::Ui) {
        use crate::gws::Phase;
        let ctx = &ui.ctx().clone();
        settings_label(ui, "Google Workspace CLI（免應用程式密碼）");
        settings_card(ui, |ui| {
            let busy = matches!(self.gws.phase, Phase::Checking | Phase::LoggingIn | Phase::Installing);
            let (status_text, status_color) = match &self.gws.phase {
                Phase::Idle => (
                    "尚未檢查本機 gws 登入狀態".to_string(),
                    settings_color(ctx, Color32::from_gray(180), Color32::from_gray(75)),
                ),
                Phase::Checking => (
                    "● 正在檢查 gws CLI 登入狀態…".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 180, 110), Color32::from_rgb(145, 89, 12)),
                ),
                Phase::LoggingIn => (
                    "● 正在開啟瀏覽器完成 Google Workspace 授權…".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 180, 110), Color32::from_rgb(145, 89, 12)),
                ),
                Phase::Connected(st) => (
                    format!("● 已登入 {}", st.user),
                    settings_color(ctx, Color32::from_rgb(120, 220, 160), Color32::from_rgb(24, 121, 64)),
                ),
                Phase::NotLoggedIn => (
                    "● 尚未登入 gws CLI".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 180, 110), Color32::from_rgb(145, 89, 12)),
                ),
                Phase::NeedsSetup => (
                    "● 尚未設定 Google OAuth 用戶端".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 180, 110), Color32::from_rgb(145, 89, 12)),
                ),
                Phase::NeedsSetupFailed(_) => (
                    "● 設定 Google OAuth 用戶端失敗".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 96, 96), Color32::from_rgb(184, 34, 34)),
                ),
                Phase::NotInstalled => (
                    "● 本機未安裝 gws CLI".to_string(),
                    settings_color(ctx, Color32::from_gray(180), Color32::from_gray(75)),
                ),
                Phase::Installing => (
                    "● 正在下載並安裝 gws…".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 180, 110), Color32::from_rgb(145, 89, 12)),
                ),
                Phase::InstallFailed(_) => (
                    "● gws 安裝失敗".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 96, 96), Color32::from_rgb(184, 34, 34)),
                ),
                Phase::Failed(_) => (
                    "● 連線檢查失敗".to_string(),
                    settings_color(ctx, Color32::from_rgb(240, 96, 96), Color32::from_rgb(184, 34, 34)),
                ),
            };
            ui.label(egui::RichText::new(status_text).size(13.0).strong().color(status_color));
            ui.add(
                egui::Label::new(
                    egui::RichText::new("透過本機 `gws` OAuth 讀取 Gmail 與 Google 日曆，免填應用程式密碼。")
                        .size(12.0)
                        .color(settings_color(ctx, Color32::from_gray(180), Color32::from_gray(75))),
                )
                .wrap(),
            );
            if matches!(self.gws.phase, Phase::Installing) {
                let (done, total) = crate::gws_install::progress();
                let mb = |b: u64| b as f64 / 1_048_576.0;
                let (fraction, text) = match total {
                    Some(t) => (done as f32 / t as f32, format!("{:.1} / {:.1} MB（{:.0}%）", mb(done), mb(t), done as f64 * 100.0 / t as f64)),
                    None if done > 0 => (0.0, format!("已下載 {:.1} MB", mb(done))),
                    None => (0.0, "正在連線到 GitHub…".to_string()),
                };
                ui.add_space(4.0);
                let bar = egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).text(text);
                ui.add(if total.is_none() { bar.animate(true) } else { bar });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if matches!(self.gws.phase, Phase::NotInstalled | Phase::InstallFailed(_))
                    && ui.add_enabled(!busy, egui::Button::new("下載並安裝 gws")).clicked()
                {
                    self.gws.phase = Phase::Installing;
                    self.gws.note = "正在從官方 GitHub 發行頁下載 Google Workspace CLI（gws），請稍候…".into();
                    self.pending.push(Cmd::GwsInstall);
                }
                if ui.add_enabled(!busy, egui::Button::new("登入並綁定 Gmail + 日曆")).clicked() {
                    self.gws.phase = Phase::LoggingIn;
                    self.gws.note = "正在啟動 Google Workspace CLI 授權…".into();
                    self.pending.push(Cmd::GwsLogin);
                }
                if ui.add_enabled(!busy, egui::Button::new("重新檢查")).clicked() {
                    self.gws.phase = Phase::Checking;
                    self.gws.note = "正在檢查 Google Workspace CLI 登入狀態…".into();
                    self.pending.push(Cmd::GwsCheck);
                }
                if matches!(self.gws.phase, Phase::NeedsSetup | Phase::NeedsSetupFailed(_)) {
                    if ui.button("匯入 client_secret.json").clicked() {
                        self.gws.phase = Phase::Checking;
                        self.gws.note = "正在從「下載」資料夾匯入 client_secret.json…".into();
                        self.pending.push(Cmd::GwsImportSecret);
                    }
                    if ui.button("執行 gws auth setup").clicked() {
                        self.pending.push(Cmd::GwsSetup);
                    }
                }
            });
            if !self.gws.note.is_empty() {
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&self.gws.note)
                            .size(12.0)
                            .color(settings_color(ctx, Color32::from_gray(190), Color32::from_gray(70))),
                    )
                    .wrap(),
                );
            }
        });
    }
}
