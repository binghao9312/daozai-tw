use crate::{app::{App,WEATHER_PAGE},extras::{settings_card,settings_hint,settings_label},settings_theme::color,
    cwa_weather::{self,CwaClient,CwaError,AUTHKEY_URL},weather::Snapshot,weather_provider::{self,Config,Error,Location,Provider}};
use eframe::egui::{self,Color32,RichText};
use std::sync::mpsc::Receiver;

/// CWA 一小時更新一次；失敗後至少等 30 分鐘再自動重試，避免每一幀都發出請求。
const REFRESH_SECS:i64=3600;
const RETRY_SECS:i64=1800;

pub(crate) fn updated_at(at:i64)->String {
    if at<=0{return "未取得".into();}
    chrono::DateTime::from_timestamp(at,0).map(|d|d.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string()).unwrap_or_else(||"未取得".into())
}

/// 來源選項：地域圖標、來源名、短標籤；整張卡片都可點擊。
pub(crate) fn provider_option(ui:&mut egui::Ui,provider:Provider,selected:bool,width:f32)->egui::Response {
    let (rect,response)=ui.allocate_exact_size(egui::vec2(width,66.0),egui::Sense::click());
    let ctx=ui.ctx();let painter=ui.painter();
    let accent=color(ctx,Color32::from_rgb(111,174,255),Color32::from_rgb(48,113,211));
    let base=color(ctx,Color32::from_rgb(37,37,41),Color32::WHITE);
    let fill=if selected{crate::paint::mix(base,accent,0.10)}else if response.hovered(){color(ctx,Color32::from_rgb(45,45,50),Color32::from_rgb(246,248,252))}else{base};
    let border=if selected||response.has_focus(){accent}else{color(ctx,Color32::from_rgb(65,65,71),Color32::from_rgb(221,226,234))};
    painter.rect(rect,9.0,fill,egui::Stroke::new(if selected{1.3}else{1.0},border),egui::StrokeKind::Inside);
    let overseas=provider==Provider::OpenMeteo;
    let title=if overseas{"Open-Meteo"}else{"中央氣象署 (CWA)"};
    let ink=if overseas{accent}else{color(ctx,Color32::from_rgb(255,180,100),Color32::from_rgb(200,120,50))};
    let icon=egui::Rect::from_min_size(rect.min+egui::vec2(10.0,10.0),egui::vec2(24.0,24.0));
    painter.rect_filled(icon,6.0,crate::paint::mix(fill,ink,0.15));
    let center=icon.center();let stroke=egui::Stroke::new(1.2,ink);
    if overseas {
        painter.circle_stroke(center,7.0,stroke);
        painter.line_segment([center+egui::vec2(-7.0,0.0),center+egui::vec2(7.0,0.0)],stroke);
        painter.line_segment([center+egui::vec2(0.0,-7.0),center+egui::vec2(0.0,7.0)],stroke);
        for sign in [-1.0,1.0]{painter.add(egui::Shape::line(vec![center+egui::vec2(0.0,-7.0),center+egui::vec2(sign*3.0,-3.5),center+egui::vec2(sign*3.0,3.5),center+egui::vec2(0.0,7.0)],stroke));}
    }else {
        // CWA 圖示：太陽 + 雲 + 雨滴
        painter.circle_filled(center+egui::vec2(-5.0,-3.0),4.0,ink);
        painter.line_segment([center+egui::vec2(-5.0,-7.0),center+egui::vec2(-5.0,-9.0)],stroke);
        painter.line_segment([center+egui::vec2(-9.0,-3.0),center+egui::vec2(-11.0,-3.0)],stroke);
        painter.line_segment([center+egui::vec2(-2.0,-6.0),center+egui::vec2(-0.5,-8.0)],stroke);
        painter.line_segment([center+egui::vec2(-8.0,-6.0),center+egui::vec2(-9.5,-8.0)],stroke);
        painter.add(egui::Shape::line(vec![center+egui::vec2(0.0,3.0),center+egui::vec2(-3.0,0.0),center+egui::vec2(0.0,-1.0),center+egui::vec2(3.0,0.0),center+egui::vec2(0.0,3.0)],stroke));
        painter.circle_filled(center+egui::vec2(2.0,5.0),1.5,ink);
        painter.circle_filled(center+egui::vec2(5.0,6.0),1.5,ink);
    }
    let title_size=if width<185.0{11.5}else{13.0};
    painter.text(rect.min+egui::vec2(42.0,20.0),egui::Align2::LEFT_CENTER,title,egui::FontId::proportional(title_size),ui.visuals().text_color());
    let badge=if overseas{"默认"}else{"授权码"};
    let badge_ink=if overseas{ui.visuals().weak_text_color()}else{color(ctx,Color32::from_rgb(255,200,150),Color32::from_rgb(200,140,80))};
    let galley=painter.layout_no_wrap(badge.into(),egui::FontId::proportional(9.0),badge_ink);
    let badge_rect=egui::Rect::from_min_size(egui::pos2(rect.right()-10.0-galley.size().x-10.0,rect.top()+12.0),galley.size()+egui::vec2(10.0,6.0));
    painter.rect_filled(badge_rect,4.0,crate::paint::mix(fill,badge_ink,0.10));painter.galley(badge_rect.min+egui::vec2(5.0,3.0),galley,badge_ink);
    let mut x=rect.left()+42.0;
    for (label,fee) in [(if overseas{"海外"}else{"台湾"},false),(if overseas{"免费"}else{"免费授权"},true)]{
        let ink=if fee{color(ctx,Color32::from_rgb(130,211,168),Color32::from_rgb(30,127,79))}else{ui.visuals().weak_text_color()};
        let galley=painter.layout_no_wrap(label.into(),egui::FontId::proportional(10.0),ink);
        let pill=egui::Rect::from_min_size(egui::pos2(x,rect.top()+40.0),galley.size()+egui::vec2(10.0,6.0));
        painter.rect_filled(pill,4.0,crate::paint::mix(fill,ink,if fee{0.10}else{0.06}));painter.galley(pill.min+egui::vec2(5.0,3.0),galley,ink);x=pill.right()+5.0;
    }
    response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,ui.is_enabled(),selected,title));
    response.on_hover_text(if overseas{"海外源，默认免费使用，无需 Key。"}else{"中央氣象署開放資料：台灣官方氣象資料。需申請免費授權碼，僅支援台灣縣市與鄉鎮。"})
}

#[derive(Default)]
pub(crate) struct State {
    pub selected:Provider,
    pub displayed:Provider,
    /// 授權碼輸入框內容；測試通過並套用後清空。
    pub key:String,
    pub message:Option<(&'static str,bool)>,
    pub config:Config,
    pub config_open:bool,
    /// 已保存在憑證管理器中的授權碼。
    pub client:Option<CwaClient>,
    testing:Option<Receiver<Result<String,CwaError>>>,
    /// 通過連線測試、等待套用的授權碼。
    tested_key:Option<String>,
    test_error:Option<CwaError>,
    pub refreshing:Option<Receiver<Result<Snapshot,CwaError>>>,
    pub refresh_error:Option<CwaError>,
    last_attempt:i64,
    pub draft_county:String,
    pub draft_district:String,
}
impl State {
    pub fn load()->Self {
        if cfg!(test){return Self::default();}
        let mut config=Config::load();
        // 舊版把授權碼明文放在設定檔；搬進憑證管理器後立即從設定檔移除。
        if !config.legacy_key.is_empty() && weather_provider::store_cwa_key(&config.legacy_key).is_ok() {
            config.legacy_key.clear();
            let _=config.save();
        }
        let client=weather_provider::load_cwa_key().map(CwaClient::new);
        let (county,district)=config.location.as_ref().filter(|l|!l.county.is_empty())
            .map(|l|(l.county.clone(),l.district.clone())).unwrap_or_else(||("高雄市".into(),String::new()));
        let active=config.provider;
        Self{selected:active,displayed:active,config,client,draft_county:county,draft_district:district,..Default::default()}
    }
    fn invalidate(&mut self){
        self.testing=None;self.tested_key=None;self.test_error=None;self.message=None;
    }
    pub(crate) fn close(&mut self){self.invalidate();self.key.clear();}
    fn test_connection(&mut self){
        self.invalidate();
        let key=self.key.trim().to_string();
        if key.is_empty(){
            self.test_error=Some(CwaError::NoKey);self.message=Some(("请先填写授权码。",true));return;
        }
        if cfg!(test){self.message=Some(("测试模式不发送真实请求。",false));return;}
        let county=self.draft_county.clone();
        self.testing=Some(cwa_weather::spawn(move||{CwaClient::new(key.clone()).test(&county)?;Ok(key)}));
    }
    pub fn poll(&mut self,enabled:bool){
        if !enabled{self.invalidate();self.refreshing=None;return;}
        if let Some(result)=self.testing.as_ref().and_then(|rx|rx.try_recv().ok()){
            self.testing=None;
            match result {
                Ok(key)=>{self.tested_key=Some(key);self.test_error=None;self.message=Some(("授权码验证通过，选择縣市后点「保存并启用」。",false));},
                Err(e)=>{self.tested_key=None;self.test_error=Some(e);self.message=Some((e.text(),true));},
            }
        }
    }
    /// 取得 CWA 資料的排程判定：有授權碼、有地點、沒有進行中的請求，且距離上次成功或嘗試已夠久。
    pub(crate) fn refresh_due(&self,now:i64,snapshot_at:Option<i64>)->bool {
        // last_attempt 为 0 表示尚未尝试过，立即取得。
        self.client.is_some()&&self.config.location.is_some()&&self.refreshing.is_none()
            &&(self.last_attempt==0||now-self.last_attempt>=RETRY_SECS)&&snapshot_at.map_or(true,|at|now-at>=REFRESH_SECS)
    }
    pub(crate) fn start_refresh(&mut self,now:i64){
        let (Some(client),Some(location))=(self.client.clone(),self.config.location.clone())else{return;};
        if self.refreshing.is_some(){return;}
        self.last_attempt=now;
        let Location{position,county,district,..}=location;
        self.refreshing=Some(cwa_weather::spawn(move||client.fetch_all(&county,&district,position)));
    }
    /// 手動刷新：不受排程限制，但仍不會與進行中的請求重疊。
    pub(crate) fn refresh_now(&mut self,now:i64){self.last_attempt=0;self.start_refresh(now);}
    pub(crate) fn poll_refresh(&mut self)->Option<Result<Snapshot,CwaError>>{
        let result=self.refreshing.as_ref()?.try_recv().ok()?;
        self.refreshing=None;
        self.refresh_error=result.as_ref().err().copied();
        Some(result)
    }
    fn connection_status(&self,ui:&mut egui::Ui){
        let (title,detail,dark,light)=if self.testing.is_some(){
            ("正在測試授權碼","正在驗證 CWA 開放資料 API...",Color32::from_rgb(29,48,70),Color32::from_rgb(232,241,255))
        }else if let Some(error)=self.test_error {
            ("授權碼驗證失敗",error.text(),Color32::from_rgb(69,32,35),Color32::from_rgb(255,235,235))
        }else if self.tested_key.is_some() {
            ("授權碼驗證通過","CWA API 連線正常。請選擇縣市並點擊「保存並啟用」。",Color32::from_rgb(24,60,44),Color32::from_rgb(228,247,237))
        }else if self.client.is_some() {
            ("授權碼已安全保存","可直接選擇縣市或鄉鎮，點擊「保存並啟用」。",Color32::from_rgb(24,60,44),Color32::from_rgb(228,247,237))
        }else{return;};
        let ink=if self.test_error.is_some(){color(ui.ctx(),Color32::from_rgb(255,155,155),Color32::from_rgb(174,46,46))}
            else if self.testing.is_some(){color(ui.ctx(),Color32::from_rgb(139,188,255),Color32::from_rgb(36,100,181))}
            else{color(ui.ctx(),Color32::from_rgb(123,225,169),Color32::from_rgb(25,120,76))};
        ui.add_space(7.0);
        egui::Frame::new().fill(color(ui.ctx(),dark,light)).stroke(egui::Stroke::new(1.0,ink.gamma_multiply(0.35))).corner_radius(8.0).inner_margin(10).show(ui,|ui|{
            ui.set_min_width(ui.available_width());ui.label(RichText::new(title).size(13.5).strong().color(ink));ui.add(egui::Label::new(RichText::new(detail).size(11.5)).wrap());
        });
    }
}

impl App {
    /// 套用天氣來源。Open-Meteo 立即切換；CWA 需要授權碼（已保存或剛通過測試）與有效的縣市或鄉鎮。
    fn apply_weather_source(&mut self){
        if !self.module_on(WEATHER_PAGE){return;}
        let now=chrono::Utc::now().timestamp();
        let source=&mut self.weather_source;
        if source.selected==Provider::OpenMeteo {
            let mut config=source.config.clone();config.provider=Provider::OpenMeteo;
            if let Err(e)=config.save(){source.message=Some((e.text(),true));return;}
            source.config=config;source.displayed=Provider::OpenMeteo;source.key.clear();source.invalidate();source.config_open=false;source.refreshing=None;
            source.message=Some(("已应用，天气与城市列表已切换到所选来源。",false));
            self.weather=crate::weather::State::load(&self.settings.weather_city,self.settings.weather_position.as_ref());
            return;
        }
        let Some(location)=weather_provider::cwa_location(&source.draft_county,&source.draft_district)else{
            source.message=Some(("请选择有效的縣市與鄉鎮。",true));return;
        };
        if let Some(key)=source.tested_key.take(){
            if weather_provider::store_cwa_key(&key).is_err(){source.message=Some((Error::Storage.text(),true));return;}
            source.client=Some(CwaClient::new(key));
        }
        if source.client.is_none(){source.message=Some(("请先填写授权码并通过测试。",true));return;}
        let mut config=source.config.clone();config.provider=Provider::Cwa;
        if !config.saved.contains(&location){
            if config.saved.len()>=crate::weather::MAX_CITIES{source.message=Some(("已保存 5 个城市，请先移除一个再添加。",true));return;}
            config.saved.push(location.clone());
        }
        config.location=Some(location);
        if let Err(e)=config.save(){source.message=Some((e.text(),true));return;}
        source.config=config;source.displayed=Provider::Cwa;source.key.clear();source.invalidate();source.config_open=false;
        source.refresh_error=None;source.refresh_now(now);
        self.weather.snapshot=None;self.weather.error=None;self.weather.loading=true;
        self.weather_source.message=Some(("已应用，正在取得中央氣象署天氣资料。",false));
    }

    /// 切換到已保存的 CWA 地點。
    pub(crate) fn cwa_select_location(&mut self,location:Location){
        if !self.module_on(WEATHER_PAGE){return;}
        let mut config=self.weather_source.config.clone();config.location=Some(location.clone());
        if config.save().is_err(){self.weather_source.message=Some((Error::Storage.text(),true));return;}
        self.weather_source.config=config;
        self.weather_source.draft_county=location.county.clone();self.weather_source.draft_district=location.district.clone();
        self.weather_source.refresh_error=None;self.weather_source.refresh_now(chrono::Utc::now().timestamp());
        self.weather.snapshot=None;self.weather.error=None;self.weather.loading=true;
    }

    /// 保存目前選擇的縣市／鄉鎮到已保存列表（不啟用）。
    pub(crate) fn cwa_add_city(&mut self){
        if !self.module_on(WEATHER_PAGE){return;}
        let Some(location)=weather_provider::cwa_location(&self.weather_source.draft_county,&self.weather_source.draft_district)else{return;};
        let source=&mut self.weather_source;
        if source.config.saved.contains(&location){source.message=Some(("这个城市已保存。",false));return;}
        if source.config.saved.len()>=crate::weather::MAX_CITIES{source.message=Some(("已保存 5 个城市，请先移除一个再添加。",true));return;}
        let mut config=source.config.clone();config.saved.push(location);
        match config.save(){Err(e)=>source.message=Some((e.text(),true)),Ok(())=>{source.config=config;source.message=None;}}
    }

    pub(crate) fn cwa_remove_city(&mut self,location:&Location){
        if !self.module_on(WEATHER_PAGE){return;}
        let mut config=self.weather_source.config.clone();config.saved.retain(|l|l!=location);
        let active_changed=config.location.as_ref()==Some(location);
        if active_changed{config.location=config.saved.first().cloned();}
        if let Err(error)=config.save(){self.weather_source.message=Some((error.text(),true));return;}
        self.weather_source.config=config;self.weather_source.message=None;
        if active_changed {
            match self.weather_source.config.location.clone(){
                Some(next)=>self.cwa_select_location(next),
                None=>{self.weather_source.refreshing=None;self.weather.snapshot=None;},
            }
        }
    }

    pub(crate) fn weather_source_settings(&mut self,ui:&mut egui::Ui){
        settings_label(ui,"天气来源");
        settings_card(ui,|ui|{
            let width=(ui.available_width()-ui.spacing().item_spacing.x)/2.0;
            ui.horizontal(|ui|{
                for provider in [Provider::OpenMeteo,Provider::Cwa]{
                    let selected=self.weather_source.selected==provider;
                    if provider_option(ui,provider,selected,width).clicked()&& !selected{
                        self.weather_source.selected=provider;self.weather_source.message=None;
                        if provider==Provider::Cwa&&self.weather_source.client.is_none(){self.weather_source.config_open=true;}
                    }
                }
            });
            ui.add_space(5.0);
            settings_hint(ui,&format!("当前生效：{}{}",self.weather_source.displayed.name(),if self.weather_source.selected!=self.weather_source.displayed{" · 所选来源尚未应用"}else{""}));
            if self.weather_source.selected==Provider::OpenMeteo {
                settings_hint(ui,"无需 Key。点击应用后使用 Open-Meteo 自己的城市与定位设置。");
                if self.weather_source.displayed!=Provider::OpenMeteo&&ui.button("应用 Open-Meteo").clicked(){self.apply_weather_source();}
                return;
            }
            ui.add_space(4.0);ui.separator();ui.add_space(3.0);
            ui.horizontal_wrapped(|ui|{
                ui.label(RichText::new("台灣中央氣象署 (CWA OpenData) API 授權碼").strong());
                if ui.small_button(if self.weather_source.config_open{"收起"}else{"展开"}).clicked(){self.weather_source.config_open=!self.weather_source.config_open;}
                ui.hyperlink_to("申请授权码",AUTHKEY_URL);
            });
            if !self.weather_source.config_open {self.weather_source.connection_status(ui);}
            if self.weather_source.config_open {
                ui.add_space(5.0);
                ui.label(RichText::new("授权码").size(12.0));
                let hint=if self.weather_source.client.is_some(){"已安全保存；填写新授权码可更换"}else{"在 CWA 开放资料平台申请的授权码"};
                let key=ui.add(egui::TextEdit::singleline(&mut self.weather_source.key).id_salt("cwa-key").password(true).hint_text(hint).desired_width(f32::INFINITY));
                if key.changed(){self.weather_source.invalidate();}
                ui.add_space(5.0);
                ui.horizontal_wrapped(|ui|{
                    if ui.add_enabled(self.weather_source.testing.is_none(),egui::Button::new("测试授权码")).clicked(){self.weather_source.test_connection();}
                    if self.weather_source.testing.is_some(){ui.spinner();ui.label("测试中...");ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
                    let clearable=self.weather_source.client.is_some()||!self.weather_source.key.is_empty();
                    if ui.add_enabled(clearable,egui::Button::new("清除本机授权码")).clicked(){
                        let clearing_active=self.weather_source.displayed==Provider::Cwa;
                        match weather_provider::delete_cwa_key(){
                            Err(e)=>self.weather_source.message=Some((e.text(),true)),
                            Ok(())=>{
                                self.weather_source.client=None;self.weather_source.key.clear();self.weather_source.invalidate();
                                if clearing_active{self.weather_source.selected=Provider::OpenMeteo;self.apply_weather_source();}
                                self.weather_source.message=Some(("本机授权码已清除。",false));
                            },
                        }
                    }
                });
                self.weather_source.connection_status(ui);
                settings_hint(ui,"授权码只保存在 Windows 凭据管理器，不写入设置文件。CWA 开放资料仅提供台湾地区天气。");
            }
            if self.weather_source.test_error.is_none() {if let Some((message,error))=self.weather_source.message {ui.add(egui::Label::new(RichText::new(message).size(11.5).color(if error{color(ui.ctx(),Color32::from_rgb(255,137,137),Color32::from_rgb(183,48,48))}else{ui.visuals().weak_text_color()})).wrap());}}
        });
    }

    /// CWA 縣市與鄉鎮選擇、已保存城市列表與來源說明。
    pub(crate) fn cwa_city_settings(&mut self,ui:&mut egui::Ui){
        let active=self.weather_source.displayed==Provider::Cwa;
        let ready=self.weather_source.client.is_some()||self.weather_source.tested_key.is_some();
        let accent=color(ui.ctx(),Color32::from_rgb(96,164,255),Color32::from_rgb(34,108,220));
        settings_label(ui,"中央氣象署城市");
        settings_card(ui,|ui|{
            ui.horizontal(|ui|{ui.label(RichText::new("已保存城市").strong());ui.label(RichText::new(format!("{} / 5",self.weather_source.config.saved.len())).color(ui.visuals().weak_text_color()));});
            let saved=self.weather_source.config.saved.clone();let(mut chosen,mut removed)=(None,None);
            for location in saved {
                let cached=if active{self.weather.snapshot.as_ref().filter(|s|s.position.as_ref()==Some(&location.position))}else{None};
                let (pick,remove)=crate::weather_ui::city_row(ui,self.weather_source.config.location.as_ref()==Some(&location),true,&location.position.label,&location.position.region,cached,accent);
                if pick {chosen=Some(location.clone());}if remove {removed=Some(location);}
            }
            if let Some(location)=removed{self.cwa_remove_city(&location);}
            else if let Some(location)=chosen {self.cwa_select_location(location);}
            if self.weather_source.config.saved.is_empty(){settings_hint(ui,"还没有保存城市，选择縣市或鄉鎮后点「添加城市」，最多保存 5 个。");}
            else {settings_hint(ui,"点击城市即可切换；中央氣象署仅支持手动选择地区。");}
            ui.add_space(7.0);ui.separator();ui.add_space(6.0);
            let counties=cwa_weather::TW_COUNTIES;
            ui.horizontal(|ui|{
                ui.label(RichText::new("縣市").size(12.0));
                egui::ComboBox::from_id_salt("cwa-county").width(150.0).selected_text(self.weather_source.draft_county.clone()).show_ui(ui,|ui|{
                    for county in counties {
                        if ui.selectable_value(&mut self.weather_source.draft_county,county.to_string(),county).changed(){self.weather_source.draft_district.clear();}
                    }
                });
            });
            let districts=cwa_weather::district_options(&self.weather_source.draft_county);
            ui.horizontal(|ui|{
                ui.label(RichText::new("鄉鎮").size(12.0));
                ui.add_enabled_ui(!districts.is_empty(),|ui|{
                    let shown=if self.weather_source.draft_district.is_empty(){"整個縣市".to_string()}else{self.weather_source.draft_district.clone()};
                    egui::ComboBox::from_id_salt("cwa-district").width(150.0).selected_text(shown).height(260.0).show_ui(ui,|ui|{
                        ui.selectable_value(&mut self.weather_source.draft_district,String::new(),"整個縣市");
                        for district in districts {ui.selectable_value(&mut self.weather_source.draft_district,district.to_string(),*district);}
                    });
                });
                if districts.is_empty(){settings_hint(ui,"此縣市目前只提供縣市級預報");}
            });
            let draft=weather_provider::cwa_location(&self.weather_source.draft_county,&self.weather_source.draft_district);
            let exists=draft.as_ref().is_some_and(|l|self.weather_source.config.saved.contains(l));
            let full=self.weather_source.config.saved.len()>=crate::weather::MAX_CITIES;
            ui.add_space(5.0);
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(draft.is_some()&&!exists&&!full,egui::Button::new("添加城市")).clicked(){self.cwa_add_city();}
                settings_hint(ui,if exists{"这个城市已保存"}else if full{"已保存 5 个城市，移除一个后可继续添加。"}else{"添加后可随时在上方列表切换。"});
            });
            if let Some(location)=draft.as_ref() {settings_hint(ui,&format!("已选择：{}",location.position.label));}
            ui.add_space(7.0);
            if ui.add_enabled(ready&&draft.is_some(),egui::Button::new("保存并启用")).clicked(){self.apply_weather_source();}
            if !ready {settings_hint(ui,"请先在上方填写并测试授权码。");}
        });
        if active {
            settings_card(ui,|ui|{
                ui.hyperlink_to("天气数据来自中央氣象署开放资料平台","https://opendata.cwa.gov.tw/");
                settings_hint(ui,"观测、36 小时与一周预报约每小时刷新。降水概率为每时段或每日较高值。");
                let stamp=self.weather.snapshot.as_ref().map_or(0,|s|s.at);
                settings_hint(ui,&format!("上次更新：{}",updated_at(stamp)));
                if let Some(e)=self.weather_source.refresh_error {settings_hint(ui,e.text());}
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn cwa_refresh_is_hourly_and_retries_failures_after_thirty_minutes(){
        let mut state=State::default();
        assert!(!state.refresh_due(1_000,None),"没有授权码或地点时不能刷新");
        state.client=Some(CwaClient::new("placeholder-key".into()));
        state.config.location=weather_provider::cwa_location("高雄市","三民區");
        assert!(state.refresh_due(1_000,None),"首次没有数据时应立即取得");
        state.last_attempt=1_000;
        assert!(!state.refresh_due(1_000+RETRY_SECS-1,None),"失败后 30 分钟内不重试");
        assert!(state.refresh_due(1_000+RETRY_SECS,None));
        // 成功取得的数据：一小时内不重复请求，满一小时后再次取得。
        state.last_attempt=10_000;
        assert!(!state.refresh_due(10_000+REFRESH_SECS-1,Some(10_000)),"成功取得后一小时内不重复请求");
        assert!(state.refresh_due(10_000+REFRESH_SECS,Some(10_000)));
        state.refreshing=Some(mpsc::channel().1);
        assert!(!state.refresh_due(10*REFRESH_SECS,Some(0)),"进行中的请求不能重叠");
    }

    #[test]
    fn empty_key_fails_fast_without_starting_a_request(){
        let mut state=State::default();
        state.key="   ".into();state.test_connection();
        assert_eq!(state.test_error,Some(CwaError::NoKey));
        assert!(state.testing.is_none());
        assert_eq!(state.message,Some(("请先填写授权码。",true)));
    }

    #[test]
    fn successful_test_stays_pending_until_the_user_applies_it(){
        let mut state=State::default();
        let (tx,rx)=mpsc::channel();state.testing=Some(rx);
        tx.send(Ok("placeholder-key".to_string())).unwrap();
        state.poll(true);
        assert_eq!(state.tested_key.as_deref(),Some("placeholder-key"));
        assert!(state.client.is_none(),"未套用前不能写入凭据或启用来源");
        let (tx,rx)=mpsc::channel();state.testing=Some(rx);
        tx.send(Err(CwaError::InvalidKey)).unwrap();
        state.poll(true);
        assert!(state.tested_key.is_none());
        assert_eq!(state.test_error,Some(CwaError::InvalidKey));
    }

    #[test]
    fn disabling_the_module_discards_pending_work(){
        let mut state=State::default();
        let (tx,rx)=mpsc::channel();state.testing=Some(rx);
        tx.send(Ok("placeholder-key".to_string())).unwrap();
        state.poll(false);
        assert!(state.testing.is_none());
        assert!(state.tested_key.is_none());
    }

    #[test]
    fn district_must_belong_to_its_county_and_labels_use_the_full_name(){
        assert!(weather_provider::cwa_location("高雄市","仁德區").is_none());
        let location=weather_provider::cwa_location("臺南","仁德區").unwrap();
        assert_eq!(location.position.label,"臺南市仁德區");
        assert_eq!((location.county.as_str(),location.district.as_str()),("臺南市","仁德區"));
        let county_only=weather_provider::cwa_location("高雄市","").unwrap();
        assert_eq!(county_only.position.label,"高雄市");
        assert!(county_only.valid(),"CWA 地点需要能通过设定读取的有效性检查");
    }
}
