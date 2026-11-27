//! 同心額度環、帳號選擇及帳號設定。
use crate::{app::{App,Cmd},extras::{setting_row,settings_card,settings_control_row},paint::{Card,H},quota::{self,Accent,Account,Amount,Config,Provider,Snapshot},theme::{self,W}};
use eframe::egui::{self,Color32};
use std::collections::{HashMap,HashSet};
use chrono::Datelike;

const CLAUDE_LOCAL_NOTICE:&str="由於 Claude 對網路使用環境要求較高，島仔不會使用登入憑證向 Claude 服務傳送任何請求，只讀取本機用量記錄與快取，自動顯示其中的重置時間；缺少有效記錄時，顯示“重置時間未知”。";

#[derive(Default)]
pub(crate) struct State {
    cache_path:Option<std::path::PathBuf>,
    pub config:Config,pub rows:HashMap<String,Result<Snapshot,String>>,pub busy:HashSet<String>,
    pub provider:Provider,pub alias:String,pub source:String,pub note:String,
    pub relay_manual:bool,
    pub relay_user_token:bool,pub relay_user_id:String,
    pub relay_script:String,pub relay_access_token:String,
    pub connect_open:bool,pub connect_target:Option<String>,
    pub login:Option<quota::grok::Login>,
    pub workbuddy_connecting:Option<quota::workbuddy::Connect>,
    pub deepseek_connecting:Option<quota::deepseek::Connect>,pub deepseek_key:String,
    pub api_connecting:Option<quota::api::Connect>,pub api_key:String,
    pub detail_account:Option<String>,
    pub kiro_connecting:Option<quota::kiro::Connect>,
    pub stale:HashMap<String,String>,
    pub gemini_connecting:Option<quota::gemini::Connect>,pub gemini_email:String,pub gemini_cookie:String,
    pub doubao_connecting:Option<quota::doubao::Connect>,pub doubao_cookie:String,
}
impl State {
    fn balance_text(&self,id:&str)->Option<String>{let s=self.rows.get(id)?.as_ref().ok()?;s.wallet.as_ref().map(|w|w.primary().compact()).or_else(||s.credits.as_ref().map(|c|c.compact())).or_else(||s.relay.as_ref().map(|q|q.compact()))}
    fn balance_loading(&self,id:&str)->bool{self.balance_text(id).is_none()&&(self.busy.contains(id)||!self.rows.contains_key(id))}
    /// 環類帳號跟隨在圖示右側的數字：有具體點數時取主週期剩餘點數（如 Kiro 的 904）。
    fn ring_tail_text(&self,id:&str)->Option<String>{self.rows.get(id)?.as_ref().ok()?.rings().first()?.amount.as_ref().map(Amount::short)}
    /// 常駐膠囊裡緊跟圖示的文字：餘額類給餘額，點數類給剩餘點數。
    fn tail_text(&self,id:&str)->Option<String>{self.balance_text(id).or_else(||self.ring_tail_text(id))}
    pub fn load()->Self {
        if cfg!(test){return Self::default();}
        let mut state=Self::restore(Config::load(),crate::model::config_dir().join("ai-quota-cache.json"));
        if let Err(error)=quota::api::cleanup_pending(&state.config){state.note=error;}state
    }

    fn restore(config:Config,path:std::path::PathBuf)->Self {
        let mut state=Self{config,cache_path:Some(path.clone()),..Default::default()};
        let cache:serde_json::Value=std::fs::read(path).ok()
            .and_then(|raw|serde_json::from_slice(&raw).ok()).unwrap_or_default();
        for a in &state.config.accounts {
            let Some(snapshot)=a.cached_snapshot(&cache)else{continue;};
            state.rows.insert(a.id.clone(),Ok(snapshot));
            state.stale.insert(a.id.clone(),"上次記錄，等待重新整理".into());
        }
        state
    }

    fn save_snapshots(&self){
        let Some(path)=&self.cache_path else{return;};
        let mut entries=serde_json::Map::new();
        for a in &self.config.accounts {
            if let Some(Ok(snapshot))=self.rows.get(&a.id) {
                let mut entry=serde_json::json!({"provider":a.provider.name(),"slot":a.slot,"source":a.source,"snapshot":snapshot});
                if a.provider==Provider::Claude{entry["local_identity_checked"]=a.cache_identity_marker();}
                entries.insert(a.id.clone(),entry);
            }
        }
        if let Ok(raw)=serde_json::to_vec(&entries){let _=crate::model::atomic_write(path,&raw);}
    }

    fn connect_kiro(&mut self,target:Option<&Account>,connected:quota::kiro::Connected)->Result<(),String>{
        let mut next=self.config.clone();let id=quota::kiro::apply_connection(&mut next,target,&connected)?;
        next.save_checked()?;self.config=next;
        self.stale.remove(&id);self.busy.remove(&id);self.rows.insert(id,Ok(connected.snapshot));self.save_snapshots();self.connect_target=None;Ok(())
    }
    pub fn receive(&mut self,mut event:quota::Event){
        self.busy.remove(&event.id);
        if !self.config.accounts.iter().any(|a|a.id==event.id && a.enabled){return;}
        if let Some(account)=self.config.accounts.iter().find(|a|a.id==event.id&&a.provider==Provider::Kiro).cloned(){
            if let Ok(snapshot)=&event.result{
                let verified=(||{
                    quota::kiro::check_owner(&account,snapshot)?;
                    if account.slot.is_none(){
                        let owner=snapshot.kiro.as_ref().unwrap().owner.clone();
                        if self.config.accounts.iter().any(|a|a.id!=account.id&&a.provider==Provider::Kiro&&a.slot.as_deref()==Some(&owner)){return Err("此 Kiro 帳號已經連線，請移除重複的登入檔案".into());}
                        let mut next=self.config.clone();next.accounts.iter_mut().find(|a|a.id==account.id).unwrap().slot=Some(owner);
                        next.save_checked()?;self.config=next;
                    }
                    Ok::<_,String>(())
                })();
                if let Err(error)=verified{event.result=Err(error);}
            }
        }
        // Claude 桌面版記錄沒有重置時間：視窗長度已過完的舊數字早已不成立，不能因為這次讀取失敗就留在畫面上。
        let desktop=self.config.accounts.iter().any(|a|a.id==event.id&&a.claude_desktop());
        if desktop{
            let all_expired=match self.rows.get_mut(&event.id){Some(Ok(old))=>old.drop_expired_windows(chrono::Utc::now().timestamp())>0&&old.windows.is_empty(),_=>false};
            if all_expired{self.rows.remove(&event.id);}
        }
        if let Err(error)=&event.result{
            if matches!(self.rows.get(&event.id),Some(Ok(_))){
                self.stale.insert(event.id,error.clone());return;
            }
        }
        let success=event.result.is_ok();
        let record_age=event.result.as_ref().ok().map(|s|chrono::Utc::now().timestamp()-s.at).filter(|age|*age>1800);
        self.stale.remove(&event.id);
        if let Some(age)=record_age.filter(|_|self.config.accounts.iter().any(|a|a.id==event.id&&a.provider==Provider::Claude)){
            let hint=if desktop{"；請開啟 Claude 桌面版的用量頁面"}else{""};
            self.stale.insert(event.id.clone(),format!("本地記錄較舊（{}），等待 Claude 更新{hint}",age_text(age)));
        }
        self.rows.insert(event.id,event.result);
        if success{self.save_snapshots();}
    }
}
/// 記錄距今多久，用於「本地記錄較舊」提示。
fn age_text(seconds:i64)->String{
    if seconds<3600{format!("{} 分鐘前",(seconds/60).max(1))}else if seconds<172800{format!("{} 小時前",seconds/3600)}else{format!("{} 天前",seconds/86400)}
}
fn accent_color(accent:Accent,provider:Provider)->Color32{
    if accent==Accent::Brand&&provider==Provider::DeepSeek{return Color32::from_rgb(77,107,254);}
    let accent=if accent==Accent::Brand{match provider{Provider::Codex|Provider::OpenCodeGo|Provider::Kiro=>Accent::Violet,Provider::Claude=>Accent::Orange,Provider::Kimi=>Accent::Blue,Provider::GrokBot=>Accent::Green,Provider::Gemini=>Accent::Cyan,Provider::Antigravity=>Accent::Blue,Provider::ZCode|Provider::Glm|Provider::Doubao=>Accent::Blue,Provider::WorkBuddy=>Accent::Green,Provider::DeepSeek=>Accent::Blue,Provider::MiniMax=>Accent::Pink,_=>Accent::Cyan}}else{accent};
    let (r,g,b)=match accent{Accent::Brand|Accent::Violet=>(142,67,255),Accent::Orange=>(255,116,26),Accent::Blue=>(40,126,255),Accent::Cyan=>(0,213,242),Accent::Pink=>(255,43,132),Accent::Gold=>(255,190,0),Accent::Green=>(52,221,145)};
    Color32::from_rgb(r,g,b)
}
fn accent_colors(config:&Config,a:&Account)->[Color32;2]{
    let main=accent_color(config.colors.get(&a.id).copied().unwrap_or_default(),a.provider);
    [main,crate::paint::mix(main,Color32::WHITE,0.16)]
}
fn color_picker(ui:&mut egui::Ui,provider:Provider,value:&mut Accent){
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(),30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
        ui.label(egui::RichText::new("顏色").size(12.0).color(theme::GRAY));
        if ui.selectable_label(*value==Accent::Brand,egui::RichText::new("預設").size(12.0)).on_hover_text("跟隨廠商").clicked(){*value=Accent::Brand;}
        for accent in Accent::ALL.into_iter().skip(1){
            let (rect,response)=ui.allocate_exact_size(egui::vec2(22.0,25.0),egui::Sense::click());
            let on=*value==accent;
            if response.on_hover_text(accent.label()).clicked(){*value=accent;}
            if on{ui.painter().circle_stroke(rect.center(),10.0,egui::Stroke::new(1.5,Color32::WHITE));}
            ui.painter().circle_filled(rect.center(),7.0,accent_color(accent,provider));
        }
        ui.label(egui::RichText::new(value.label()).size(12.0).color(theme::GRAY));
    });
}
pub(crate) fn bar_tooltip(response:egui::Response,text:String)->egui::Response{
    let ctx=&response.ctx;let id=response.id.with("bar-tooltip");
    if response.hovered()&&!ctx.input(|i|i.pointer.any_down()){
        let now=ctx.input(|i|i.time);
        let since=ctx.data_mut(|data|*data.get_temp_mut_or_insert_with(id,||now));
        if now-since>=0.5{
            egui::Area::new(id).order(egui::Order::Tooltip).fixed_pos(response.rect.left_bottom()+egui::vec2(0.0,4.0)).interactable(false).show(ctx,|ui|{
                egui::Frame::popup(ui.style()).show(ui,|ui|{ui.label(&text);});
            });
        }else{ctx.request_repaint_after_secs((0.5-(now-since))as f32);}
    }else{ctx.data_mut(|data|data.remove::<f64>(id));}
    response
}
/// 這些廠商的圖示自帶品牌配色（官方方形標），直接使用原圖；其餘統一壓成白色標識。
fn keeps_brand_color(provider:Provider)->bool{matches!(provider,Provider::WorkBuddy|Provider::ZCode|Provider::MiniMax|Provider::Kiro|Provider::Doubao|Provider::Antigravity|Provider::Gemini|Provider::Codex|Provider::Claude|Provider::Glm|Provider::DeepSeek|Provider::NewApi|Provider::Sub2Api)}
fn logo_texture(ctx:&egui::Context,provider:Provider,pixels:u32)->Option<egui::TextureHandle>{
    let key=egui::Id::new(("quota-logo",provider.name(),pixels));
    if let Some(texture)=ctx.data_mut(|data|data.get_temp::<egui::TextureHandle>(key)){return Some(texture);}
    // WorkBuddy 圖示來自官方 WorkBuddyAI 客戶端的 workbuddy-app-icon SVG，保留原始配色。
    let mut image=if provider==Provider::Doubao{
        let bitmap=image::load_from_memory(include_bytes!("../assets/ai-icons/doubao.png")).ok()?.resize_exact(pixels,pixels,image::imageops::FilterType::Lanczos3).to_rgba8();
        egui::ColorImage::from_rgba_unmultiplied([pixels as usize,pixels as usize],bitmap.as_raw())
    }else{
        let bytes:&[u8]=match provider{Provider::ZCode=>include_bytes!("../assets/ai-icons/zcode.svg"),Provider::Codex=>include_bytes!("../assets/ai-icons/codex.svg"),Provider::Claude=>include_bytes!("../assets/ai-icons/claude.svg"),Provider::Kimi=>include_bytes!("../assets/ai-icons/kimi.svg"),Provider::GrokBot=>include_bytes!("../assets/ai-icons/grokbot.svg"),Provider::Gemini=>include_bytes!("../assets/ai-icons/gemini.svg"),Provider::WorkBuddy=>include_bytes!("../assets/ai-icons/workbuddy.svg"),Provider::DeepSeek=>include_bytes!("../assets/ai-icons/deepseek.svg"),Provider::Kiro=>include_bytes!("../assets/ai-icons/kiro.svg"),Provider::Antigravity=>include_bytes!("../assets/ai-icons/antigravity.svg"),Provider::OpenCodeGo=>include_bytes!("../assets/ai-icons/opencode.svg"),Provider::MiniMax=>include_bytes!("../assets/ai-icons/minimax.svg"),Provider::Glm=>include_bytes!("../assets/ai-icons/glm.svg"),Provider::NewApi=>include_bytes!("../assets/ai-icons/newapi.svg"),Provider::Sub2Api=>include_bytes!("../assets/ai-icons/sub2api.svg"),_=>include_bytes!("../assets/ai-icons/api-key.svg")};
        egui_extras::image::load_svg_bytes_with_size(bytes,Some(egui::load::SizeHint::Size(pixels,pixels))).ok()?
    };
    // 白色標識使用向量覆蓋率，避免透明邊緣再次變暗。
    if !keeps_brand_color(provider){for pixel in &mut image.pixels{*pixel=Color32::from_white_alpha(pixel.a());}}
    let texture=ctx.load_texture(format!("quota-logo-{}-{pixels}",provider.name()),image,egui::TextureOptions::LINEAR);
    if (!provider.balance()||provider.relay())&&!keeps_brand_color(provider){crate::skin_paint::mark_monochrome(texture.id());}else{crate::skin_paint::mark_colored(texture.id());}
    ctx.data_mut(|data|data.insert_temp(key,texture.clone()));Some(texture)
}
fn logo(c:&Card,provider:Provider,x:f32,y:f32,size:f32){
    let ctx=c.ui.ctx();let ppp=ctx.pixels_per_point();let center=c.pos(x,y);
    let pixels=(size*c.s*ppp).round().max(1.0) as u32;let half=pixels as f32/2.0;
    let min=egui::pos2((center.x*ppp-half).round()/ppp,(center.y*ppp-half).round()/ppp);
    let rect=egui::Rect::from_min_size(min,egui::Vec2::splat(pixels as f32/ppp));
    if let Some(texture)=logo_texture(ctx,provider,pixels){c.painter.image(texture.id(),rect,egui::Rect::from_min_max(egui::pos2(0.0,0.0),egui::pos2(1.0,1.0)),Color32::WHITE);}
}
fn deepseek_badge_svg(skin:crate::model::Skin)->String{
    use crate::model::Skin;
    let (fill,rim)=match skin{Skin::Classic=>("#e9edff","#cfd7ff"),Skin::Cream=>("#e9edff","#dce2fc"),Skin::GlassLight=>("#f5f7ff","#dfe5fa"),Skin::GlassDark|/* Skin::LiquidGlass| */Skin::Fluid=>("#dce4fa","#b8c9ec")};
    let whale=include_str!("../assets/ai-icons/deepseek.svg").replace("width=\"1em\"","x=\"12.5\" width=\"75\"").replace("height=\"1em\"","y=\"12.5\" height=\"75\"");
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"48\" fill=\"{fill}\" stroke=\"{rim}\" stroke-width=\"3\"/>{whale}</svg>")
}
fn deepseek_badge(c:&Card,skin:crate::model::Skin,x:f32,y:f32,size:f32){
    let ctx=c.ui.ctx();let ppp=ctx.pixels_per_point();let pixels=(size*c.s*ppp).round().max(1.0) as u32;
    let key=egui::Id::new(("deepseek-badge",skin.key(),pixels));
    let texture=ctx.data_mut(|data|data.get_temp::<egui::TextureHandle>(key)).or_else(||{
        let image=egui_extras::image::load_svg_bytes_with_size(deepseek_badge_svg(skin).as_bytes(),Some(egui::load::SizeHint::Size(pixels,pixels))).ok()?;
        let texture=ctx.load_texture(format!("deepseek-badge-{}-{pixels}",skin.key()),image,egui::TextureOptions::LINEAR);
        crate::skin_paint::mark_colored(texture.id());
        ctx.data_mut(|data|data.insert_temp(key,texture.clone()));Some(texture)
    });
    if let Some(texture)=texture{
        let center=c.pos(x,y);let half=pixels as f32/2.0;let min=egui::pos2((center.x*ppp-half).round()/ppp,(center.y*ppp-half).round()/ppp);
        c.painter.image(texture.id(),egui::Rect::from_min_size(min,egui::Vec2::splat(pixels as f32/ppp)),egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
    }
}
fn ring(c:&Card,a:&Account,snapshot:Option<&Snapshot>,x:f32,y:f32,size:f32,colors:[Color32;2]){ring_with(c,a,snapshot,x,y,size,colors,false);}
/// `logo_single`：單一週期時環中央也放廠商標誌，百分比改由旁邊的文字顯示。
fn ring_with(c:&Card,a:&Account,snapshot:Option<&Snapshot>,x:f32,y:f32,size:f32,colors:[Color32;2],logo_single:bool){
    let windows=snapshot.map(Snapshot::rings).unwrap_or_default();let dual=windows.len()>1;
    let stroke=if dual{if size<35.0{2.2}else{6.5}}else{(size*0.102).clamp(3.0,6.5)};
    let radius=size/2.0-stroke/2.0-1.0;
    let gap=if size<35.0{1.5}else{3.0};
    let start=-std::f32::consts::FRAC_PI_2;
    for i in 0..if dual{2}else{1}{
        let r=radius-i as f32*(stroke+gap);
        if let Some(w)=windows.get(i){
            let col=colors[i];let dark=crate::paint::mix(col,Color32::BLACK,0.8);
            if w.remaining>=100.0{c.ring(x,y,r,stroke,col);}else if w.remaining<=0.0{c.ring(x,y,r,stroke,dark);}else{
                let end=start+std::f32::consts::TAU*w.remaining/100.0;
                c.arc(x,y,r,end,start+std::f32::consts::TAU,stroke,dark);
                c.arc(x,y,r,start,end,stroke,col);
                for angle in [start,end]{c.circle(x+r*angle.cos(),y+r*angle.sin(),stroke/2.0,col);}
            }
        }
    }
    if a.provider==Provider::Kiro{logo(c,a.provider,x,y,if size<35.0{if dual{10.0}else{12.0}}else if dual{24.0}else{34.0});}
    else if a.provider==Provider::ZCode && windows.is_empty(){logo(c,a.provider,x,y,if size<35.0{size-3.0}else{size*0.72});}
    else if size<35.0 {logo(c,a.provider,x,y,if dual{10.0}else{12.0});}else if dual {logo(c,a.provider,x,y,24.0);}else if logo_single&&windows.first().is_some_and(|w|w.amount.is_none()){logo(c,a.provider,x,y,34.0);}else if let Some(w)=windows.first(){
        let value=format!("{:.0}",w.remaining);let large=if value.len()>2{22.0}else{28.0};
        let number_width=c.text_w(&value,large,W::Bold);let width=number_width+c.text_w("%",11.0,W::Semibold)+2.0;
        let left=x-width/2.0;c.centered_text(left,y,&value,large,W::Bold,theme::WHITE,H::Left);
        c.centered_text(left+number_width+2.0,y+3.0,"%",11.0,W::Semibold,theme::GRAY,H::Left);
    }else{c.centered_text(x,y,"—",24.0,W::Semibold,theme::DIM,H::Center);}
}
fn reset_text(at:Option<i64>,now:&chrono::DateTime<chrono::Local>)->String {
    let Some(at)=at.and_then(|at|chrono::DateTime::from_timestamp(at,0)) else{return "重置時間未知".into();};
    let at=at.with_timezone(&chrono::Local);
    if at.timestamp()<=now.timestamp(){return "等待重新整理".into();}
    let format=if at.date_naive()==now.date_naive(){"今天 %H:%M 重置"}else if at.year()==now.year(){"%-m月%-d日 %H:%M 重置"}else{"%Y/%-m/%-d %H:%M 重置"};
    at.format(format).to_string()
}
fn snapshot_details(snapshot:&Snapshot)->String{
    if let Some(relay)=&snapshot.relay{return relay.full();}
    if snapshot.kiro.is_some(){return kiro_details(snapshot);}
    let mut lines=Vec::new();let now=chrono::Local::now();
    for w in snapshot.windows.iter().chain(snapshot.extra.iter()){
        lines.push(format!("{}：剩餘 {:.1}% · {}",w.label,w.remaining,reset_text(w.reset,&now)));
    }
    lines.join("\n")
}
fn connection_group(provider:Provider)->Provider{
    if provider==Provider::Glm{Provider::ZCode}else if provider.relay(){Provider::NewApi}else{provider}
}
fn connection_name(provider:Provider)->&'static str{
    match connection_group(provider){Provider::ZCode=>"GLM / ZCode",Provider::NewApi=>"中轉站",other=>other.name()}
}
fn kiro_details(snapshot:&Snapshot)->String{
    let Some(details)=&snapshot.kiro else{return String::new();};
    let mut lines=details.pools.iter().map(|p|{
        let time=if let Some(at)=p.expires{
            chrono::DateTime::from_timestamp(at,0).map(|t|format!("{} 到期",t.with_timezone(&chrono::Local).format("%Y/%-m/%-d %H:%M"))).unwrap_or_default()
        }else if p.reset.is_some(){reset_text(p.reset,&chrono::Local::now())}else{String::new()};
        format!("{}：{} 點{}",p.label,p.amount.full(),if time.is_empty(){String::new()}else{format!(" · {time}")})
    }).collect::<Vec<_>>();
    for window in snapshot.windows.iter().skip(1){lines.push(format!("{}：{} 點 · {}",window.label,window.value_text(),reset_text(window.reset,&chrono::Local::now())));}
    lines.join("\n")
}
fn quota_reset_text(snapshot:&Snapshot,window:&quota::Window,now:&chrono::DateTime<chrono::Local>)->String{
    if window.label=="可用點數"{
        if let Some(details)=&snapshot.kiro{return details.pools.first().map(|p|format!("月度 · {}",reset_text(p.reset,now))).unwrap_or_default();}
    }
    reset_text(window.reset,now)
}
fn claude_disclaimer(ui:&mut egui::Ui){
    ui.separator();
    ui.label(egui::RichText::new("免責宣告").size(12.0).strong());
    ui.add(egui::Label::new(egui::RichText::new("島仔為非官方輔助工具，資料僅供參考，以 Claude 官方顯示為準；本地讀取不構成帳號安全保證。").size(12.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_gray(180),Color32::from_gray(75)))).wrap());
}
impl App {
    pub(crate) fn quota_close_connection(&mut self){
        if let Some(connect)=&mut self.quota.kiro_connecting{connect.cancel();}
        self.quota.connect_open=false;self.quota.connect_target=None;
        self.quota.api_connecting=None;self.quota.api_key.clear();
        self.quota.relay_user_token=false;self.quota.relay_user_id.clear();
        self.quota.relay_script.clear();self.quota.relay_access_token.clear();
        self.quota.gemini_connecting=None;
        self.quota.deepseek_key.clear();self.quota.gemini_cookie.clear();
        self.quota.doubao_connecting=None;self.quota.doubao_cookie.clear();
    }
    pub(crate) fn quota_tick(&mut self){
        if (!self.settings_open||!self.quota.connect_open)&&self.quota.gemini_connecting.is_some(){self.quota.gemini_connecting=None;self.quota.gemini_cookie.clear();}
        if (!self.settings_open||!self.quota.connect_open)&&self.quota.doubao_connecting.is_some(){self.quota_close_connection();}
        if !self.settings_open && self.quota.api_connecting.is_some(){self.quota_close_connection();}
        if let Some(result)=self.quota.api_connecting.as_ref().and_then(|c|c.poll()){
            self.quota.api_connecting=None;
            // Only an open, current dialog may persist a validated key.
            if self.settings_open && self.quota.connect_open {
                match result.and_then(|v|v.finish()){
                    Ok(connected)=>{
                        let previous=self.quota.config.clone();
                        let outcome=self.quota.config.connect_api(&connected,self.quota.connect_target.as_deref()).and_then(|id|self.quota.config.save_checked().map(|_|id));
                        match outcome{
                            Ok(id)=>{
                                self.quota.rows.insert(id.clone(),Ok(connected.snapshot));self.quota.stale.remove(&id);self.quota.busy.remove(&id);self.quota.save_snapshots();self.quota.connect_target=None;self.quota.note="已連線；資料以服務端返回為準".into();self.pending.push(Cmd::QuotaConnected);
                            },
                            Err(error)=>{self.quota.config=previous;self.quota.note=error;}
                        }
                    },Err(error)=>self.quota.note=error
                }
                if let Err(error)=quota::api::cleanup_pending(&self.quota.config){self.quota.note=format!("{}；{error}",self.quota.note);}
            }
        }

        if !self.settings_open||!self.quota.connect_open{if let Some(connect)=&mut self.quota.kiro_connecting{connect.cancel();}}
        if let Some(result)=self.quota.kiro_connecting.as_ref().and_then(|c|c.poll()){
            let connect=self.quota.kiro_connecting.take().unwrap();
            if !connect.cancelled(){
                self.quota.note=match result.and_then(|connected|self.quota.connect_kiro(connect.target.as_ref(),connected)){Ok(())=>{self.pending.push(Cmd::QuotaConnected);"Kiro 已連線，使用者身份已核驗".into()},Err(error)=>error};
            }
        }
        if let Some(result)=self.quota.workbuddy_connecting.as_ref().and_then(|c|c.poll()){
            self.quota.workbuddy_connecting=None;
            match result{Ok(connected)=>{
                let id=self.quota.config.connect_workbuddy(connected.slot,connected.label);self.quota.stale.remove(&id);self.quota.rows.insert(id,Ok(connected.snapshot));self.quota.save_snapshots();self.quota.connect_target=None;self.quota.config.save();self.quota.note="WorkBuddy 已連線".into();self.pending.push(Cmd::QuotaConnected);
            },Err(error)=>self.quota.note=error}
        }

        if let Some(result)=self.quota.deepseek_connecting.as_ref().and_then(|connect|connect.poll()){
            self.quota.deepseek_connecting=None;
            match result{Ok(connected)=>{let old=self.quota.connect_target.as_ref().and_then(|id|self.quota.config.accounts.iter().find(|a|&a.id==id)).cloned();
                let id=if let Some(target)=self.quota.connect_target.as_deref(){match self.quota.config.reconnect_deepseek(target,connected.slot.clone(),connected.label){Ok(id)=>id,Err(error)=>{self.quota.note=error;return;}}}else{self.quota.config.connect_deepseek(connected.slot.clone(),connected.label)};
                if let Some(old)=old.filter(|a|a.slot.as_deref()!=Some(&connected.slot)){quota::deepseek::forget(&old);}
                self.quota.connect_target=None;self.quota.stale.remove(&id);self.quota.rows.insert(id,Ok(connected.snapshot));self.quota.save_snapshots();self.quota.config.save();self.quota.note="DeepSeek 已連線".into();self.pending.push(Cmd::QuotaConnected);},Err(error)=>self.quota.note=error}
        }
        let result=self.quota.gemini_connecting.as_ref().and_then(|connect|connect.poll());
        if let Some(result)=result{
            self.quota.gemini_connecting=None;
            if self.settings_open&&self.quota.connect_open{
                match result.and_then(|v|v.finish()){Ok(connected)=>{
                    let previous=self.quota.config.clone();let id=self.quota.config.connect_gemini(connected.slot,connected.label);
                    if let Err(error)=self.quota.config.save_checked(){self.quota.config=previous;self.quota.note=error;}
                    else{self.quota.rows.insert(id.clone(),Ok(connected.snapshot));self.quota.stale.remove(&id);self.quota.save_snapshots();self.quota.connect_target=None;self.quota.gemini_cookie.clear();self.quota.note="Gemini 已連線".into();self.pending.push(Cmd::QuotaConnected);}
                },Err(error)=>self.quota.note=error}
            }
        }
        if let Some(result)=self.quota.doubao_connecting.as_ref().and_then(|connect|connect.poll()){
            self.quota.doubao_connecting=None;
            if self.settings_open&&self.quota.connect_open{
                let target=self.quota.connect_target.clone();
                match result.and_then(|validated|validated.finish(&mut self.quota.config,target.as_deref())){
                    Ok(connected)=>{
                        self.quota.stale.remove(&connected.id);self.quota.busy.remove(&connected.id);self.quota.rows.insert(connected.id,Ok(connected.snapshot));
                        self.quota.save_snapshots();self.quota.connect_target=None;self.quota.doubao_cookie.clear();
                        self.quota.note=connected.cleanup_error.map(|error|format!("豆包已連線；{error}")).unwrap_or_else(||"豆包已連線".into());self.pending.push(Cmd::QuotaConnected);
                    },Err(error)=>self.quota.note=error
                }
            }
        }
        let result=self.quota.login.as_ref().and_then(|login|login.poll());
        if let Some(result)=result{
            self.quota.login=None;
            match result{Ok(connected)=>{self.quota.config.connect_grok(connected.slot,connected.label);self.quota.config.save();self.quota.note="Grok Bot 已連線".into();self.pending.push(Cmd::QuotaConnected);},Err(error)=>self.quota.note=error}
        }
    }
    fn quota_login(&mut self,alias:String){
        if self.quota.login.is_some(){return;}
        match quota::grok::Login::start(alias){Ok((login,url))=>{self.quota.login=Some(login);self.quota.note="請在瀏覽器中完成登入，帳號會自動新增".into();self.pending.push(Cmd::OpenUrl(url));},Err(error)=>self.quota.note=error}
    }
    fn quota_start_kiro(&mut self,path:std::path::PathBuf){
        if self.quota.kiro_connecting.is_some(){return;}
        let target=self.quota.connect_target.as_ref().and_then(|id|self.quota.config.accounts.iter().find(|a|&a.id==id)).cloned();
        if self.quota.connect_target.is_some()&&target.is_none(){self.quota.note="原帳號已移除，請重新新增".into();return;}
        self.quota.note.clear();self.quota.kiro_connecting=Some(quota::kiro::Connect::start(path,self.quota.alias.clone(),target));
    }
    pub fn quota_config(&self)->&Config{&self.quota.config}
    pub fn quota_initialize(&mut self){if !crate::model::config_dir().join("ai-quota.json").exists(){self.quota.config.discover();self.quota.config.save();}}
    pub fn quota_pending(&mut self,accounts:&[Account]){self.quota.busy.extend(accounts.iter().map(|a|a.id.clone()));}
    pub fn quota_event(&mut self,event:quota::Event){self.quota.receive(event);}
    pub fn quota_set_source(&mut self,path:std::path::PathBuf){self.quota.source=path.to_string_lossy().into_owned();}
    pub(crate) fn quota_bar_indices(&self)->Vec<usize>{
        if !self.quota.config.enabled || !self.quota.config.compact || !self.settings.visible_pages[6] || self.settings.minimal || self.t<self.notification_bell_until{return vec![];}
        self.quota.config.pinned_indices()
    }
    /// 常駐膠囊裡每個額度槽的實際寬度：圖示右側跟數字的帳號需要額外佔位。
    pub(crate) fn quota_balance_layout(&self,ctx:&egui::Context)->Vec<(usize,f32)>{
        self.quota_bar_indices().iter().enumerate().filter_map(|(position,index)|{
            let account=&self.quota.config.accounts[*index];
            let extra=if let Some(text)=self.quota.tail_text(&account.id){ctx.fonts(|fonts|fonts.layout_no_wrap(text,theme::font(theme::size::PILL_CLOCK,W::Bold),theme::WHITE).size().x).ceil()+6.0}
                else if account.provider.balance()&&self.quota.balance_loading(&account.id){22.0}
                else{return None;};
            Some((position,extra))
        }).collect()
    }
    pub(crate) fn draw_quota_bar(&mut self,c:&Card,cx:f32,layout:&crate::bar_layout::Layout){
        let indices=self.quota_bar_indices();let mut chosen=None;
        for (position,i)in indices.iter().enumerate(){
            let alpha=layout.pet_clearance(crate::bar_layout::quota_slot(position));
            let faded=c.faded(alpha);let faded=if alpha<0.8{faded.ghost()}else{faded};let c=&faded;
            let a=&self.quota.config.accounts[*i];let x=cx+layout.x[crate::bar_layout::quota_slot(position)].unwrap();
            let snapshot=self.quota.rows.get(&a.id).and_then(|r|r.as_ref().ok());
            if a.provider.balance(){
                let wide=true;let extra=if wide{layout.widths[crate::bar_layout::quota_slot(position)]-27.0}else{0.0};let icon_x=x-extra/2.0;
                if a.provider!=Provider::DeepSeek{logo(c,a.provider,icon_x,15.0,22.0);}else{deepseek_badge(c,self.settings.skin,icon_x,15.0,24.0);}
                let loading=self.quota.balance_loading(&a.id);let stale=self.quota.stale.contains_key(&a.id);
                if stale||(!loading&&snapshot.is_none()){
                    c.circle(icon_x+9.0,24.0,3.8,Color32::BLACK);
                    c.circle(icon_x+9.0,24.0,2.5,Color32::from_rgb(240,165,84));
                }
                if wide{
                    if let Some(text)=self.quota.balance_text(&a.id){
                        let wallet=snapshot.and_then(|s|s.wallet.as_ref());
                        let color=if stale{theme::GRAY}else if wallet.is_some_and(|w|!w.available||w.primary().total<=0.0){Color32::from_rgb(255,166,77)}else{theme::WHITE};
                        c.text(icon_x+18.0,21.0,&text,theme::size::PILL_CLOCK,W::Bold,color,H::Left);
                    }else if loading{
                        let active=(self.t*3.0) as usize%3;
                        for dot in 0..3{c.circle(icon_x+20.0+dot as f32*5.0,15.0,1.65,theme::GRAY.gamma_multiply(if dot==active{1.0}else{0.35}));}
                        c.ui.ctx().request_repaint_after(std::time::Duration::from_millis(160));
                    }
                }
                if c.hit(x-(27.0+extra)/2.0,1.0,27.0+extra,28.0,("quota-pin",&a.id)).clicked(){chosen=Some(a.id.clone());}continue;
            }
            // 有具體點數的額度（如 Kiro）在環右側跟一段剩餘點數，與餘額類的排布一致。
            let extra=(layout.widths[crate::bar_layout::quota_slot(position)]-27.0).max(0.0);
            let x=if extra>0.0{x-extra/2.0}else{x};
            ring(c,a,snapshot,x,15.0,27.0,accent_colors(&self.quota.config,a));
            if extra>0.0{let stale=self.quota.stale.contains_key(&a.id);if let Some(text)=self.quota.tail_text(&a.id){c.text(x+15.5,21.0,&text,theme::size::PILL_CLOCK,W::Bold,if stale{theme::GRAY}else{theme::WHITE},H::Left);}}
            let mut same=self.quota.config.accounts.iter().filter(|other|other.provider==a.provider && other.enabled);
            if same.clone().count()>1{let n=same.position(|other|other.id==a.id).unwrap_or(0)+1;c.circle(x+10.0,24.0,4.5,Color32::from_gray(225));c.text(x+10.0,26.8,&n.to_string(),6.5,W::Bold,Color32::BLACK,H::Center);}
            let summary=snapshot.map(|s|s.rings().iter().map(|w|w.summary()).collect::<Vec<_>>().join(" · ")).unwrap_or_else(||if self.quota.busy.contains(&a.id){"正在更新".into()}else{"用量暫不可用".into()});
            if bar_tooltip(c.hit(x-14.0,1.0,28.0+extra,28.0,("quota-pin",&a.id)),format!("{} · {}\n{}",a.provider.name(),a.alias,summary)).clicked(){chosen=Some(a.id.clone());}
        }
        if let Some(id)=chosen{self.quota.config.selected=id;self.quota.config.save();self.show_page(6);}
    }
    pub(crate) fn page_quota(&mut self,c:&Card){
        if c.hit(303.0,8.0,32.0,21.0,"quota-refresh").on_hover_text("重新整理用量").clicked(){self.pending.push(Cmd::QuotaRefresh);}
        c.text(319.0,23.0,if self.quota.busy.is_empty(){"重新整理"}else{"更新中"},10.5,W::Regular,theme::GRAY,H::Center);
        let Some(selected)=self.quota.config.selected_index() else{
            c.text(43.0,61.0,"還沒有新增 AI 帳號",13.0,W::Bold,theme::WHITE,H::Left);
            c.text(43.0,80.0,"連線 AI 帳號，檢視剩餘額度",10.5,W::Regular,theme::GRAY,H::Left);
            let(r,b)=crate::paint::capsule(c,43.0,92.0,94.0,24.0,"add-quota-account",theme::BTN,theme::BTN_HOVER);b.text(90.0,108.0,"新增帳號",10.0,W::Semibold,theme::WHITE,H::Center);if r.clicked(){self.settings_tab=11;self.open_settings();}return;
        };
        let indices=self.quota.config.ordered_indices(false);
        let pos=indices.iter().position(|i|*i==selected).unwrap_or(0);let start=pos/3*3;let mut picked=None;
        if indices.len()>3{for (x,offset,label)in [(42.0,-1isize,"‹"),(357.0,1,"›")]{if c.hit(x-7.0,30.0,18.0,19.0,("quota-account-next",offset)).clicked(){picked=Some(indices[(pos as isize+offset).rem_euclid(indices.len() as isize) as usize]);}c.text(x+2.0,44.0,label,13.0,W::Regular,theme::GRAY,H::Center);}}
        for (column,index)in indices.iter().skip(start).take(3).enumerate(){let a=&self.quota.config.accounts[*index];let x=59.0+column as f32*99.0;let on=*index==selected;let colors=accent_colors(&self.quota.config,a);let color=colors[0];let(r,b)=crate::paint::capsule(c,x,30.0,94.0,21.0,&format!("quota-account-{}",a.id),if on{Color32::from_rgb(color.r()/5,color.g()/5,color.b()/5)}else{Color32::BLACK},theme::BTN_HOVER);let text=b.fit(&a.alias,10.5,W::Regular,84.0);b.text(x+47.0,45.0,&text,10.5,W::Regular,if on{colors[1]}else{theme::GRAY},H::Center);if r.on_hover_text(format!("{} · {}",a.provider.name(),a.alias)).clicked(){picked=Some(*index);}}
        if let Some(i)=picked{self.quota.config.selected=self.quota.config.accounts[i].id.clone();self.quota.config.save();}
        let a=&self.quota.config.accounts[selected];let result=self.quota.rows.get(&a.id);let snapshot=result.and_then(|r|r.as_ref().ok());
        let colors=accent_colors(&self.quota.config,a);
        if let Some(s)=snapshot.filter(|s|s.at>0){
            if let Some(at)=chrono::DateTime::from_timestamp(s.at,0){
                let at=at.with_timezone(&chrono::Local);let time=at.format(if at.date_naive()==chrono::Local::now().date_naive(){"%H:%M"}else{"%m/%d %H:%M"}).to_string();
                let busy=self.quota.busy.contains(&a.id);let stale=self.quota.stale.get(&a.id);
                let status=if busy{format!("{} · {time}",if a.provider==Provider::Claude{"讀取中…"}else{"正在更新…"})}
                    else if stale.is_some(){format!("上次記錄 · {time}")}
                    else if a.provider==Provider::Claude{format!("本地記錄 · {time}")}
                    else if let Some(wallet)=&s.wallet{let b=wallet.primary();if wallet.balances.len()>1{let other=wallet.balances.iter().find(|other|other.currency!=b.currency).unwrap();format!("另有 {} {} · {time}",other.currency,other.full(other.total))}else{format!("{} · {time} 更新",b.currency)}}
                    else{format!("{time} 更新")};
                let x=if a.provider.balance(){125.0}else{157.0};let width=357.0-x;let y=154.0;
                c.centered_text(x,y,&c.fit(&status,10.0,W::Regular,width),10.0,W::Regular,if stale.is_some()&&!busy{Color32::from_rgb(255,166,77)}else{theme::DIM},H::Left);
                c.hit(x,y-6.0,width,8.0,("quota-update-time",&a.id)).on_hover_text(if let Some(reason)=stale{format!("{status}\n{reason}")}else{status});
            }
        }
        if a.provider==Provider::WorkBuddy{
            logo(c,a.provider,85.0,105.0,44.0);
            if let Some(credits)=snapshot.and_then(|s|s.credits.as_ref()){
                c.text(125.0,81.0,"可用積分",10.5,W::Regular,theme::GRAY,H::Left);
                c.text(125.0,107.0,&credits.full(),23.0,W::Semibold,theme::WHITE,H::Left);
                let detail=if let Some(reason)=self.quota.stale.get(&a.id){reason.clone()}else{credits.packages.iter().map(|(label,n)|format!("{label} {n}")).collect::<Vec<_>>().join(" · ")};c.text(125.0,131.0,&c.fit(&detail,10.0,W::Regular,225.0),10.0,W::Regular,theme::GRAY,H::Left);
            }else{let text=result.and_then(|r|r.as_ref().err()).map(String::as_str).unwrap_or("正在查詢積分…");for(i,line)in c.wrap(text,11.0,W::Regular,226.0,3).iter().enumerate(){c.text(125.0,92.0+i as f32*17.0,line,11.0,W::Regular,theme::GRAY,H::Left);}}
            return;
        }
        if a.provider==Provider::DeepSeek{
            deepseek_badge(c,self.settings.skin,85.0,105.0,50.0);
            if let Some(s)=snapshot{
                if let Some(wallet)=&s.wallet{
                    let b=wallet.primary();
                    c.text(125.0,81.0,if wallet.available{"可用餘額"}else{"餘額不可用"},10.5,W::Regular,theme::GRAY,H::Left);
                    let amount=b.full(b.total);let size=(23.0*145.0/c.text_w(&amount,23.0,W::Semibold).max(145.0)).min(23.0);
                    c.text(125.0,109.0,&amount,size,W::Semibold,if !wallet.available{Color32::from_rgb(255,166,77)}else{theme::WHITE},H::Left);
                    for (y,label,value) in [(90.0,"充值",b.topped_up),(118.0,"贈送",b.granted)]{
                        c.text(281.0,y,label,10.0,W::Regular,theme::GRAY,H::Left);
                        let text=b.full(value);let size=10.0*54.0/c.text_w(&text,10.0,W::Semibold).max(54.0);c.text(357.0,y,&text,size,W::Semibold,theme::WHITE,H::Right);
                    }
                }
            }else{let text=if self.quota.busy.contains(&a.id){"正在更新餘額…"}else{result.and_then(|r|r.as_ref().err()).map(String::as_str).unwrap_or("等待更新餘額")};for (i,line) in c.wrap(text,11.0,W::Regular,226.0,3).iter().enumerate(){c.text(125.0,92.0+i as f32*17.0,line,11.0,W::Regular,theme::GRAY,H::Left);}}
            return;
        }
        if a.provider.relay(){
            logo(c,a.provider,85.0,105.0,44.0);
            if let Some(q)=snapshot.and_then(|s|s.relay.as_ref()){
                if let Some(amount)=q.primary(){
                    let exhausted=amount.remaining.is_some_and(|n|n<=0.0)&&!amount.unlimited;
                    let title=if matches!(amount.scope,quota::relay::Scope::Account){if amount.label=="賬戶餘額"{"賬戶餘額".into()}else{"賬戶剩餘額度".into()}}else if amount.label=="剩餘額度"{"剩餘額度".into()}else{format!("{}剩餘",amount.label)};
                    c.text(125.0,78.0,&c.fit(&title,11.0,W::Semibold,176.0),11.0,W::Semibold,theme::GRAY,H::Left);
                    let value=amount.remaining_text();let size=23.0*(225.0/c.text_w(&value,23.0,W::Semibold).max(225.0));
                    c.text(125.0,109.0,&value,size,W::Semibold,if exhausted{Color32::from_rgb(255,166,77)}else{theme::WHITE},H::Left);
                    let detail=if exhausted{format!("{}已用盡",amount.label)}else if matches!(amount.scope,quota::relay::Scope::Subscription){
                        let reset=snapshot.and_then(|s|s.windows.iter().find(|w|w.label==amount.label)).and_then(|w|w.reset);
                        format!("{} · {}",q.source,reset_text(reset,&chrono::Local::now()))
                    }else{format!("{} · 以站點返回單位為準",q.source)};
                    c.text(125.0,132.0,&c.fit(&detail,10.5,W::Regular,225.0),10.5,W::Regular,if exhausted{Color32::from_rgb(255,166,77)}else{theme::GRAY},H::Left);
                }else{
                    c.text(125.0,78.0,"賬戶餘額與套餐用量",11.0,W::Semibold,theme::GRAY,H::Left);
                    c.text(125.0,109.0,q.unavailable_text(),18.0,W::Semibold,theme::WHITE,H::Left);
                    c.text(125.0,132.0,if a.provider==Provider::NewApi{"請檢查查詢模板或賬戶查詢許可權"}else{"請到站點檢視賬戶餘額或套餐"},10.5,W::Regular,theme::GRAY,H::Left);
                }
                if c.hit(310.0,60.0,47.0,22.0,("relay-details",&a.id)).on_hover_text("檢視全部週期與用量").clicked(){
                    self.quota.detail_account=Some(a.id.clone());self.settings_tab=11;self.open_settings();
                }
                c.text(355.0,77.0,"明細 ›",10.0,W::Regular,theme::GRAY,H::Right);
            }else{
                let text=result.and_then(|r|r.as_ref().err()).map(String::as_str).unwrap_or("等待更新用量");
                for(i,line)in c.wrap(text,11.0,W::Regular,226.0,3).iter().enumerate(){c.text(125.0,99.0+i as f32*17.0,line,11.0,W::Regular,theme::GRAY,H::Left);}
            }
            return;
        }
        if a.provider.api_key()||a.provider==Provider::Doubao{
            let label=match snapshot.map(|s|s.plan.as_str()).filter(|s|!s.is_empty()){Some(plan)=>format!("{} · {plan}",a.provider.name()),None=>a.provider.name().into()};
            c.text(157.0,64.0,&c.fit(&label,12.0,W::Semibold,145.0),12.0,W::Semibold,theme::WHITE,H::Left);
            if let Some(snapshot)=snapshot{
                let mut main=snapshot.windows.iter().collect::<Vec<_>>();main.sort_by(|a,b|a.remaining.total_cmp(&b.remaining));
                let meter=Snapshot{windows:main.iter().take(2).map(|w|(**w).clone()).collect(),..Default::default()};
                ring_with(c,a,Some(&meter),103.0,105.0,70.0,colors,true);
                let displayed=meter.rings();
                let mut rows=snapshot.windows.iter().chain(snapshot.extra.iter()).take(3).collect::<Vec<_>>();
                if let Some(limit)=main.first(){
                    if rows.len()==3&&!rows.contains(limit){rows[2]=limit;}
                    let exhausted=limit.remaining<=0.0;
                    let label=if exhausted{format!("{}已用盡",limit.label)}else{format!("{}剩餘",limit.label)};
                    c.text(103.0,153.0,&c.fit(&label,9.0,W::Regular,96.0),9.0,W::Regular,if exhausted{Color32::from_rgb(255,166,77)}else{theme::GRAY},H::Center);
                }
                let now=chrono::Local::now();
                for (i,w) in rows.iter().enumerate(){
                    let y=83.0+i as f32*23.0;let color=if w.remaining<=0.0{Color32::from_rgb(255,166,77)}else{displayed.iter().position(|v|**v==**w).map(|slot|colors[slot]).unwrap_or(theme::GRAY)};
                    c.circle(159.0,y-4.0,2.5,color);
                    c.text(168.0,y,&c.fit(&w.label,11.0,W::Semibold,114.0),11.0,W::Semibold,theme::WHITE,H::Left);
                    c.text(355.0,y,&format!("{:.0}%",w.remaining),17.0,W::Semibold,if w.remaining<=0.0{color}else{theme::WHITE},H::Right);
                    c.text(168.0,y+12.0,&c.fit(&reset_text(w.reset,&now),9.0,W::Regular,187.0),9.0,W::Regular,theme::GRAY,H::Left);
                }
                let details=c.hit(309.0,51.0,48.0,20.0,("api-quota-details",&a.id)).on_hover_text(snapshot_details(snapshot)).clicked();
                c.text(355.0,64.0,"明細 ›",10.0,W::Regular,theme::GRAY,H::Right);
                if details{self.quota.detail_account=Some(a.id.clone());self.settings_tab=11;self.open_settings();}
            }else{
                logo(c,a.provider,103.0,105.0,40.0);
                let text=result.and_then(|r|r.as_ref().err()).map(String::as_str).unwrap_or("等待更新用量");
                for(i,line)in c.wrap(text,11.0,W::Regular,197.0,3).iter().enumerate(){c.text(157.0,96.0+i as f32*17.0,line,11.0,W::Regular,theme::GRAY,H::Left);}
            }
            return;
        }
        ring_with(c,a,snapshot,103.0,105.0,70.0,colors,true);
        let raw_plan=snapshot.map(|s|s.plan.as_str()).unwrap_or_default();let plan=match raw_plan{"pro"|"prolite"|"pro_lite"=>"Pro","plus"=>"Plus","free"=>"Free","team"|"business"=>"Business","enterprise"=>"Enterprise",other=>other};let label=if a.provider==Provider::Claude{"Claude · 本地記錄".into()}else if plan.is_empty(){a.provider.name().to_owned()}else{format!("{} · {}",a.provider.name(),plan)};
        c.text(157.0,64.0,&c.fit(&label,12.0,W::Semibold,145.0),12.0,W::Semibold,theme::WHITE,H::Left);
        let mut open_details=false;
        if let Some(s)=snapshot{
            let windows=s.rings();let now=chrono::Local::now();
            let has_details=!s.extra.is_empty()||s.windows.len()>windows.len();
            if has_details{
                let open=c.hit(309.0,51.0,48.0,20.0,("quota-details",&a.id)).on_hover_text(snapshot_details(s)).clicked();
                c.text(355.0,64.0,"明細 ›",10.0,W::Regular,theme::GRAY,H::Right);
                open_details=open;
            }
            if windows.is_empty() && a.provider==Provider::ZCode{
                c.text(157.0,104.0,"官網未分配體驗額度",11.0,W::Regular,theme::GRAY,H::Left);
            }else if windows.len()==1{
                let w=windows[0];
                // 百分比在右側時，左側標籤與重置時間只能用到百分比左邊，避免長標籤或跨年日期撞字。
                let text_width=if w.amount.is_some(){197.0}else{355.0-157.0-c.text_w(&w.value_text(),28.0,W::Semibold)-10.0};
                c.text(157.0,103.0,&c.fit(&format!("{}剩餘",w.label),12.0,W::Semibold,text_width),12.0,W::Semibold,theme::WHITE,H::Left);
                if let Some(amount)=&w.amount{let text=format!("{} 點",amount.full());let available=355.0-157.0-c.text_w(&format!("{}剩餘",w.label),12.0,W::Semibold)-8.0;let size=12.5*(available/c.text_w(&text,12.5,W::Semibold).max(available));c.text(355.0,103.0,&text,size,W::Semibold,theme::WHITE,H::Right);}else{c.centered_text(355.0,107.0,&w.value_text(),28.0,W::Semibold,theme::WHITE,H::Right);}
                let reset=quota_reset_text(s,w,&now);c.text(157.0,124.0,&c.fit(&reset,11.0,W::Regular,text_width),11.0,W::Regular,theme::GRAY,H::Left);
            }else{
                if !has_details{c.text(355.0,64.0,"剩餘",10.5,W::Regular,theme::GRAY,H::Right);}
                for (i,w)in windows.iter().enumerate(){
                    let y=86.0+i as f32*35.0;c.circle(159.0,y-4.0,2.5,colors[i]);
                    let value_width=c.text_w(&w.value_text(),if w.amount.is_some(){13.0}else{22.0},W::Semibold);
                    c.text(168.0,y,&c.fit(&w.label,11.5,W::Semibold,355.0-168.0-value_width-8.0),11.5,W::Semibold,theme::WHITE,H::Left);
                    let reset=quota_reset_text(s,w,&now);
                    c.text(168.0,y+17.0,&c.fit(&reset,11.0,W::Regular,185.0),11.0,W::Regular,theme::GRAY,H::Left);
                    c.centered_text(355.0,y+4.0,&w.value_text(),if w.amount.is_some(){13.0}else{22.0},W::Semibold,theme::WHITE,H::Right);
                }
            }
        }else{let message=if self.quota.busy.contains(&a.id){"正在更新用量…"}else{result.and_then(|r|r.as_ref().err()).map(String::as_str).unwrap_or("等待更新用量")};for(i,line)in c.wrap(message,11.0,W::Regular,197.0,2).iter().enumerate(){c.text(157.0,103.0+i as f32*17.0,line,11.0,W::Regular,theme::GRAY,H::Left);}}
        if open_details{self.quota.detail_account=Some(a.id.clone());self.settings_tab=11;self.open_settings();}
    }
    fn quota_open_connection(&mut self,id:Option<String>){
        if self.quota.kiro_connecting.is_some()||self.quota.api_connecting.is_some()||self.quota.workbuddy_connecting.is_some()||self.quota.deepseek_connecting.is_some()||self.quota.gemini_connecting.is_some()||self.quota.doubao_connecting.is_some()||self.quota.login.is_some(){self.quota.connect_open=true;return;}
        self.quota.note.clear();self.quota.api_key.clear();self.quota.deepseek_key.clear();self.quota.gemini_cookie.clear();self.quota.doubao_cookie.clear();self.quota.source.clear();self.quota.alias.clear();
        self.quota.gemini_email.clear();
        self.quota.relay_manual=id.is_some();
        self.quota.relay_user_token=false;self.quota.relay_user_id.clear();
        self.quota.relay_script.clear();self.quota.relay_access_token.clear();
        if let Some(a)=id.as_ref().and_then(|id|self.quota.config.accounts.iter().find(|a|&a.id==id)){
            self.quota.provider=a.provider;self.quota.alias=a.alias.clone();self.quota.source=a.source.to_string_lossy().into();
            if let Some(user_id)=quota::api::new_api_user_id(a){self.quota.relay_user_token=true;self.quota.relay_user_id=user_id;}
            if let Some(c)=quota::api::custom_credentials(a){self.quota.relay_script=c.script;self.quota.api_key=c.api_key;self.quota.relay_access_token=c.access_token;self.quota.relay_user_id=c.user_id;}
            if a.provider==Provider::Gemini{self.quota.gemini_email=quota::gemini::address(a).unwrap_or_default();}
        }
        if self.quota.provider==Provider::RelayCustom&&self.quota.relay_script.is_empty(){self.quota.relay_script=quota::relay::script::DEFAULT_SCRIPT.into();}
        self.quota.connect_target=id;self.quota.connect_open=true;
    }
    fn quota_remove_account(&mut self,id:&str){
        if let Some(a)=self.quota.config.accounts.iter().find(|a|a.id==id&&(a.provider.api_key()||a.provider==Provider::Doubao)).cloned(){
            if a.provider.api_key(){if let Some(slot)=&a.slot{if let Err(error)=quota::api::queue_cleanup(slot){self.quota.note=error;return;}}}
            let mut candidate=self.quota.config.clone();candidate.accounts.retain(|a|a.id!=id);candidate.colors.remove(id);
            if candidate.selected==id{candidate.selected=candidate.accounts.iter().find(|a|a.enabled).map(|a|a.id.clone()).unwrap_or_default();}
            if let Err(error)=candidate.save_checked(){self.quota.note=error;return;}
            self.quota.config=candidate;self.quota.rows.remove(id);self.quota.stale.remove(id);self.quota.busy.remove(id);self.quota.save_snapshots();
            self.quota.note=match if a.provider==Provider::Doubao{quota::doubao::forget(&a)}else{quota::api::cleanup_pending(&self.quota.config)}{Ok(())=>"帳號已移除".into(),Err(error)=>error};return;
        }
        if let Some(a)=self.quota.config.accounts.iter().find(|a|a.id==id){match a.provider{Provider::GrokBot=>quota::grok::forget(a),Provider::Gemini=>quota::gemini::forget(a),Provider::DeepSeek=>quota::deepseek::forget(a),Provider::WorkBuddy=>quota::workbuddy::forget(a),_=>{}}}
        self.quota.config.accounts.retain(|a|a.id!=id);self.quota.config.colors.remove(id);self.quota.rows.remove(id);self.quota.stale.remove(id);self.quota.busy.remove(id);self.quota.save_snapshots();
        if self.quota.config.selected==id{self.quota.config.selected=self.quota.config.accounts.first().map(|a|a.id.clone()).unwrap_or_default();}
    }
    pub(crate) fn quota_settings(&mut self,ui:&mut egui::Ui){
        let before=self.quota.config.clone();
        settings_card(ui,|ui|{
            setting_row(ui,"開啟 AI 用量","每 3 分鐘自動更新",&mut self.quota.config.enabled);
            ui.separator();
            setting_row(ui,"收起時顯示常駐帳號","WorkBuddy 積分在前，DeepSeek 餘額在最後",&mut self.quota.config.compact);
        });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui|{
            if ui.button("＋ 新增帳號").clicked(){self.quota_open_connection(None);}
            if ui.button("識別本機帳號").clicked(){self.quota.note=self.quota.config.discover().message();}
            if ui.add_enabled(self.quota.busy.is_empty(),egui::Button::new("重新整理用量")).clicked(){self.pending.push(Cmd::QuotaRefresh);}
        });ui.add_space(8.0);
        let accounts=self.quota.config.accounts.clone();let mut remove=None;let mut connect=None;let mut reorder=None;
        for mut a in accounts{
            let fill=crate::settings_theme::color(ui.ctx(),Color32::from_rgb(35,35,38),Color32::WHITE);
            let row=egui::Frame::new().fill(fill).corner_radius(10).inner_margin(egui::Margin::symmetric(16,12)).show(ui,|ui|{
                ui.set_width((ui.available_width()-1.0).max(0.0));ui.spacing_mut().item_spacing.y=6.0;
                ui.push_id(a.id.clone(),|ui|{
                    ui.horizontal(|ui|{
                        ui.add(egui::Label::new("::").sense(egui::Sense::drag())).on_hover_text("拖動調整帳號順序").dnd_set_drag_payload(a.id.clone());
                        let (rect,_)=ui.allocate_exact_size(egui::vec2(26.0,28.0),egui::Sense::hover());let c=Card::new(ui,rect.min,ui.clip_rect());
                        if a.provider==Provider::DeepSeek{deepseek_badge(&c,if ui.visuals().dark_mode{crate::model::Skin::Classic}else{crate::model::Skin::GlassLight},13.0,14.0,26.0);}else if matches!(a.provider,Provider::ZCode|Provider::Kiro|Provider::Codex){logo(&c,a.provider,13.0,14.0,26.0);}else{c.circle(13.0,14.0,13.0,crate::paint::mix(accent_colors(&self.quota.config,&a)[0],Color32::BLACK,0.75));logo(&c,a.provider,13.0,14.0,20.0);}
                        ui.label(egui::RichText::new(if a.provider.relay(){"中轉站"}else{a.provider.name()}).strong());
                        // 從右往左排：按鈕先靠右放好，名稱輸入框佔用剩餘寬度，按鈕再寬也不會頂出卡片。
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                            if ui.add_enabled(self.quota.api_connecting.is_none()&&self.quota.workbuddy_connecting.is_none()&&self.quota.deepseek_connecting.is_none()&&self.quota.gemini_connecting.is_none()&&self.quota.doubao_connecting.is_none()&&self.quota.login.is_none(),egui::Button::new("移除")).clicked(){remove=Some(a.id.clone());}
                            if ui.selectable_label(self.quota.config.selected==a.id,"當前").clicked(){self.quota.config.selected=a.id.clone();}
                            ui.add_sized([ui.available_width().max(55.0),26.0],egui::TextEdit::singleline(&mut a.alias).hint_text("帳號名稱")).on_hover_text("直接修改帳號名稱");
                        });
                    });
                    let result=self.quota.rows.get(&a.id);let snapshot=result.and_then(|r|r.as_ref().ok());let error=self.quota.stale.get(&a.id).or_else(||result.and_then(|r|r.as_ref().err()));
                    if let Some(s)=snapshot{
                        if a.provider.api_key()||a.provider==Provider::Doubao{ui.label(snapshot_details(s));}else if let Some(credits)=&s.credits{ui.horizontal_wrapped(|ui|{ui.label(egui::RichText::new("可用積分").size(11.0).color(ui.visuals().weak_text_color()));ui.label(egui::RichText::new(credits.full()).size(20.0).strong());ui.label(egui::RichText::new(credits.packages.iter().map(|(label,n)|format!("{label} {n}")).collect::<Vec<_>>().join(" · ")).size(11.0).color(ui.visuals().weak_text_color()));});}else if let Some(wallet)=&s.wallet{
                            ui.horizontal_wrapped(|ui|{
                                ui.label(egui::RichText::new(if wallet.available{"可用餘額"}else{"餘額不可用"}).size(11.0).color(ui.visuals().weak_text_color()));
                                ui.label(egui::RichText::new(wallet.primary().full(wallet.primary().total)).size(20.0).strong());
                                ui.label(egui::RichText::new(format!("充值 {} · 贈送 {}",wallet.primary().full(wallet.primary().topped_up),wallet.primary().full(wallet.primary().granted))).size(11.0).color(ui.visuals().weak_text_color()));
                            });
                        }else if a.provider==Provider::ZCode && s.windows.is_empty(){
                            ui.label("已連線 · 暫無體驗額度");
                        }else if let Some(details)=&s.kiro{
                            if details.pools.len()>1{if let Some(window)=s.windows.first(){ui.label(format!("{}：{} 點",window.label,window.value_text()));}}
                            ui.label(egui::RichText::new(kiro_details(s)).size(11.0));
                        }else{
                            let windows=s.rings();let count=windows.len().max(1);let width=(ui.available_width()-12.0*(count-1) as f32)/count as f32;
                            ui.horizontal_top(|ui|{ui.spacing_mut().item_spacing.x=12.0;for window in windows{ui.allocate_ui_with_layout(egui::vec2(width,30.0),egui::Layout::top_down(egui::Align::Min),|ui|{
                                ui.horizontal(|ui|{ui.label(egui::RichText::new(format!("{}  剩餘 {}",window.label,window.value_text())).size(11.0));});
                                ui.add(egui::ProgressBar::new((window.remaining/100.0).clamp(0.0,1.0)).desired_width(width).desired_height(3.0).fill(accent_colors(&self.quota.config,&a)[0]));
                            });}});
                            if !s.extra.is_empty(){
                                let now=chrono::Local::now();
                                let lines=s.extra.iter().map(|w|format!("{}  剩餘 {} · {}",w.label,w.value_text(),reset_text(w.reset,&now))).collect::<Vec<_>>().join("
");
                                ui.label(egui::RichText::new(lines).size(11.0).color(ui.visuals().weak_text_color()));
                            }
                        }
                    }
                    if !a.enabled{ui.label(egui::RichText::new("已暫停監控").size(11.0).color(ui.visuals().weak_text_color()));}
                    else if let Some(error)=error{ui.add(egui::Label::new(egui::RichText::new(error).size(11.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_rgb(231,175,108),Color32::from_rgb(151,94,28)))).wrap());}
                    else if snapshot.is_none(){ui.label(egui::RichText::new(if self.quota.busy.contains(&a.id){"正在查詢用量…"}else{"等待更新"}).size(11.0).color(ui.visuals().weak_text_color()));}
                    ui.horizontal_wrapped(|ui|{
                        ui.checkbox(&mut a.enabled,"監控");ui.checkbox(&mut a.pinned,"常駐島上");
                        if ui.add_enabled(!self.quota.busy.contains(&a.id),egui::Button::new(if error.is_some(){"重試"}else{"重新整理"})).clicked(){self.pending.push(Cmd::QuotaRefresh);}
                        if ui.button("重新連線").clicked(){connect=Some(a.id.clone());}
                        if let Some(s)=snapshot{let time=chrono::DateTime::from_timestamp(s.at,0).map(|at|at.with_timezone(&chrono::Local).format(if a.provider==Provider::Claude{"%m-%d %H:%M 本地記錄"}else{"%H:%M 更新"}).to_string()).unwrap_or_default();ui.label(egui::RichText::new(time).size(10.0).color(ui.visuals().weak_text_color()));}
                    });
                    if !a.provider.balance(){
                        let mut color=self.quota.config.colors.get(&a.id).copied().unwrap_or_default();color_picker(ui,a.provider,&mut color);
                        if color==Accent::Brand{self.quota.config.colors.remove(&a.id);}else{self.quota.config.colors.insert(a.id.clone(),color);}
                    }
                    if a.provider==Provider::Claude{
                        ui.separator();ui.label(egui::RichText::new("本地讀取說明").size(12.0).strong());
                        ui.add(egui::Label::new(egui::RichText::new(CLAUDE_LOCAL_NOTICE).size(12.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_gray(180),Color32::from_gray(75)))).wrap());
                        claude_disclaimer(ui);
                    }
                });
            });
            if self.quota.detail_account.as_deref()==Some(a.id.as_str()){
                row.response.scroll_to_me(Some(egui::Align::Min));self.quota.detail_account=None;
            }
            if row.response.dnd_hover_payload::<String>().is_some(){ui.painter().rect_stroke(row.response.rect,10,egui::Stroke::new(2.0,ui.visuals().selection.bg_fill),egui::StrokeKind::Inside);}
            if let Some(from)=row.response.dnd_release_payload::<String>(){reorder=Some(((*from).clone(),a.id.clone()));}
            if let Some(account)=self.quota.config.accounts.iter_mut().find(|account|account.id==a.id){*account=a;}
            ui.add_space(8.0);
        }
        if let Some((from,to))=reorder{self.quota.config.move_account(&from,&to);}
        if let Some(id)=remove{self.quota_remove_account(&id);}
        if let Some(id)=connect{self.quota_open_connection(Some(id));}
        if !self.quota.connect_open&&!self.quota.note.is_empty(){ui.add_space(8.0);ui.label(egui::RichText::new(&self.quota.note).size(12.0).color(ui.visuals().weak_text_color()));}
        self.quota_connect_window(ui.ctx());
        if before!=self.quota.config{self.quota.config.save();}
    }
    fn quota_connect_window(&mut self,ctx:&egui::Context){
        if !self.quota.connect_open{return;}let mut open=true;
        egui::Window::new(egui::RichText::new(if self.quota.connect_target.is_some(){"重新連線帳號"}else{"新增帳號"}).size(16.0).color(crate::settings_theme::color(ctx,Color32::from_gray(235),Color32::from_gray(35)))).id(egui::Id::new("quota-connect-window")).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO).default_width(410.0).show(ctx,|ui|{
            ui.set_max_width(410.0);ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);ui.spacing_mut().item_spacing.y=9.0;ui.visuals_mut().override_text_color=Some(crate::settings_theme::color(ctx,Color32::from_gray(225),Color32::from_gray(35)));
            if self.quota.provider==Provider::RelayCustom{ui.set_min_height((ctx.screen_rect().height()-160.0).clamp(260.0,560.0));}
            egui::ScrollArea::vertical().max_height(if self.quota.provider==Provider::RelayCustom{(ctx.screen_rect().height()-140.0).clamp(260.0,640.0)}else{460.0}).show(ui,|ui|{
            let previous_provider=self.quota.provider;
            let mut group=connection_group(self.quota.provider);
            ui.add_enabled_ui(self.quota.connect_target.is_none()&&self.quota.api_connecting.is_none()&&self.quota.kiro_connecting.is_none()&&self.quota.workbuddy_connecting.is_none()&&self.quota.deepseek_connecting.is_none()&&self.quota.gemini_connecting.is_none()&&self.quota.doubao_connecting.is_none()&&self.quota.login.is_none(),|ui|{
                settings_control_row(ui,"服務",56.0,|ui|{egui::ComboBox::from_id_salt("quota-provider").selected_text(connection_name(group)).show_ui(ui,|ui|{
                    for p in Provider::ALL.into_iter().filter(|p|connection_group(*p)==*p){ui.selectable_value(&mut group,p,connection_name(p));}
                }).response});
            });
            if group!=connection_group(previous_provider){self.quota.provider=group;}
            ui.add_enabled_ui((self.quota.connect_target.is_none()||group==Provider::NewApi)&&self.quota.api_connecting.is_none()&&self.quota.kiro_connecting.is_none(),|ui|{
                if group==Provider::ZCode{
                    let mut use_key=self.quota.provider==Provider::Glm;
                    ui.horizontal(|ui|{ui.label("連線方式");ui.selectable_value(&mut use_key,false,"本機 ZCode");ui.selectable_value(&mut use_key,true,"套餐 API Key");});
                    self.quota.provider=if use_key{Provider::Glm}else{Provider::ZCode};
                }else if group==Provider::NewApi&&!self.quota.relay_user_token{
                    ui.collapsing("高階設定",|ui|{
                        settings_control_row(ui,"站點型別",76.0,|ui|{egui::ComboBox::from_id_salt("relay-protocol").selected_text(if self.quota.relay_manual{self.quota.provider.name()}else{"自動識別（推薦）"}).show_ui(ui,|ui|{
                            ui.selectable_value(&mut self.quota.relay_manual,false,"自動識別（推薦）");
                            for p in [Provider::RelayGeneral,Provider::NewApi,Provider::Sub2Api,Provider::RelayCustom,Provider::OneApi]{if ui.selectable_label(self.quota.relay_manual&&self.quota.provider==p,p.name()).clicked(){self.quota.relay_manual=true;self.quota.provider=p;}}
                        }).response});
                    });
                }
            });
            if previous_provider!=self.quota.provider{
                self.quota.relay_user_token=false;self.quota.relay_user_id.clear();
                self.quota.relay_script=if self.quota.provider==Provider::RelayCustom{quota::relay::script::DEFAULT_SCRIPT.into()}else{String::new()};self.quota.relay_access_token.clear();
                self.quota.doubao_cookie.clear();
                self.quota.api_key.clear();self.quota.deepseek_key.clear();self.quota.note.clear();
                if !(previous_provider.relay()&&self.quota.provider.relay()){self.quota.source.clear();}
            }
            if self.quota.provider==Provider::NewApi{
                ui.add_enabled_ui(self.quota.api_connecting.is_none(),|ui|{
                    let previous=self.quota.relay_user_token;
                    ui.horizontal_wrapped(|ui|{ui.label("連線方式");ui.selectable_value(&mut self.quota.relay_user_token,false,"API Key");ui.selectable_value(&mut self.quota.relay_user_token,true,"使用者 Token（賬戶餘額）");});
                    if previous!=self.quota.relay_user_token{self.quota.api_key.clear();self.quota.relay_user_id.clear();self.quota.note.clear();}
                });
            }
            settings_control_row(ui,"備註",56.0,|ui|{ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.alias).vertical_align(egui::Align::Center).hint_text("例如：個人、工作、備用"))});
            if self.quota.provider.api_key(){
                let provider=self.quota.provider;
                let user_token=provider==Provider::NewApi&&self.quota.relay_user_token;
                let custom=provider==Provider::RelayCustom;
                ui.label(if custom{"查詢指令碼和憑據由 Windows 加密儲存在本機。"}else{"僅查詢用量；憑據儲存在 Windows 憑據管理器，不寫入配置或快取。"});
                ui.add_enabled_ui(self.quota.api_connecting.is_none(),|ui|{
                    if provider.relay(){
                        if self.quota.connect_target.is_some(){ui.label(format!("使用已儲存的 {} 介面",provider.name()));}
                        else if !self.quota.relay_manual&&!user_token{ui.label("自動識別站點型別");}
                        ui.label(if custom{"支援貼上 CC Switch 的 request / extractor 查詢指令碼。"}else if user_token{"填寫 New API 站點個人設定中的 Access Token 和使用者 ID，查詢賬戶餘額。"}else{"填寫站點網站地址和 API Key，讀取站點提供的用量。"});
                        ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.source).hint_text("https://你的站點地址（可含子目錄）"));
                        if user_token{settings_control_row(ui,"使用者 ID",64.0,|ui|{ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.relay_user_id).hint_text("站點個人資料中的數字 ID"))});}
                    }else if matches!(provider,Provider::MiniMax|Provider::Glm){
                        if !matches!(self.quota.source.as_str(),"cn"|"global"){self.quota.source="cn".into();}
                        ui.horizontal(|ui|{
                            ui.label("帳號地區");ui.selectable_value(&mut self.quota.source,"cn".into(),"中國區");ui.selectable_value(&mut self.quota.source,"global".into(),"國際區");
                            if provider==Provider::Glm&&ui.small_button("獲取金鑰").on_hover_text(if self.quota.source=="global"{"開啟國際區控制檯，選擇 Personal Coding Plan"}else{"開啟中國區個人程式設計套餐頁，獲取套餐 Key"}).clicked(){
                                let url=if self.quota.source=="global"{"https://z.ai/manage-apikey/apikey-list"}else{"https://bigmodel.cn/coding-plan/personal/overview"};
                                self.pending.push(Cmd::OpenUrl(url.into()));
                            }
                        });
                        ui.label(if provider==Provider::Glm{"請使用 Coding Plan 套餐 Key，非標準 API Key；本機 ZCode 登入無需填寫金鑰。"}else{"讀取 MiniMax Coding Plan；按量 API 餘額暫不包含。"});
                    }else{ui.label("查詢 OpenCode Go 套餐使用率，不查詢 Zen 按量餘額。");}
                    ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.api_key).password(true).hint_text(if custom{"API Key（指令碼需要時填寫）"}else if user_token{"貼上使用者 Access Token（非 API Key）"}else{"貼上 API Key"}));
                    if custom{
                        ui.collapsing("其他指令碼變數（可選）",|ui|{
                            ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.relay_access_token).password(true).hint_text("Access Token"));
                            ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.relay_user_id).hint_text("使用者 ID"));
                        });
                        ui.label("查詢指令碼");
                        ui.add_sized([ui.available_width(),180.0],egui::TextEdit::multiline(&mut self.quota.relay_script).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                    }
                    if ui.add_enabled((custom||!self.quota.api_key.trim().is_empty())&&(!provider.relay()||!self.quota.source.trim().is_empty())&&(!user_token||!self.quota.relay_user_id.trim().is_empty())&&(!custom||!self.quota.relay_script.trim().is_empty()),egui::Button::new("驗證並連線")).clicked(){
                        self.quota.note.clear();let source=self.quota.source.trim().into();let alias=self.quota.alias.clone();let key=std::mem::take(&mut self.quota.api_key);
                        self.quota.api_connecting=Some(if custom{quota::api::Connect::start_custom(source,alias,quota::relay::script::Credential{script:self.quota.relay_script.clone(),api_key:key,access_token:std::mem::take(&mut self.quota.relay_access_token),user_id:self.quota.relay_user_id.clone()})}else if user_token{quota::api::Connect::start_new_api_user(source,alias,key,self.quota.relay_user_id.clone())}else if provider.relay()&&!self.quota.relay_manual{quota::api::Connect::start_auto(source,alias,key)}else{quota::api::Connect::start(provider,source,alias,key)});
                    }
                });
                if self.quota.api_connecting.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("正在驗證用量…");if ui.button("取消").clicked(){self.quota.api_connecting=None;self.quota.note="已取消，未儲存金鑰".into();}});ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
            }else if self.quota.provider==Provider::WorkBuddy{
                ui.label("連線官網個人帳號，檢視剩餘積分。");
                ui.horizontal(|ui|{
                    if ui.button("開啟官網用量頁").clicked(){self.pending.push(Cmd::OpenUrl("https://www.codebuddy.cn/profile/usage".into()));}
                    if ui.add_enabled(self.quota.workbuddy_connecting.is_none(),egui::Button::new("登入並連線")).clicked(){
                        let expected=self.quota.connect_target.as_ref().and_then(|id|self.quota.config.accounts.iter().find(|a|&a.id==id)).and_then(|a|a.slot.clone());self.quota.note.clear();self.quota.workbuddy_connecting=Some(quota::workbuddy::Connect::start(self.quota.alias.clone(),expected));
                    }
                });
                ui.label(egui::RichText::new("自動識別本機登入；需要授權時開啟預設瀏覽器。登入狀態加密儲存在本機。").size(12.0).color(ui.visuals().weak_text_color()));
                if self.quota.workbuddy_connecting.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("請在預設瀏覽器完成官網登入…");if ui.button("取消").clicked(){self.quota.workbuddy_connecting=None;self.quota.note="已取消 WorkBuddy 連線".into();}});ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
            }else if self.quota.provider==Provider::DeepSeek{
                ui.label("查詢 API 賬戶餘額，金鑰僅儲存在本機。");
                ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.deepseek_key).password(true).hint_text("貼上 API Key"));
                if ui.add_enabled(self.quota.deepseek_connecting.is_none()&&!self.quota.deepseek_key.trim().is_empty(),egui::Button::new("驗證並連線")).clicked(){self.quota.note.clear();self.quota.deepseek_connecting=Some(quota::deepseek::Connect::start(self.quota.alias.clone(),std::mem::take(&mut self.quota.deepseek_key)));}
                if self.quota.deepseek_connecting.is_some(){ui.spinner();ui.label("正在連線 DeepSeek…");ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
            }else if self.quota.provider==Provider::Gemini{
                ui.label(egui::RichText::new("讀取 Gemini 網頁的 5 小時與每週用量。").size(12.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_gray(180),Color32::from_gray(75))));
                ui.horizontal_wrapped(|ui|{
                    if ui.button("開啟預設瀏覽器登入").clicked(){self.pending.push(Cmd::OpenUrl("https://gemini.google.com/usage".into()));}
                    if ui.add_enabled(self.quota.gemini_connecting.is_none(),egui::Button::new("連線已登入的 Chrome")).clicked(){self.quota.note.clear();self.quota.gemini_connecting=Some(quota::gemini::Connect::start(self.quota.alias.clone(),self.quota.gemini_email.clone(),None,self.quota.connect_target.as_ref().and_then(|id|self.quota.config.accounts.iter().find(|a|&a.id==id)).and_then(|a|a.slot.clone())));}
                });
                ui.label(egui::RichText::new("使用已登入的 Chrome 144+，允許連線後驗證用量，無需擴充套件。").size(12.0).color(ui.visuals().weak_text_color()));
                ui.horizontal_wrapped(|ui|{ui.label("首次在 Chrome 開啟原生連線：");ui.code("chrome://inspect/#remote-debugging");if ui.small_button("複製地址").clicked(){ui.ctx().copy_text("chrome://inspect/#remote-debugging".into());}});
                ui.add(egui::Label::new(egui::RichText::new("Chrome 會請求瀏覽器會話訪問許可權。島仔僅在連線時讀取 Gemini 登入，完成後斷開，後續使用本機加密會話重新整理。").size(11.0).color(ui.visuals().weak_text_color())).wrap());
                settings_control_row(ui,"Google 信箱",80.0,|ui|{ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.gemini_email).vertical_align(egui::Align::Center).hint_text("可選：指定要連線的 Google 信箱"))});
                ui.collapsing("高階：手動 Cookie 連線",|ui|{
                    ui.label("在 Gemini 用量頁按 F12，開啟網路，重新整理後找到 executeQuery 請求，複製 Cookie 請求頭。");
                    ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.gemini_cookie).vertical_align(egui::Align::Center).password(true).hint_text("貼上 Cookie 請求頭"));
                    if ui.add_enabled(self.quota.gemini_connecting.is_none() && !self.quota.gemini_cookie.trim().is_empty()&&!self.quota.gemini_email.trim().is_empty(),egui::Button::new("驗證並連線")).clicked(){self.quota.note.clear();self.quota.gemini_connecting=Some(quota::gemini::Connect::start(self.quota.alias.clone(),self.quota.gemini_email.clone(),Some(std::mem::take(&mut self.quota.gemini_cookie)),self.quota.connect_target.as_ref().and_then(|id|self.quota.config.accounts.iter().find(|a|&a.id==id)).and_then(|a|a.slot.clone())));}
                });
                if self.quota.gemini_connecting.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("正在連線並驗證 Gemini 用量…");if ui.button("取消").clicked(){self.quota.gemini_connecting=None;self.quota.note="已取消 Gemini 連線".into();}});ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
            }else if self.quota.provider==Provider::Doubao{
                ui.label("查詢豆包網頁會員的 5 小時、每週及活動額度。");
                let expected=self.quota.connect_target.as_ref().and_then(|id|self.quota.config.accounts.iter().find(|account|&account.id==id)).and_then(|account|account.slot.clone());
                ui.horizontal(|ui|{
                    if ui.add_enabled(self.quota.doubao_connecting.is_none(),egui::Button::new("瀏覽器登入")).clicked(){
                        self.quota.note.clear();self.quota.doubao_connecting=Some(quota::doubao::Connect::start(self.quota.alias.clone(),quota::doubao::LoginSource::Browser,expected.clone()));
                    }
                    if ui.add_enabled(self.quota.doubao_connecting.is_none(),egui::Button::new("從本機匯入")).clicked(){
                        self.quota.note.clear();self.quota.doubao_connecting=Some(quota::doubao::Connect::start(self.quota.alias.clone(),quota::doubao::LoginSource::Local,expected.clone()));
                    }
                    if ui.button("開啟豆包用量").clicked(){self.pending.push(Cmd::OpenUrl("https://www.doubao.com/member/quota-management".into()));}
                });
                ui.add(egui::Label::new("無需瀏覽器擴充套件，在獨立視窗完成豆包官網登入。也可匯入本機豆包客戶端當前帳號；登入狀態加密儲存在本機。").wrap());
                ui.collapsing("使用 Cookie 連線",|ui|{
                    ui.label("在豆包網頁按 F12，開啟網路，複製用量請求的完整 Cookie 請求頭。");
                    ui.add_sized([ui.available_width(),27.0],egui::TextEdit::singleline(&mut self.quota.doubao_cookie).password(true).hint_text("貼上 Cookie 請求頭"));
                    if ui.add_enabled(self.quota.doubao_connecting.is_none()&&!self.quota.doubao_cookie.trim().is_empty(),egui::Button::new("驗證並連線")).clicked(){
                        self.quota.note.clear();self.quota.doubao_connecting=Some(quota::doubao::Connect::start(self.quota.alias.clone(),quota::doubao::LoginSource::Cookie(std::mem::take(&mut self.quota.doubao_cookie)),expected.clone()));
                    }
                });
                if self.quota.doubao_connecting.is_some(){
                    ui.horizontal(|ui|{ui.spinner();ui.label("正在連線豆包…");if ui.button("取消").clicked(){self.quota.doubao_connecting=None;self.quota.doubao_cookie.clear();self.quota.note="已取消，未儲存豆包登入".into();}});
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
            }else if self.quota.provider==Provider::GrokBot{
                ui.horizontal(|ui|{
                    if ui.add_enabled(self.quota.login.is_none(),egui::Button::new("瀏覽器登入")).clicked(){self.quota_login(self.quota.alias.clone());}
                    if ui.button("從本機匯入").clicked(){self.quota.note=match quota::grok::native_path(){Some(path)=>match self.quota.config.import_grok(&path,&self.quota.alias){Ok(n)=>format!("新增 {n} 個 Grok Bot 帳號"),Err(e)=>e},None=>"請先安裝 Grok Bot 並登入，或使用瀏覽器連線".into()};}
                });
                ui.collapsing("匯入其他登入檔案",|ui|{self.quota_file_form(ui);});
                ui.label(egui::RichText::new("使用 Grok Bot 對應的 Cursor 帳號登入，瀏覽器授權支援自動續期。").size(12.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_gray(180),Color32::from_gray(75))));
            }else if self.quota.provider==Provider::ZCode{
                if ui.button("連線本機 ZCode").clicked(){self.quota.note=match quota::zcode::path().ok_or_else(||"未找到 ZCode 登入目錄".to_owned()).and_then(|p|quota::zcode::add(&mut self.quota.config,&p,&self.quota.alias)){Ok(())=>"ZCode 已連線".into(),Err(e)=>e};}
            }else if self.quota.provider==Provider::Antigravity{
                ui.label("讀取 Antigravity 各模型組的 5 小時與每週用量。");
                if ui.button("連線本機 Antigravity CLI").clicked(){self.quota.note=match quota::antigravity::path().ok_or_else(||"未找到 Antigravity CLI（agy），請先安裝並登入".to_owned()).and_then(|p|quota::antigravity::add(&mut self.quota.config,&p,&self.quota.alias)){Ok(())=>{self.pending.push(Cmd::QuotaConnected);"Antigravity 已連線".into()},Err(e)=>e};}
                ui.label(egui::RichText::new("透過官方 agy 的 /usage 指令查詢，不送出對話、不消耗額度，也不讀取登入令牌；登入失效時請在 agy 中重新登入。").size(12.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_gray(180),Color32::from_gray(75))));
            }else if self.quota.provider==Provider::Kiro{
                if ui.add_enabled(self.quota.kiro_connecting.is_none(),egui::Button::new("連線本機 Kiro")).clicked(){match quota::kiro::path(){Some(path)=>self.quota_start_kiro(path),None=>self.quota.note="未找到 Kiro 登入目錄".into()}}
                ui.collapsing("匯入其他登入檔案",|ui|{self.quota_file_form(ui);});
                ui.label(egui::RichText::new("只讀識別 Kiro 登入快取，核驗使用者後顯示套餐、有效試用和贈送 / 加購點數；超額上限單獨顯示。不自動續期，登入過期時請在 Kiro 中重新登入。").size(12.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_gray(180),Color32::from_gray(75))));
            }else if self.quota.provider==Provider::Claude{
                ui.add(egui::Label::new(CLAUDE_LOCAL_NOTICE).wrap());
                ui.horizontal(|ui|{
                    if ui.button("讀取桌面版本地記錄").clicked(){self.quota.note=match self.quota.config.connect_claude_desktop(&self.quota.alias){Ok(_)=>{self.pending.push(Cmd::QuotaConnected);"已連線，資料隨 Claude 桌面版使用更新".into()},Err(e)=>e};}
                    if ui.button("啟用 Code 本地採集").clicked(){self.quota.note=match self.quota.config.connect_claude_code(&self.quota.alias){Ok(chained)=>{self.pending.push(Cmd::QuotaConnected);if chained{"本地採集已啟用，並接在你原有的狀態列前面（原本的狀態列輸出不變）；使用 Claude Code 後更新"}else{"本地採集已啟用，使用 Claude Code 後更新"}.into()},Err(e)=>e};}
                });
                for text in ["桌面版自動從本地用量快取讀取重置時間；快取不可用時顯示未知，用量仍以本地記錄為準。","Code 本地採集會配置官方狀態列輸出；已有其他狀態列時，島仔接在它前面，原本的輸出不變、不會被覆蓋。僅使用網頁版時，暫無可讀取的本機記錄。"]{ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(ui.visuals().weak_text_color())).wrap());}
                ui.collapsing("選擇本地用量 JSON",|ui|{self.quota_file_form(ui);});
                claude_disclaimer(ui);
            }else{
                self.quota_file_form(ui);
                ui.label(egui::RichText::new("各帳號使用各自的登入檔案；移除只取消監控。登入失效時，請在對應工具中重新登入。").size(12.0).color(crate::settings_theme::color(ui.ctx(),Color32::from_gray(180),Color32::from_gray(75))));
            }
            if !self.quota.note.is_empty(){
                ui.add_space(8.0);
                let color=crate::settings_theme::color(ui.ctx(),Color32::from_rgb(112,190,255),Color32::from_rgb(24,94,160));
                egui::Frame::new().fill(color.gamma_multiply(0.12)).stroke(egui::Stroke::new(1.0,color.gamma_multiply(0.4))).corner_radius(8).inner_margin(10.0).show(ui,|ui|{
                    ui.set_min_width((ui.available_width()-1.0).max(0.0));
                    ui.label(egui::RichText::new(format!("●  {}",self.quota.note)).size(13.0).strong().color(color));
                });
            }

                if self.quota.login.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("等待瀏覽器登入…");if ui.button("取消登入").clicked(){self.quota.login=None;self.quota.note="登入已取消".into();}});ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
                if self.quota.gemini_connecting.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("正在連線 Gemini…");});ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
                if let Some(connect)=&mut self.quota.kiro_connecting{ui.horizontal(|ui|{ui.spinner();ui.label(if connect.cancelled(){"正在結束 Kiro 查詢…"}else{"正在核驗 Kiro 使用者與用量…"});if !connect.cancelled()&&ui.button("取消連線").clicked(){connect.cancel();}});ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
            });
        });self.quota.connect_open=open;
        if !open{self.quota_close_connection();}
    }
    fn quota_file_form(&mut self,ui:&mut egui::Ui){
        let local=self.quota.provider==Provider::Claude;
        ui.label(if local{"本地用量 JSON 檔案"}else{"登入 JSON 檔案"});ui.horizontal(|ui|{ui.add_sized([(ui.available_width()-65.0).max(100.0),27.0],egui::TextEdit::singleline(&mut self.quota.source).vertical_align(egui::Align::Center).hint_text(if local{"選擇 Claude 本地用量記錄"}else{"選擇該帳號的登入檔案"}));if ui.button("選擇…").clicked(){self.pending.push(Cmd::QuotaChooseFile);}});
        if self.quota.provider==Provider::Kiro{
            if ui.add_enabled(self.quota.kiro_connecting.is_none()&&!self.quota.source.trim().is_empty(),egui::Button::new("驗證並連線")).clicked(){self.quota_start_kiro(self.quota.source.trim().into());}
        }else if ui.button("新增帳號").clicked(){match self.quota.config.add(self.quota.provider,self.quota.alias.clone(),self.quota.source.trim().into()){Ok(())=>{self.quota.alias.clear();self.quota.source.clear();self.quota.note="帳號已新增".into();},Err(e)=>self.quota.note=e}}
    }
    #[cfg(feature="record")]
    pub fn quota_demo(&mut self,mode:&str){
        use serde_json::json;
        self.quota=State::default();let reset=chrono::Utc::now().timestamp()+8200;
        for(i,(provider,alias))in [(Provider::Codex,"個人"),(Provider::Codex,"工作"),(Provider::Claude,"Claude")].into_iter().enumerate(){let id=format!("demo-{i}");self.quota.config.accounts.push(Account{id:id.clone(),provider,alias:alias.into(),source:format!("demo-{i}.json").into(),slot:None,enabled:true,pinned:mode=="multi" || i==0});let payload=if provider==Provider::Codex{json!({"plan_type":if i==0{"pro"}else{"plus"},"rate_limit":{"primary_window":if mode=="single" && i==0{serde_json::Value::Null}else{json!({"used_percent":28+i*15,"limit_window_seconds":18000,"reset_at":reset})},"secondary_window":{"used_percent":56,"limit_window_seconds":604800,"reset_at":reset+200000}}})}else{json!({"five_hour":{"utilization":45,"resets_at":"2030-01-01T12:00:00Z"},"seven_day":{"utilization":27}})};self.quota.rows.insert(id,quota::parse(provider,&payload));}
        self.quota.config.selected="demo-0".into();self.show_page(6);if self.settings_open{self.settings_tab=11;}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kiro_pool_details_include_monthly_reset_and_overage_once(){
        let s=quota::parse(Provider::Kiro,&serde_json::json!({"userInfo":{"userId":"details-user"},"usageBreakdownList":[{"resourceType":"CREDIT","usageLimitWithPrecision":1000,"currentUsageWithPrecision":1080,"currentOveragesWithPrecision":80,"overageCapWithPrecision":200,"nextDateReset":1796083200_i64}],"overageConfiguration":{"overageStatus":"ENABLED"}})).unwrap();
        let details=snapshot_details(&s);
        assert_eq!(details.matches("月度額度").count(),1);
        assert!(details.contains("超額上限餘量：120 / 200 點"));
        assert!(details.contains("重置"));
    }
    #[test] fn api_details_include_third_window_tools_and_resets(){
        let w=|label:&str|quota::Window{label:label.into(),seconds:None,remaining:0.0,reset:Some(4102444800),amount:None};
        let snapshot=Snapshot{windows:vec![w("滾動"),w("本週"),w("本月")],extra:vec![w("工具額度")],..Default::default()};
        let detail=snapshot_details(&snapshot);for label in ["滾動","本週","本月","工具額度"]{assert!(detail.contains(label));}assert!(detail.contains("重置"));
    }
    #[test]
    fn claude_local_quota_displays_only_recorded_reset_times(){
        let now=chrono::Local::now();let id="local-fixture".to_string();
        let account=Account{id:id.clone(),provider:Provider::Claude,alias:"離線帳號".into(),source:"fixture/config.json".into(),slot:Some("fixture-slot".into()),enabled:true,pinned:true};
        let mut state=State::default();state.config.accounts.push(account.clone());
        let window=quota::Window{label:"5 小時".into(),seconds:Some(18000),remaining:37.5,reset:None,amount:None};
        let snapshot=Snapshot{at:now.timestamp(),windows:vec![window.clone()],..Default::default()};
        state.receive(quota::Event{id:id.clone(),result:Ok(snapshot.clone())});
        assert_eq!(reset_text(window.reset,&now),"重置時間未知");
        assert_eq!(state.rows[&id].as_ref().unwrap().windows[0],window);
        let later=now+chrono::Duration::minutes(11);
        assert_eq!(reset_text(window.reset,&later),"重置時間未知");
        assert_eq!(state.rows[&id].as_ref().unwrap().windows[0].remaining,37.5,"缺少重置時間不能影響用量百分比");
        let actual=quota::Window{reset:Some(now.timestamp()+1200),remaining:20.0,..window.clone()};
        assert_ne!(reset_text(actual.reset,&now),"重置時間未知");
        state.receive(quota::Event{id:id.clone(),result:Ok(Snapshot{windows:vec![actual.clone()],..snapshot})});
        assert_eq!(state.rows[&id].as_ref().unwrap().windows[0],actual,"本地更新仍應正常生效");
    }
    #[test]
    fn claude_local_records_do_not_restore_old_online_snapshots_or_claim_a_new_update(){
        let path=std::env::temp_dir().join(format!("daozai-claude-cache-{}.json",std::process::id()));
        let mut config=Config::default();let id="claude-local".to_string();config.accounts.push(Account{id:id.clone(),provider:Provider::Claude,alias:"Claude".into(),source:"desktop/config.json".into(),slot:Some("fixture".into()),enabled:true,pinned:true});
        let at=chrono::Utc::now().timestamp()-3600;let snapshot=Snapshot{at,windows:vec![quota::Window{label:"5 小時".into(),seconds:Some(18000),remaining:72.0,reset:None,amount:None}],..Default::default()};
        std::fs::write(&path,serde_json::json!({&id:{"provider":"Claude","slot":"fixture","snapshot":snapshot}}).to_string()).unwrap();
        let mut state=State::restore(config.clone(),path.clone());assert!(state.rows.is_empty(),"遷移後不能把舊聯網快取顯示為本地資料");
        state.receive(quota::Event{id:id.clone(),result:Ok(snapshot)});assert!(state.stale[&id].contains("本地記錄較舊"));
        let restored=State::restore(config,path.clone());assert_eq!(restored.rows[&id].as_ref().unwrap().at,at,"讀取快取不能重新整理官方記錄時間");std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn claude_verified_history_survives_client_exit_and_island_restart(){
        let path=std::env::temp_dir().join(format!("daozai-claude-exit-{}.json",std::process::id()));let mut config=Config::default();let id="claude-closed".to_string();config.accounts.push(Account{id:id.clone(),provider:Provider::Claude,alias:"Claude".into(),source:"desktop/config.json".into(),slot:Some("verified-owner".into()),enabled:true,pinned:true});
        let at=chrono::Utc::now().timestamp()-60;let snapshot=Snapshot{at,windows:vec![quota::Window{label:"本週".into(),seconds:Some(604800),remaining:16.0,reset:Some(at+86400),amount:None}],..Default::default()};
        let mut state=State::restore(config.clone(),path.clone());state.receive(quota::Event{id:id.clone(),result:Ok(snapshot.clone())});state.receive(quota::Event{id:id.clone(),result:Err("客戶端已關閉，等待新記錄".into())});assert_eq!(state.rows[&id].as_ref().unwrap().windows,snapshot.windows);assert!(state.stale.contains_key(&id));
        let mut restarted=State::restore(config,path.clone());assert_eq!(restarted.rows[&id].as_ref().unwrap().windows,snapshot.windows);restarted.receive(quota::Event{id:id.clone(),result:Err("等待新記錄".into())});assert_eq!(restarted.rows[&id].as_ref().unwrap().windows,snapshot.windows);assert_eq!(restarted.rows[&id].as_ref().unwrap().at,at);std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn expired_claude_desktop_windows_do_not_linger_and_the_old_record_says_how_old_it_is(){
        let mut config=Config::default();let id="claude-desktop-old".to_string();
        config.accounts.push(Account{id:id.clone(),provider:Provider::Claude,alias:"Claude".into(),source:"desktop/config.json".into(),slot:Some("fixture".into()),enabled:true,pinned:true});
        let window=|label:&str,seconds:i64,reset:Option<i64>|quota::Window{label:label.into(),seconds:Some(seconds),remaining:74.0,reset,amount:None};
        let labels=|state:&State|state.rows[&id].as_ref().unwrap().windows.iter().map(|w|w.label.clone()).collect::<Vec<_>>();
        let long_ago=chrono::Utc::now().timestamp()-18*3600;
        let mut state=State{config,..Default::default()};
        // 18 小時前的記錄：5 小時視窗早已重置，一週視窗仍然有效。
        state.rows.insert(id.clone(),Ok(Snapshot{at:long_ago,windows:vec![window("5 小時",18000,None),window("本週",604800,None)],..Default::default()}));
        state.receive(quota::Event{id:id.clone(),result:Err("客戶端已關閉，等待新記錄".into())});
        assert_eq!(labels(&state),["本週"]);assert!(state.stale.contains_key(&id));
        // 全部過期：不再顯示舊數字，改顯示原因。
        state.rows.insert(id.clone(),Ok(Snapshot{at:long_ago,windows:vec![window("5 小時",18000,None)],..Default::default()}));
        state.receive(quota::Event{id:id.clone(),result:Err("客戶端已關閉，等待新記錄".into())});
        assert_eq!(state.rows[&id].as_ref().err().map(String::as_str),Some("客戶端已關閉，等待新記錄"));
        // 有明確重置時間的視窗不受影響。
        state.rows.insert(id.clone(),Ok(Snapshot{at:long_ago,windows:vec![window("5 小時",18000,Some(long_ago+18000))],..Default::default()}));
        state.receive(quota::Event{id:id.clone(),result:Err("等待新記錄".into())});assert_eq!(labels(&state),["5 小時"]);
        // 還沒過期但較舊的記錄：提示說明多舊，以及桌面版要開到用量頁面才會寫新記錄。
        let two_hours=chrono::Utc::now().timestamp()-7200;
        state.receive(quota::Event{id:id.clone(),result:Ok(Snapshot{at:two_hours,windows:vec![window("5 小時",18000,None)],..Default::default()})});
        let note=&state.stale[&id];assert!(note.contains("本地記錄較舊")&&note.contains("2 小時前")&&note.contains("Claude 桌面版"),"{note}");
        let mut code=state.config.clone();code.accounts[0].source="claude/.credentials.json".into();code.accounts[0].id="claude-code".into();
        let mut code_state=State{config:code,..Default::default()};
        code_state.receive(quota::Event{id:"claude-code".into(),result:Ok(Snapshot{at:two_hours,windows:vec![window("5 小時",18000,Some(two_hours+18000))],..Default::default()})});
        assert!(!code_state.stale["claude-code"].contains("桌面版"),"Claude Code 的記錄不用叫人開桌面版");
    }
    #[test] fn side_amounts_stay_short(){
        for (value,symbol,expected) in [(4.18,"¥","¥4.1"),(999.9,"¥","¥999"),(9999.0,"¥","¥9.99k"),(28560.0,"","2.85萬"),(123456.7,"¥","¥12.3萬"),
            (1234567.8,"¥","¥123萬"),(99999999.0,"¥","¥0.99億"),(12345678901.0,"","123億"),(-12.5,"¥","-¥12.5"),(0.0,"","0"),(0.04,"¥","¥0")] {
            assert_eq!(side_amount(value,symbol),expected,"{value}");
        }
        assert!(side_amount(f64::NAN,"¥").contains("--"));
    }
    #[test] fn deepseek_missing_balance_distinguishes_loading_and_failure(){
        let mut state=State::default();assert!(state.balance_loading("test"));assert!(state.balance_text("test").is_none());
        state.rows.insert("test".into(),Err("無法連線".into()));assert!(!state.balance_loading("test"));assert!(state.balance_text("test").is_none());
        state.busy.insert("test".into());assert!(state.balance_loading("test"));
    }
    #[test] fn zcode_badge_has_clear_corners_and_a_visible_edge(){
        let icon=egui_extras::image::load_svg_bytes_with_size(include_bytes!("../assets/ai-icons/zcode.svg"),Some(egui::load::SizeHint::Size(26,26))).unwrap();
        for (x,y) in [(3,3),(22,3),(3,22),(22,22)] {assert_eq!(icon[(x,y)].a(),0);}
        assert!(icon[(13,1)].a()>180);
        assert!(icon.pixels.iter().filter(|p|p.r()>230&&p.a()>240).count()>35);
    }
    #[test] fn relay_key_icons_remain_visible_in_light_skins(){
        let ctx=egui::Context::default();
        for provider in [Provider::OneApi,Provider::RelayGeneral,Provider::RelayCustom]{
            let texture=logo_texture(&ctx,provider,24).unwrap();
            for palette in [crate::skin_paint::CREAM,crate::skin_paint::GLASS_LIGHT]{
                let mut mesh=egui::Mesh::with_texture(texture.id());
                mesh.add_rect_with_uv(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(24.0,24.0)),egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
                let mut shape=egui::Shape::mesh(mesh);crate::skin_paint::recolor(&mut shape,&palette);
                if let egui::Shape::Mesh(mesh)=shape{assert!(mesh.vertices.iter().all(|v|v.color.r()<80),"淺色皮膚上的鑰匙應跟隨文字變深");}else{panic!("應繪製鑰匙紋理");}
            }
        }
    }
    #[test] fn deepseek_badge_keeps_brand_blue_in_every_skin(){
        use crate::model::Skin;
        for skin in [Skin::Classic,Skin::Cream,Skin::GlassLight,Skin::GlassDark]{
            let image=egui_extras::image::load_svg_bytes_with_size(deepseek_badge_svg(skin).as_bytes(),Some(egui::load::SizeHint::Size(100,100))).unwrap();
            assert!(image.pixels.iter().any(|p|*p==Color32::from_rgb(77,107,254)));
            let id=egui::TextureId::User(0xDEE5);crate::skin_paint::mark_monochrome(id);crate::skin_paint::mark_colored(id);
            let mut mesh=egui::Mesh::with_texture(id);mesh.add_rect_with_uv(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(24.0,24.0)),egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
            let mut shape=egui::Shape::mesh(mesh);crate::skin_paint::recolor(&mut shape,&crate::skin_paint::CREAM);
            if let egui::Shape::Mesh(mesh)=shape{assert!(mesh.vertices.iter().all(|v|v.color==Color32::WHITE));}else{panic!("徽標應保留紋理");}
        }
    }
    #[test]
    #[ignore="只讀驗證本機已啟用帳號冷啟動時可恢復用量"]
    fn quota_live_cold_start(){
        let state=State::restore(Config::load(),crate::model::config_dir().join("ai-quota-cache.json"));
        let enabled:Vec<_>=state.config.accounts.iter().filter(|a|a.enabled).collect();
        assert!(!enabled.is_empty());
        for a in enabled {
            assert!(matches!(state.rows.get(&a.id),Some(Ok(_))),"{} 缺少上次記錄",a.provider.name());
            assert!(state.stale.contains_key(&a.id));
            println!("{} 已從磁碟恢復",a.provider.name());
        }
    }
    #[test] fn restart_restores_last_balance_only_for_the_bound_account(){
        let path=std::env::temp_dir().join(format!("daozai-quota-restart-{}.json",std::process::id()));
        let mut config=Config::default();
        let id=config.connect_workbuddy("fixture".into(),"測試帳號".into());
        let mut state=State::restore(config.clone(),path.clone());
        let snapshot=quota::workbuddy::parse(&serde_json::json!({"code":0,"data":{"Packages":[{"CycleRemainCapacity":2000}]}})).unwrap();
        state.receive(quota::Event{id:id.clone(),result:Ok(snapshot)});
        let mut restored=State::restore(config.clone(),path.clone());
        assert_eq!(restored.balance_text(&id).as_deref(),Some("2k"));
        assert!(restored.stale.contains_key(&id));
        restored.receive(quota::Event{id:id.clone(),result:Err("瀏覽器未啟動".into())});
        assert_eq!(restored.balance_text(&id).as_deref(),Some("2k"));
        config.accounts[0].slot=Some("不同帳號".into());
        assert!(State::restore(config,path.clone()).rows.is_empty());
        std::fs::remove_file(path).unwrap();
    }
    #[test] fn deepseek_refresh_failure_keeps_balance_until_success(){
        let mut state=State::default();let id=state.config.connect_deepseek("test".into(),"DeepSeek".into());
        let snapshot=quota::deepseek::parse(&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"12800.00","granted_balance":"800","topped_up_balance":"12000"}]})).unwrap();
        state.receive(quota::Event{id:id.clone(),result:Ok(snapshot.clone())});state.receive(quota::Event{id:id.clone(),result:Err("連線失敗".into())});
        assert_eq!(state.rows[&id].as_ref().unwrap().wallet.as_ref().unwrap().primary().total,12800.0);assert!(state.stale.contains_key(&id));
        state.receive(quota::Event{id:id.clone(),result:Ok(snapshot)});assert!(!state.stale.contains_key(&id));
    }

    use chrono::TimeZone;
    #[test]
    fn quota_logo_matches_screen_pixels_at_each_display_scale(){
        for scale in [1.0,1.25,1.5,1.75,2.0] {
            let ctx=egui::Context::default();egui_extras::install_image_loaders(&ctx);
            for provider in Provider::ALL {for size in [10.0,12.0,20.0,24.0] {
                let raw=egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(120.0,80.0))),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
                let output=ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                    let c=Card::new(ui,egui::pos2(0.37,0.21),ui.max_rect());logo(&c,provider,32.25,32.5,size);
                });});
                let mesh=output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Mesh(mesh)=>Some(mesh),_=>None}).expect("應繪製廠商圖示");
                let image=&output.textures_delta.set.iter().find(|(id,_)|*id==mesh.texture_id).expect("不同尺寸應各自生成紋理").1.image;
                let pixels=(size*scale).round() as usize;
                assert_eq!(image.size(),[pixels,pixels],"圖示應按螢幕畫素生成，廠商={}，縮放={scale}",provider.name());
                for v in &mesh.vertices {
                    assert!((v.pos.x*scale-(v.pos.x*scale).round()).abs()<0.001 && (v.pos.y*scale-(v.pos.y*scale).round()).abs()<0.001,"圖示邊界應對齊物理畫素");
                }
                if let egui::ImageData::Color(image)=image {
                    if provider==Provider::Doubao{
                        assert!(image.pixels.iter().any(|pixel|pixel.a()>150&&pixel.b()>pixel.r().saturating_add(20)),"豆包頭像應保留官方原始配色");
                        assert!(image.pixels.iter().any(|pixel|pixel.a()==0),"豆包頭像應保留透明底");
                    }else if provider==Provider::MiniMax{
                        let original=egui_extras::image::load_svg_bytes_with_size(include_bytes!("../assets/ai-icons/minimax.svg"),Some(egui::load::SizeHint::Size(pixels as u32,pixels as u32))).unwrap();
                        assert_eq!(image.pixels,original.pixels,"MiniMax 應保留品牌顏色和透明邊緣");
                    }else if keeps_brand_color(provider){
                        assert!(image.pixels.iter().any(|p|p.a()>200),"{} 官方彩色標誌應實際繪出不透明像素",provider.name());
                        assert!(image.pixels.iter().any(|p|p.a()>200&&(p.r()!=p.g()||p.g()!=p.b())),"{} 官方標誌應保留彩色",provider.name());
                    }else{for pixel in &image.pixels {let expected=Color32::from_white_alpha(pixel.a());assert_eq!(*pixel,expected,"標識顏色與透明邊緣應保持一致");}}
                }
            }}
        }
    }
    #[test]
    fn quota_reset_uses_local_calendar_dates_and_clock_times(){
        let now=chrono::Local.with_ymd_and_hms(2026,10,31,22,0,0).unwrap();
        for (parts,expected) in [
            ((2026,10,31,23,30),"今天 23:30 重置"),
            ((2026,11,1,9,0),"11月1日 09:00 重置"),
            ((2026,11,3,9,0),"11月3日 09:00 重置"),
            ((2027,1,1,9,0),"2027/1/1 09:00 重置"),
        ] {
            let (year,month,day,hour,minute)=parts;
            let at=chrono::Local.with_ymd_and_hms(year,month,day,hour,minute,0).unwrap().timestamp();
            assert_eq!(reset_text(Some(at),&now),expected);
        }
    }
    #[test]
    fn quota_reset_does_not_invent_missing_or_expired_times(){
        let now=chrono::Local.with_ymd_and_hms(2026,10,31,22,0,0).unwrap();
        assert_eq!(reset_text(None,&now),"重置時間未知");
        assert_eq!(reset_text(Some(i64::MAX),&now),"重置時間未知");
        assert_eq!(reset_text(Some(now.timestamp()),&now),"等待重新整理");
        assert_eq!(reset_text(Some(now.timestamp()-1),&now),"等待重新整理");
    }
}

// ---------------------------------------------------------------- 左右停靠：豎向額度
// 側邊停靠時額度豎排在膠囊裡：環形額度一個圓環一行；餘額類（DeepSeek、WorkBuddy）圖示在上、金額在下。
pub(crate) const SIDE_QUOTA_RING_H:f32=30.0;

/// 側邊豎排的金額：4.1、999、9.99k、99.9萬、999萬、0.12億……截斷不進位，避免顯示得比實際多。
pub(crate) fn side_amount(value:f64,symbol:&str)->String{
    if !value.is_finite(){return format!("{symbol}--");}
    let n=value.abs();
    let (mut v,mut unit)=if n>=100_000_000.0{(n/100_000_000.0,"億")}else if n>=10_000.0{(n/10_000.0,"萬")}else if n>=1000.0{(n/1000.0,"k")}else{(n,"")};
    if unit=="萬" && v>=1000.0 {v=n/100_000_000.0;unit="億";}
    let digits=if unit.is_empty(){if v<100.0{1}else{0}}else if v<10.0{2}else if v<100.0{1}else{0};
    let factor=10f64.powi(digits);
    let text=format!("{:.*}",digits as usize,(v*factor).trunc()/factor);
    let text=if text.contains('.'){text.trim_end_matches('0').trim_end_matches('.').to_string()}else{text};
    format!("{}{symbol}{text}{unit}",if value<0.0 && text!="0"{"-"}else{""})
}
pub(crate) const SIDE_QUOTA_BALANCE_H:f32=38.0;
impl App {
    pub(crate) fn quota_side_items(&self)->Vec<(usize,f32)>{
        self.quota_bar_indices().into_iter().map(|i|{
            let balance=self.quota.config.accounts[i].provider.balance();
            (i,if balance{SIDE_QUOTA_BALANCE_H}else{SIDE_QUOTA_RING_H})
        }).collect()
    }

    pub(crate) fn draw_quota_side(&mut self,c:&Card,cx:f32,mid:f32,height:f32,i:usize){
        let Some(a)=self.quota.config.accounts.get(i).cloned() else{return};
        let snapshot=self.quota.rows.get(&a.id).and_then(|r|r.as_ref().ok()).cloned();
        let summary;
        if a.provider.balance(){
            let icon_y=mid-7.0;
            if a.provider!=Provider::DeepSeek{logo(c,a.provider,cx,icon_y,20.0);}else{deepseek_badge(c,self.settings.skin,cx,icon_y,22.0);}
            let loading=self.quota.balance_loading(&a.id);let stale=self.quota.stale.contains_key(&a.id);
            if stale||(!loading&&snapshot.is_none()){
                c.circle(cx+8.0,icon_y+8.0,3.6,Color32::BLACK);
                c.circle(cx+8.0,icon_y+8.0,2.4,Color32::from_rgb(240,165,84));
            }
            if let Some(text)=self.quota.balance_text(&a.id){
                let wallet=snapshot.as_ref().and_then(|s|s.wallet.as_ref());
                let color=if stale{theme::GRAY}else if wallet.is_some_and(|w|!w.available||w.primary().total<=0.0){Color32::from_rgb(255,166,77)}else{theme::WHITE};
                // 豎向膠囊只有 34 點寬：用更短的寫法（最多約 4 位有效字元），放不下再縮小字號。
                let short=snapshot.as_ref().and_then(|s|s.wallet.as_ref().map(|w|side_amount(w.primary().total,w.primary().symbol()))
                    .or_else(||s.credits.as_ref().map(|c|side_amount(c.remaining,""))).or_else(||s.relay.as_ref().map(|q|q.compact().split(" · ").next().unwrap_or("未知").to_owned()))).unwrap_or_else(||text.clone());
                let size=[9.5,8.5,7.5].into_iter().find(|&size|c.text_w(&short,size,W::Bold)<=34.0).unwrap_or(7.5);
                let shown=c.fit(&short,size,W::Bold,34.0);
                c.text(cx,mid+15.0,&shown,size,W::Bold,color,H::Center);
                summary=format!("{} {text}",if a.provider.relay(){"額度"}else{"餘額"});
            }else if loading{
                let active=(self.t*3.0) as usize%3;
                for dot in 0..3{c.circle(cx-5.0+dot as f32*5.0,mid+11.0,1.5,theme::GRAY.gamma_multiply(if dot==active{1.0}else{0.35}));}
                c.ui.ctx().request_repaint_after(std::time::Duration::from_millis(160));
                summary="正在更新".into();
            }else{summary="餘額暫不可用".into();}
        }else{
            ring(c,&a,snapshot.as_ref(),cx,mid,26.0,accent_colors(&self.quota.config,&a));
            let mut same=self.quota.config.accounts.iter().filter(|other|other.provider==a.provider && other.enabled);
            if same.clone().count()>1{let n=same.position(|other|other.id==a.id).unwrap_or(0)+1;c.circle(cx+10.0,mid+9.0,4.5,Color32::from_gray(225));c.text(cx+10.0,mid+11.8,&n.to_string(),6.5,W::Bold,Color32::BLACK,H::Center);}
            summary=snapshot.as_ref().map(|s|s.rings().iter().map(|w|w.summary()).collect::<Vec<_>>().join(" · ")).unwrap_or_else(||if self.quota.busy.contains(&a.id){"正在更新".into()}else{"用量暫不可用".into()});
        }
        let hit=c.hit(cx-17.0,mid-height/2.0,34.0,height,("quota-side",&a.id));
        if bar_tooltip(hit,format!("{} · {}\n{}",a.provider.name(),a.alias,summary)).clicked(){
            self.quota.config.selected=a.id.clone();self.quota.config.save();self.show_page(6);
        }
    }
}
