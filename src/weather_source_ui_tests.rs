//! 天氣來源設定的原生互動與合成資料；只使用佔位授權碼，不傳送網路請求。
use super::*;
use crate::{weather_provider::Provider,settings_theme::SettingsTheme};

fn fixture(theme:SettingsTheme)->H {
    let mut h=H::new();h.app.settings.settings_theme=theme;h.app.settings_tab=12;h.app.open_settings();h.ctx.set_embed_viewports(false);
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,code:2,day:true,feels:Some(18.0),humidity:Some(58),wind:Some(12.0),high:Some(23.0),low:Some(15.0),at:chrono::Utc::now().timestamp(),..Default::default()});
    h
}

#[test]
fn weather_badge_only_dims_after_repeated_failures_and_old_data(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.weather_source.displayed=Provider::OpenMeteo;
    for (age,failures,gray) in [(crate::weather::INTERVAL+900,0,false),(crate::weather::INTERVAL*3,0,false),(crate::weather::INTERVAL*3,1,false),(crate::weather::INTERVAL,2,false),(crate::weather::INTERVAL*3,2,true),(crate::weather::INTERVAL*3,0,false)] {
        h.app.weather.failures=failures;
        let snapshot=crate::weather::Snapshot{place:"廣州".into(),temp:23.0,code:2,day:true,at:chrono::Utc::now().timestamp()-age,..Default::default()};
        let output=h.ctx.run(Default::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{
            let c=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect());
            let layout=crate::bar_layout::Layout::new(false,0,false,false,false,false,true);
            h.app.paint_pill_weather(&c,100.0,&layout,&snapshot);
            h.app.paint_side_weather(&c,220.0,30.0,&snapshot);
        });});
        let colors:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="23°"=>Some(t.fallback_color),_=>None}).collect();
        assert_eq!(colors.len(),2,"橫向和側邊都應顯示溫度");
        assert!(colors.iter().all(|c|*c==if gray{crate::theme::GRAY}else{crate::theme::WHITE}),"等待重新整理或單次失敗時應保持白色，只有連續失敗且資料明顯過期才變灰：age={age}, failures={failures}");
    }
}

// 與設定頁真實渲染一致：繪製一幀並返回設定視窗的文字。
fn draw(h:&mut H,events:Vec<egui::Event>,size:Vec2,scale:f32)->(egui::FullOutput,Vec<(egui::ViewportId,egui::FullOutput)>){
    let children=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=children.clone();h.t+=0.02;let time=h.t;
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),time:Some(time),
            events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
        let out=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));capture.lock().unwrap().push((id,out));
    });
    let root=h.ctx.run(egui::RawInput{time:Some(time),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
    let child_frames=std::mem::take(&mut *children.lock().unwrap());
    h.shapes=child_frames.iter().find(|(id,_)|*id==egui::ViewportId::from_hash_of("daozai-settings")).unwrap().1.shapes.clone();
    (root,child_frames)
}

#[test]
fn cwa_settings_show_authkey_flow_and_no_qweather_text(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=fixture(SettingsTheme::Dark);
    h.app.weather_source.selected=Provider::Cwa;h.app.weather_source.config_open=true;
    draw(&mut h,vec![],egui::vec2(740.0,980.0),1.0);
    let texts:Vec<String>=h.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t.galley.text().to_string()),_=>None}).collect();
    assert!(texts.iter().any(|t|t.contains("中央氣象署")),"應顯示 CWA 設定區塊");
    assert!(texts.iter().any(|t|t.contains("申請授權碼")),"應提供授權碼申請入口");
    assert!(texts.iter().any(|t|t.contains("臺灣中央氣象署 (CWA OpenData) API 授權碼")),"設定標題應與需求一致");
    for banned in ["和風","QWeather","qweather","配置教程","Host"] {
        assert!(!texts.iter().any(|t|t.contains(banned)),"設定頁不應再出現 {banned}");
    }
}
