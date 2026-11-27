//! 無視窗互動測試：直接向 egui 注入輸入事件，驗證懸停展開、切頁、按鈕。
use crate::app::{App, Env, Options, PH, PW, WIN_H, WIN_W};
// 舊版 IM（企業微信、飛書、釘釘、QQ）互動測試，依賴 cn-messengers 功能。
#[cfg(feature="cn-messengers")]
#[path="messenger/ui_tests.rs"]
mod enterprise_messenger;
#[path="calendar/ui_compatibility_tests.rs"]
mod calendar_compatibility;
#[path="doubao_ui_tests.rs"]
mod doubao;
#[path="weather_source_ui_tests.rs"]
mod weather_source;
#[path="lyrics_switch_ui_tests.rs"]
mod lyrics_switch;
#[path="health_reminder_ui_tests.rs"]
mod health_reminder;
use eframe::egui::{self, Pos2, Rect, Vec2};
use std::sync::Mutex;

pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn gemini_connect_form_opens_default_browser_without_claiming_connection(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dark in [true,false]{
        let mut h=H::new();h.ctx.set_visuals(if dark{egui::Visuals::dark()}else{egui::Visuals::light()});
        h.app.settings_open=true;h.app.quota.provider=crate::quota::Provider::Gemini;h.app.quota.connect_open=true;
        let mut raster=crate::preview_raster::Raster::new(640,760);
        for _ in 0..4{
            let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(640.0,760.0))),..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));});
            raster.update_textures(&out.textures_delta);h.shapes=out.shapes;
        }
        let texts=h.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t.galley.text())}else{None}).collect::<Vec<_>>();
        assert!(texts.contains(&"開啟預設瀏覽器登入"),"Gemini 應在系統預設瀏覽器中登入");
        assert!(texts.contains(&"連線已登入的 Chrome"),"Gemini 應支援本人授權連線現有 Chrome 會話");
        assert!(texts.iter().any(|s|s.contains("chrome://inspect/#remote-debugging")),"首次連線應提供 Chrome 原生設定入口");
        assert!(!texts.iter().any(|s|s.contains("自動連線")),"開啟官網不能表示島仔已獲得登入會話");
        assert!(!texts.iter().any(|s|s.contains("Kimi 瀏覽器擴充套件")),"Gemini 連線不應要求額外安裝擴充套件");
        assert!(texts.contains(&"Google 信箱"));
        let original=h.app.quota.config.clone();
        let point=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="開啟預設瀏覽器登入"=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap();
        for pressed in [true,false]{
            let _=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(640.0,760.0))),events:vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));});
        }
        assert!(h.app.pending.contains(&crate::app::Cmd::OpenUrl("https://gemini.google.com/usage".into())));
        assert!(h.app.quota.gemini_connecting.is_none());assert_eq!(h.app.quota.config,original);
        if let Some(dest)=std::env::var_os("DAOZAI_GEMINI_PREVIEW_DIR"){
            let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
            raster.buf.fill([0.08,0.09,0.12,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),1.0),1.0);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dest.join(if dark{"dark.png"}else{"light.png"}),&rgba,640,760,image::ColorType::Rgba8).unwrap();
        }
    }
}

#[test]
fn gemini_closed_dialogs_discard_completed_logins_without_saving(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for close_settings in [false,true]{
        let mut h=H::new();let original=h.app.quota.config.clone();h.app.settings_open=true;h.app.quota.connect_open=true;
        let (connect,tx)=crate::quota::gemini::Connect::pending();h.app.quota.gemini_connecting=Some(connect);
        tx.send(Ok(crate::quota::gemini::simulated_connected())).unwrap();
        if close_settings{h.app.settings_open=false;}else{h.app.quota_close_connection();}
        h.app.quota_tick();assert!(h.app.quota.gemini_connecting.is_none());assert_eq!(h.app.quota.config,original);
        assert!(h.app.quota.rows.is_empty());assert!(!crate::model::config_dir().join("credentials").exists(),"關閉後不能儲存晚到的會話");
    }
}

#[test]
fn weather_content_tap_switches_both_ways_with_a_short_transition(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (x,y) in [(100.0,75.0),(270.0,82.0),(314.0,178.0)] {
        let mut h=H::new();h.app.settings.pet=false;
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,sunrise:Some("06:52".into()),sunset:Some("17:21".into()),..Default::default()});
        h.app.show_page(8);let p=H::local(x,y);h.hover(p,1.0);h.click(p);
        assert!(h.app.weather.solar_open,"天氣內容區 ({x},{y}) 應可以輕點切換");
        h.hover(p,0.05);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("返回天氣")&&t.fallback_color.a()>0&&t.fallback_color.a()<255)),"切換時應有短暫淡入動效");
        h.hover(p,0.3);h.click(p);assert!(!h.app.weather.solar_open,"日照內容區也可以輕點返回");
        h.hover(p,0.3);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="19°"&&t.fallback_color.a()==255)),"動效完成後應恢復清晰的天氣概覽");
    }
}

#[test]
fn weather_content_tap_keeps_other_controls_independent(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (x,y,target) in [(375.0,crate::app::WEATHER_H-7.0,8),(394.0,18.0,8),(367.0,18.0,0),(398.0,88.0,0),(20.0,88.0,7)] {
        let mut h=H::new();h.app.settings.pet=false;
        h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),playing:true,..Default::default()};
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,..Default::default()});
        h.app.show_page(8);h.hover(H::local(250.0,65.0),1.0);h.click(H::local(x,y));
        assert!(!h.app.weather.solar_open,"操作 ({x},{y}) 不應觸發日照切換");
        assert_eq!(h.app.page(),target,"操作 ({x},{y}) 的頁面跳轉應保持不變");
        if y>200.0 {assert!(h.cmds.contains(&crate::app::Cmd::OpenUrl("https://open-meteo.com/".into())));}
        if x==394.0 {assert!(h.app.settings_open);}
    }
}

#[test]
fn weather_week_shows_all_seven_days(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,
        days:["今天","明天","週五","週六","週日","週一","週二"].into_iter().map(|label|crate::weather::Day{label:label.into(),high:23.0,low:15.0,..Default::default()}).collect(),
        at:chrono::Utc::now().timestamp(),..Default::default()});
    h.app.show_page(8);h.hover(H::local(250.0,65.0),1.0);
    for label in ["今天","明天","週五","週六","週日","週一","週二"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==label)),"七天預報缺少 {label}");
    }
}

#[test]
fn weather_week_keeps_each_forecast_date_visible(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::parse("杭州",&serde_json::json!({"current":{"temperature_2m":19},
        "daily":{"time":["2026-11-28","2026-11-29","2026-11-30","2026-12-01","2026-12-02","2026-12-03","2026-12-04"]}})).unwrap());
    h.app.show_page(8);h.hover(H::local(250.0,65.0),1.0);
    for date in ["11/28","11/29","11/30","12/01","12/02","12/03","12/04"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==date)),"七天預報缺少日期 {date}");
    }
    for s in &h.shapes {if let egui::Shape::Text(t)=&s.shape {assert!(s.clip_rect.contains_rect(t.visual_bounding_rect()),"日期與預報文字不應裁切：{}",t.galley.text());}}
}

#[test]
fn weather_today_opens_solar_details_and_returns_to_forecast(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,..Default::default()});
    h.app.show_page(8);h.hover(H::local(250.0,65.0),1.0);h.click(H::local(100.0,75.0));h.hover(H::local(100.0,75.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("返回天氣"))),"點選當天區域後應進入日出日落檢視");
    for phrase in ["日出","日落","06:52","17:21","日照進度待更新"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"舊快取日照檢視缺少 {phrase}");
    }
    assert!(!h.app.weather.loading,"瀏覽日照檢視不應額外發起網路請求");
    h.click(H::local(80.0,44.0));
    assert!(!h.app.weather.solar_open);
    h.hover(H::local(80.0,44.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="日出 06:52")),"返回後應顯示天氣檢視的日照入口");
}

#[test]
fn weather_side_bar_shows_a_clickable_temperature_on_both_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
        let mut h=H::new();h.app.settings.dock=dock;
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:-12.0,code:0,day:true,..Default::default()});
        for _ in 0..60{side_frame(&mut h,Some(Pos2::new(250.0,580.0)));}
        let response=h.ctx.read_response(egui::Id::new(("card",true,"weather-side"))).expect("兩側都應提供常駐天氣入口");
        let temp=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="12°"=>Some((s.clip_rect,t.visual_bounding_rect())),_=>None}).expect("負溫度應完整顯示");
        assert!(temp.0.contains_rect(temp.1),"{dock:?} 負溫度不應裁切");
        let click=response.rect.center();
        side_frame(&mut h,Some(click));
        for pressed in [true,false] {
            h.t+=1.0/60.0;
            let env=Env{t:h.t,clock:"09:41:00".into(),cursor:Some(click),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(500.0,600.0))),time:Some(h.t),focused:true,
                events:vec![egui::Event::PointerButton{pos:click,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],..Default::default()};
            let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});
        }
        assert_eq!(h.app.page(),8,"{dock:?} 點選側邊溫度應進入天氣頁");
        assert!(!h.app.weather.solar_open);
        h.app.settings.visible_pages[8]=false;
        assert!(!h.app.side_items().contains(&crate::side_dock::SideItem::Weather));
        h.app.settings.visible_pages[8]=true;h.app.settings.minimal=true;
        assert!(!h.app.side_items().contains(&crate::side_dock::SideItem::Weather));
    }
}

#[test]
fn weather_solar_view_preserves_readable_times_across_skins_and_scales(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use chrono::TimeZone;
    let now=chrono_tz::Asia::Shanghai.with_ymd_and_hms(2026,10,28,9,41,0).single().unwrap().with_timezone(&chrono::Local);
    for skin in crate::model::Skin::ALL {for scale in [1.0,1.25,1.5] {
        let mut h=H::new();h.app.settings.skin=skin;h.ctx.set_pixels_per_point(scale);
        h.app.weather.snapshot=Some(crate::weather::parse("杭州",&serde_json::json!({"timezone":"Asia/Shanghai","current":{"temperature_2m":19},
            "daily":{"time":["2026-10-28"],"sunrise":["2026-10-28T06:52"],"sunset":["2026-10-28T17:21"]}})).unwrap());
        h.app.weather.solar_open=true;h.app.show_page(8);
        for _ in 0..60 {
            h.t+=1.0/60.0;
            let cursor=H::local(180.0,185.0);
            let env=Env{t:h.t,clock:now.format("%H:%M:%S").to_string(),today:now.format("%Y-%m-%d").to_string(),date:now.format("%m月%d日").to_string(),cursor:Some(cursor),bt_ready:true,..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),focused:true,
                events:vec![egui::Event::PointerMoved(cursor)],..Default::default()};
            h.shapes=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);
            });}).shapes;
        }
        for phrase in ["返回天氣","距離日落","07:40","白晝中點","12:06","06:52","17:21","白晝 10小時29分"] {
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"{skin:?} / {scale} 日照檢視缺少 {phrase}");
        }
        for s in &h.shapes {if let egui::Shape::Text(t)=&s.shape {assert!(s.clip_rect.contains_rect(t.visual_bounding_rect()),"日照文字不應裁切：{}",t.galley.text());}}
    }}
}

#[test]
#[ignore="按需匯出七日預報、日照和兩側常駐天氣的實際畫面"]
fn weather_foundation_preview(){
    use crate::model::{Dock,Skin};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_WEATHER_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for (name,dock,solar,clock,skin) in [
        ("weather-week",Dock::Top,false,"14:08:00",Skin::ALL[0]),
        ("weather-sun",Dock::Top,true,"14:08:00",Skin::ALL[0]),
        ("weather-night",Dock::Top,true,"20:08:00",Skin::ALL[0]),
        ("weather-side-left",Dock::Left,false,"14:08:00",Skin::ALL[0]),
        ("weather-side-right",Dock::Right,false,"14:08:00",Skin::ALL[0]),
        ("weather-week-cream",Dock::Top,false,"14:08:00",Skin::Cream),
        ("weather-sun-cream",Dock::Top,true,"14:08:00",Skin::Cream),
        ("weather-week-glass",Dock::Top,false,"14:08:00",Skin::GlassDark),
        ("weather-sun-glass",Dock::Top,true,"14:08:00",Skin::GlassDark),
        ("weather-city-picker",Dock::Top,false,"14:08:00",Skin::Classic),
    ] {
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.skin=skin;h.app.settings.pet=false;
        let mut snapshot=crate::weather::parse("杭州",&serde_json::json!({"timezone":"Asia/Shanghai",
            "current":{"temperature_2m":19,"apparent_temperature":18,"relative_humidity_2m":58,"wind_speed_10m":12,"weather_code":2,"is_day":1},
            "daily":{"time":["2026-11-11","2026-11-12","2026-11-13","2026-11-14","2026-11-15","2026-11-16","2026-11-17"],
                "weather_code":[2,0,3,61,80,2,0],"temperature_2m_max":[23,24,22,19,18,20,22],"temperature_2m_min":[15,16,14,13,12,12,14],
                "precipitation_probability_max":[10,0,20,70,55,10,0],"sunrise":["2026-11-11T06:52"],"sunset":["2026-11-11T17:21"]}})).unwrap();
        if clock=="20:08:00" {snapshot.day=false;}
        h.app.weather.snapshot=Some(snapshot);
        if name=="weather-city-picker" {
            let city=|label:&str,region:&str,lat:f64|crate::weather::Position{lat,lon:120.0,label:label.into(),region:region.into()};
            h.app.settings.weather_saved=vec![city("杭州","浙江 · 中國",30.27),city("上海","上海 · 中國",31.23),city("哈爾濱","黑龍江 · 中國",45.75)];
            h.app.settings.weather_city="杭州".into();h.app.settings.weather_position=Some(h.app.settings.weather_saved[0].clone());
            h.app.weather.snapshot.as_mut().unwrap().position=h.app.settings.weather_position.clone();
            h.app.weather.picker_open=true;
        }
        if dock==Dock::Top {h.app.weather.solar_open=solar;h.app.show_page(8);}
        let (w,hgt)=if dock==Dock::Top {(crate::app::CANVAS_W,WIN_H)}else{(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)};
        let mut raster=feature_raster_sized(w,hgt);
        let frames=90;
        for frame in 0..frames {
            h.t+=1.0/60.0;
            let cursor=Some(if dock==Dock::Top{H::local(180.0,185.0)}else{Pos2::new(250.0,580.0)});
            let env=Env{t:h.t,clock:clock.into(),today:"2026-11-11".into(),date:"11月11日 週三".into(),cursor,battery:Some(76),bt_ready:true,..Default::default()};
            let events=vec![egui::Event::PointerMoved(cursor.unwrap())];
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
                events,viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(if dock==Dock::Top{WIN_W}else{w},hgt)),&env);
            });});
            raster.update_textures(&output.textures_delta);
            if frame==frames-1 {
                raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw((w*2.0)as u32,(hgt*2.0)as u32,rgba).unwrap();
                let (x,y,crop_w,crop_h)=if dock==Dock::Top {(200,0,880,(h.app.expanded_height()*2.0+24.0)as u32)}else{
                    let rect=h.app.island_rect(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt)),h.app.w.pos,h.app.h.pos);
                    (if dock==Dock::Left{0}else{880},((rect.top()-12.0)*2.0)as u32,120,((rect.height()+24.0)*2.0)as u32)
                };
                let file=format!("{name}.png");
                image::imageops::crop_imm(&image,x,y,crop_w,crop_h).to_image().save(dest.join(file)).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
#[ignore="按需匯出進入日照檢視時的漣漪與太陽滑入過程"]
fn weather_solar_entry_preview(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_WEATHER_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let mut h=H::new();h.app.settings.pet=false;
    h.app.weather.snapshot=Some(crate::weather::parse("杭州",&serde_json::json!({"timezone":"Asia/Shanghai",
        "current":{"temperature_2m":19,"apparent_temperature":18,"relative_humidity_2m":58,"wind_speed_10m":12,"weather_code":2,"is_day":1},
        "daily":{"time":["2026-11-11"],"weather_code":[2],"temperature_2m_max":[23],"temperature_2m_min":[15],
            "precipitation_probability_max":[10],"sunrise":["2026-11-11T06:52"],"sunset":["2026-11-11T17:21"]}})).unwrap());
    h.app.show_page(8);
    let (w,hgt)=(crate::app::CANVAS_W,WIN_H);
    let mut raster=feature_raster_sized(w,hgt);
    let click=H::local(330.0,94.0);
    // 第 60 幀按下、61 幀鬆開；之後按時間點匯出。
    let shots=[(70,"weather-entry-a"),(85,"weather-entry-b"),(110,"weather-entry-c"),(160,"weather-entry-d")];
    for frame in 0..161 {
        h.t+=1.0/60.0;
        let env=Env{t:h.t,clock:"14:08:00".into(),today:"2026-11-11".into(),date:"11月11日 週三".into(),cursor:Some(click),battery:Some(76),bt_ready:true,..Default::default()};
        let mut events=vec![egui::Event::PointerMoved(click)];
        if frame==60||frame==61 {events.push(egui::Event::PointerButton{pos:click,button:egui::PointerButton::Primary,pressed:frame==60,modifiers:Default::default()});}
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,events,
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
            h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,hgt)),&env);
        });});
        raster.update_textures(&output.textures_delta);
        if let Some((_,name))=shots.iter().find(|(at,_)|*at==frame) {
            raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let image=image::RgbaImage::from_raw((w*2.0)as u32,(hgt*2.0)as u32,rgba).unwrap();
            image::imageops::crop_imm(&image,200,0,880,(h.app.expanded_height()*2.0+24.0)as u32).to_image().save(dest.join(format!("{name}.png"))).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
    assert!(h.app.weather.solar_open,"點選天氣檢視後應進入日照檢視");
}

#[test]
fn island_meshes_stay_valid_after_long_uptime_and_repeated_hover(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.page=0;h.power=(Some(100),true);
    let mut frames=0;
    for days in [0.0,1.0,3.0,7.0,30.0] {
        h.t=days*86400.0;
        for skin in crate::model::Skin::ALL {
            h.app.settings.skin=skin;
            for scale in [1.0,1.25,1.5] {
                h.ctx.set_pixels_per_point(scale);
                for frame in 0..120 {
                    let point=if frame%60<30{H::local(200.0,15.0)}else{Pos2::new(900.0,250.0)};
                    h.cursor=Some(point);h.frame(vec![egui::Event::PointerMoved(point)]);
                    let meshes=h.ctx.tessellate(h.shapes.clone(),h.ctx.pixels_per_point());
                    for primitive in meshes {if let egui::epaint::Primitive::Mesh(mesh)=primitive.primitive {
                        assert!(mesh.is_valid(),"執行 {days} 天后的網格索引必須有效");
                        assert!(mesh.vertices.iter().all(|v|v.pos.is_finite()&&v.uv.is_finite()),"執行 {days} 天后展開不能出現非有限座標");
                        assert!(mesh.vertices.iter().all(|v|v.pos.x.abs()<10000.0&&v.pos.y.abs()<10000.0),"執行 {days} 天后頂點不能拉到視窗遠處");
                    }}
                    frames+=1;
                }
            }
        }
    }
    eprintln!("長執行繪製檢查：{frames} 幀；覆蓋 0、1、3、7、30 天及全部皮膚、三種縮放。");
}


#[test]
fn weather_hidden_page_opens_its_settings_without_panicking(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.set_page_visible(8,false);h.app.show_page(8);
    assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,12);
}

#[test]
fn weather_settings_maps_to_the_module_switch(){
    assert_eq!(crate::extras::settings_tab_module(12),Some(8));
}

#[test]
fn weather_city_draft_keeps_the_saved_location_until_confirmed(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.weather_city="杭州".into();h.app.settings.wecom_notifications=true;
    h.app.settings.feishu_notifications=true;h.app.settings.dingtalk_notifications=true;
    h.app.settings.save();h.app.weather.city_input="北京".into();
    h.frame(vec![]);
    assert_eq!(h.app.settings.weather_city,"杭州");
    let saved=crate::model::Settings::load();
    assert_eq!(saved.weather_city,"杭州");
    assert!(saved.wecom_notifications && saved.feishu_notifications && saved.dingtalk_notifications);
    h.app.set_page_visible(8,false);h.app.add_weather_city();
    assert_eq!(h.app.settings.weather_city,"杭州");assert!(!h.app.weather.loading);
}

#[test]
fn weather_cities_switch_from_the_island_title_and_keep_their_own_cache(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=false;
    let hz=crate::weather::Position{lat:30.27,lon:120.15,label:"杭州".into(),region:"浙江 · 中國".into()};
    let bj=crate::weather::Position{lat:39.90,lon:116.40,label:"北京".into(),region:"北京 · 中國".into()};
    // 設定頁新增兩個城市：只加入列表，搜尋框清空。
    for city in [&hz,&bj] {
        h.app.weather.city_input=city.label.clone();h.app.weather.selected_city=Some(city.clone());
        assert!(h.app.add_weather_city());
        assert!(h.app.settings.weather_city.is_empty(),"新增城市只加入列表，不切換當前城市");
        assert!(h.app.weather.city_input.is_empty());
    }
    assert_eq!(h.app.settings.weather_saved,vec![hz.clone(),bj.clone()]);
    h.app.weather.city_input=hz.label.clone();h.app.weather.selected_city=Some(hz.clone());
    assert!(!h.app.add_weather_city(),"重複城市不再新增");
    assert_eq!(crate::model::Settings::load().weather_saved,vec![hz.clone(),bj.clone()],"城市列表應儲存到設定");
    // 開啟天氣頁，點標題展開下拉框，選「杭州」。
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"北京".into(),position:Some(bj.clone()),temp:-3.0,code:71,day:true,at:chrono::Utc::now().timestamp(),..Default::default()});
    h.app.show_page(8);h.hover(H::local(250.0,65.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="當地")),"標題保持「當地天氣」");
    h.click(H::local(115.0,112.0));h.hover(H::local(330.0,200.0),0.4);
    assert!(h.app.weather.picker_open,"點地點行展開城市下拉");assert!(!h.app.weather.solar_open,"點地點行不應切到日照");
    for phrase in ["自動定位","杭州","北京","新增或管理城市"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"下拉框缺少 {phrase}");
    }
    let row=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="杭州"=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap();
    h.click(row);h.hover(row,0.3);
    assert_eq!(h.app.settings.weather_city,"杭州");assert!(!h.app.weather.picker_open);assert!(!h.app.weather.solar_open);
    assert!(h.app.weather.snapshot.is_none(),"杭州沒有快取時不能顯示北京的天氣");
    // 點面板外收起，不觸發日照檢視。
    h.click(H::local(209.0,147.0));h.hover(H::local(330.0,200.0),0.4);assert!(h.app.weather.picker_open,"無資料時入口在頁面中下方");
    h.click(H::local(330.0,200.0));h.hover(H::local(330.0,200.0),0.3);
    assert!(!h.app.weather.picker_open);assert!(!h.app.weather.solar_open,"點外面只收起下拉框");
    // 刪除當前城市：回到自動定位。
    h.app.remove_weather_city(&hz);
    assert_eq!(h.app.settings.weather_saved,vec![bj.clone()]);assert!(h.app.settings.weather_city.is_empty());assert!(h.app.settings.weather_position.is_none());
}

#[test]
fn weather_negative_temperature_draws_a_hyphen_length_minus_on_the_digit_midline(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=false;
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"哈爾濱".into(),temp:-6.0,code:71,day:true,at:chrono::Utc::now().timestamp(),..Default::default()});
    for _ in 0..60{h.frame(vec![]);}
    let digits=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="6°"=>Some(t.visual_bounding_rect()),_=>None}).expect("膠囊應顯示 6°");
    let bar=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Rect(r) if (digits.left()-r.rect.right()).abs()<4.0 && r.rect.right()<digits.center().x && r.rect.width()>r.rect.height()*1.5 && r.rect.height()<4.0=>Some(r.rect),_=>None}).expect("6° 左側應有手繪減號");
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains('−'))),"不再使用過長的排版減號");
    assert!(bar.width()<digits.height()*0.6,"減號長度與連字元相當");
    assert!((bar.center().y-digits.center().y).abs()<digits.height()*0.2,"減號在數字中線上");
}

#[test]
fn weather_city_dropdown_selects_the_region_without_applying_the_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let correct=crate::weather::Position{lat:23.02677,lon:113.13148,label:"佛山市".into(),region:"廣東 · 中國".into()};
    let wrong=crate::weather::Position{lat:28.73401,lon:98.80612,label:"佛山".into(),region:"迪慶藏族自治州 · 雲南 · 中國".into()};
    h.app.settings.weather_city="杭州".into();h.app.weather.manual_city=true;h.app.weather.city_input="佛山".into();
    h.app.weather.cities.seed("佛山",vec![correct.clone(),wrong]);
    fn draw(h:&mut H,events:Vec<egui::Event>){
        h.t+=1.0/60.0;h.app.t=h.t;
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(560.0,380.0))),time:Some(h.t),events,..Default::default()};
        h.shapes=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.weather_city_picker(ui);});}).shapes;
    }
    for _ in 0..3 {draw(&mut h,vec![]);}
    h.ctx.memory_mut(|m|m.open_popup(egui::Id::new("weather-city-options")));
    for _ in 0..3 {draw(&mut h,vec![]);}
    let point=h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(text) if text.galley.text()=="佛山市"=>Some(text.visual_bounding_rect().center()),_=>None}).expect("下拉框必須顯示完整城市名");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="廣東 · 中國")),"下拉框必須顯示省份");
    draw(&mut h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    draw(&mut h,vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    assert_eq!(h.app.weather.selected_city.as_ref(),Some(&correct));assert_eq!(h.app.weather.draft_city(),Some("佛山市"));
    assert_eq!(h.app.settings.weather_city,"杭州","選擇候選後仍需應用確認");assert!(!h.app.weather.loading);
    assert!(!h.ctx.memory(|m|m.is_popup_open(egui::Id::new("weather-city-options"))));
    h.app.settings.weather_city="佛山市".into();h.app.settings.weather_position=Some(correct.clone());h.app.settings.save();
    let saved=crate::model::Settings::load();assert_eq!(saved.weather_position,Some(correct));
    h.app.weather.city_input="倫敦".into();assert_eq!(h.app.weather.draft_city(),None,"改寫輸入後必須重新選擇地區");
}

#[test]
fn weather_attribution_opens_its_source_without_changing_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(8);h.hover(H::local(300.0,60.0),1.0);
    h.click(H::local(375.0,crate::app::WEATHER_H-7.0));
    assert!(h.cmds.contains(&crate::app::Cmd::OpenUrl("https://open-meteo.com/".into())));
    assert_eq!(h.app.page(),8);
}

#[test]
fn weather_refresh_keeps_music_and_settings_shortcuts_separate(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),playing:true,..Default::default()};
    h.app.show_page(8);h.hover(H::local(300.0,60.0),1.0);
    let rect=|name|h.ctx.read_response(egui::Id::new(("card",true,name))).unwrap().rect;
    let refresh=rect("weather-refresh");
    assert!(!refresh.intersects(rect("header-music")));
    assert!(!refresh.intersects(rect("page-settings")));
}

#[test]
fn resized_font_atlases_stop_reuploading_and_release_unused_sizes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let ctx=egui::Context::default();crate::theme::load_fonts(&ctx);
    ctx.set_embed_viewports(false);
    let child=egui::ViewportId::from_hash_of("font-upload-settings");
    let children=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=children.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let input=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,270.0))),max_texture_side:Some(2048),
            viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()};
        capture.lock().unwrap().push(ctx.run(input,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    let frame=|ppp:f32,settings:bool|{
        let mut viewports=std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect::<egui::ViewportIdMap<_>>();
        if settings{viewports.insert(child,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()});}
        let input=egui::RawInput{viewports,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,270.0))),max_texture_side:Some(2048),..Default::default()};
        let draw=|ctx:&egui::Context|{egui::CentralPanel::default().show(ctx,|ui|{ui.label("專注時間 25:00 · 休息 · 音樂與歌詞");});};
        let root=ctx.run(input,|ctx|{draw(ctx);if settings{ctx.show_viewport_immediate(child,Default::default(),|ctx,_|draw(ctx));}});
        let mut outputs=std::mem::take(&mut *children.lock().unwrap());outputs.push(root);
        outputs
    };
    for settings in [false,true,false] {
        for ppp in [1.0,1.4,1.2,0.8,1.4,1.0] {
            for _ in 0..3{frame(ppp,settings);}
            let start=std::time::Instant::now();let mut bytes=0usize;
            for _ in 0..60 {for output in frame(ppp,settings){
                bytes+=output.textures_delta.set.iter().map(|(_,delta)|delta.image.width()*delta.image.height()*4).sum::<usize>();
            }}
            println!("縮放={ppp:.2} 設定={settings} 60幀上傳={bytes}位元組 耗時={:?}",start.elapsed());
            assert_eq!(bytes,0,"大小穩定後不能重複上傳字形圖集，包括設定同時開啟時");
            let textures=ctx.tex_manager();let textures=textures.read();
            let fonts=textures.allocated().filter(|(_,meta)|meta.name.starts_with("egui_font_texture")).count();
            assert!(fonts<=if settings&&ppp!=1.0{2}else{1},"舊倍率的字型貼圖應釋放：{fonts}");
        }
    }
}

#[test]
fn focus_durations_support_direct_input_and_keep_the_current_timer(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_settings_category(2);h.hover(H::local(200.0,60.0),1.0);
    let remaining=h.app.pomo.remaining;
    for (label,input,expected,rest) in [("25 分鐘","45",45,false),("5 分鐘","12",12,true),("45 分鐘","999",180,false),("12 分鐘","0",1,true),("180 分鐘","abc",180,false)]{
        h.cmds.clear();h.click_text(label);h.frame(vec![]);
        assert!(h.app.is_editing(),"單擊時長應進入輸入狀態");
        assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"島內輸入必須取得原生鍵盤焦點");
        h.frame(vec![egui::Event::Text(input.into())]);
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        h.frame(vec![]);
        let settings=crate::model::Settings::load();
        assert_eq!(if rest{settings.rest_minutes}else{settings.focus_minutes},expected,"輸入需校驗範圍並儲存");
        assert!(!h.app.is_editing());assert_eq!(h.app.pomo.remaining,remaining,"調整時長不能重置當前計時");
    }
    h.click_text("180 分鐘");h.frame(vec![]);h.frame(vec![egui::Event::Text("30".into())]);
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    h.frame(vec![]);assert_eq!(h.app.settings.focus_minutes,180,"Esc 應取消本次輸入");
    h.click_text("180 分鐘");h.frame(vec![]);h.frame(vec![egui::Event::Text("50".into())]);
    h.click(H::local(268.0,62.0));h.frame(vec![]);
    assert_eq!(h.app.settings.focus_minutes,51,"點選加號應先儲存輸入，再增加一分鐘");
    assert_eq!(crate::model::Settings::load().focus_minutes,51);
    h.click_text("51 分鐘");h.frame(vec![]);h.frame(vec![egui::Event::Text("40".into())]);
    h.click(H::local(50.0,65.0));h.frame(vec![]);
    assert_eq!(h.app.settings.focus_minutes,40,"點選別處應儲存輸入");
    assert!(!h.app.is_editing());
}

#[test]
fn enlarged_island_preserves_font_resolution_with_a_separate_settings_window(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_settings_category(2);h.hover(H::local(200.0,60.0),1.0);
    h.app.open_settings();h.app.settings_tab=2;h.ctx.set_embed_viewports(false);
    let captured=std::sync::Arc::new(Mutex::new(None));let capture=captured.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let input=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(690.0,560.0))),
            viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(input,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    let mut raster=crate::preview_raster::Raster::new(WIN_W as usize,WIN_H as usize);
    raster.update_textures(&egui::TexturesDelta{set:vec![(egui::TextureId::default(),egui::epaint::ImageDelta::full(h.ctx.fonts(|fonts|fonts.image()),egui::TextureOptions::LINEAR))],..Default::default()});
    for scale in [1.0,1.2,1.4] {
        raster.w=(WIN_W*scale) as usize;raster.h=(WIN_H*scale) as usize;raster.buf.resize(raster.w*raster.h,[0.0;4]);
        for frame in 0..3 {
            h.t+=1.0/60.0;
            let input=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),
                time:Some(h.t),events:vec![egui::Event::PointerMoved(H::local(70.0,40.0))],viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
            let root=h.ctx.run(input,|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));
                    h.app.frame(ui,win,&Env{t:h.t,cursor:Some(H::local(70.0,40.0)),clock:"09:41:00".into(),..Default::default()});
                });
                h.app.settings_ui(ctx,false);
            });
            let child=captured.lock().unwrap().take().unwrap();
            assert_eq!(child.pixels_per_point,1.0,"設定視窗不跟隨島體變大");
            for (shapes,ppp) in [(&root.shapes,scale),(&child.shapes,1.0)] {
                for shape in shapes {if let egui::Shape::Text(text)=&shape.shape {
                    assert!((text.galley.pixels_per_point-ppp).abs()<0.0001,"各視窗應使用各自物理尺寸的字形");
                }}
            }
            raster.update_textures(&child.textures_delta);raster.update_textures(&root.textures_delta);
            if frame==2 {if let Some(dest)=std::env::var_os("DAOZAI_FOCUS_SCALE_PREVIEW") {
                let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
                raster.buf.fill([0.18,0.20,0.24,1.0]);raster.draw(&h.ctx.tessellate(root.shapes,root.pixels_per_point),root.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest.join(format!("focus-size-{}.png",(scale*100.0) as u32)),&rgba,(WIN_W*scale) as u32,(WIN_H*scale) as u32,image::ColorType::Rgba8).unwrap();
            }}
            raster.free_textures(&root.textures_delta);
            raster.free_textures(&child.textures_delta);
        }
    }
}

#[test]
fn opaque_surface_gets_a_contour_with_every_skin_and_dock(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for skin in crate::model::Skin::ALL {for dock in crate::model::Dock::ALL {for island_shape in crate::model::IslandShape::ALL {for opaque_surface in [false,true] {
        let mut h=H::new();h.app.settings.skin=skin;h.app.settings.dock=dock;h.app.settings.island_shape=island_shape;h.app.settings.glass_blur=false;
        let size=if dock.is_side(){Vec2::new(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)}else{Vec2::new(WIN_W,WIN_H)};
        let mut outlined=false;
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),..Default::default()};
        let _=h.ctx.run(raw,|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                let win=Rect::from_min_size(ui.max_rect().min,size);
                let out=h.app.frame(ui,win,&Env{opaque_surface,..Default::default()});
                outlined=out.glass.is_some_and(|shapes|!shapes.is_empty());
            });
        });
        assert_eq!(outlined,opaque_surface,"皮膚={skin:?}，停靠={dock:?}，形狀={island_shape:?}");
    }}}}
}

#[test]
fn changing_qq_metadata_cannot_spawn_lyrics_workers_while_one_is_running(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let (release,wait)=std::sync::mpsc::channel();
    let worker=std::thread::spawn(move||{let _=wait.recv();});
    let id=worker.thread().id();h.app.ly_worker=Some(worker);
    h.app.opts.fetch_lyrics=true;h.app.settings.island_lyrics=true;
    for step in 0..1440 {
        h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:format!("歌曲{}",step%2),artist:"歌手".into(),duration:120.0,playing:true,..Default::default()};
        h.t=step as f64*0.25;h.frame(vec![]);
        assert_eq!(h.app.ly_worker.as_ref().unwrap().thread().id(),id,"舊請求未結束時不得另開取詞執行緒");
        assert!(h.app.ly.rx.is_none(),"不能為抖動的歌曲堆積請求");
    }
    release.send(()).unwrap();h.app.ly_worker.take().unwrap().join().unwrap();
}

#[test]
fn top_bar_modes_preserve_legacy_settings_and_save_the_selected_display_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let _h=H::new();
    use crate::model::Settings;
    let path=crate::model::config_dir().join("settings.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bounds=crate::display_layout::Bounds{left:-1920,top:-400,right:0,bottom:680};
    let display=crate::display_layout::Display{id:"test".into(),label:"測試屏".into(),bounds,work_top:-368,primary:true};
    for enabled in [false,true] {
        std::fs::write(&path,format!("avoid_top_bar={}\n",enabled as u8)).unwrap();
        let mut settings=Settings::load();
        assert_eq!(settings.avoid_top_bar,enabled);assert!(!settings.top_bar_on_top,"舊配置預設沿用避讓方式");
        for on_top in [false,true] {
            settings.top_bar_on_top=on_top;settings.save();let saved=Settings::load();
            assert_eq!((saved.avoid_top_bar,saved.top_bar_on_top),(enabled,on_top));
            assert_eq!(crate::display_layout::placement_bounds(&display,saved.avoids_top_bar()).top,if enabled&&!on_top{-368}else{-400},"置頂方式應回到螢幕頂邊；關閉適配後不避讓");
        }
    }
}

#[test]
fn top_bar_settings_switch_modes_and_respect_the_master_switch(){
    fn draw(h:&mut H,events:Vec<egui::Event>,raster:&mut crate::preview_raster::Raster)->egui::FullOutput {
        h.t+=0.1;let time=h.t;
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,time:Some(time),focused:true,events:events.clone(),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1800.0))),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);
        let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);child
    }
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dark in [true,false] {
        let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
        h.app.settings.settings_theme=if dark{crate::settings_theme::SettingsTheme::Dark}else{crate::settings_theme::SettingsTheme::Light};
        let mut raster=crate::preview_raster::Raster::new(690,1800);
        for _ in 0..3{let _=draw(&mut h,vec![],&mut raster);}
        for (on_top,label) in [(true,"島仔置頂"),(false,"避開頂欄")] {
            let output=draw(&mut h,vec![],&mut raster);
            let position=output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>{
                assert!(s.clip_rect.contains_rect(t.visual_bounding_rect()),"顯示方式按鈕不能裁切");Some(t.visual_bounding_rect().center())
            },_=>None}).expect("顯示方式應提供避讓和置頂兩個選項");
            for pressed in [true,false] {let _=draw(&mut h,vec![egui::Event::PointerMoved(position),egui::Event::PointerButton{pos:position,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],&mut raster);}
            assert_eq!(h.app.settings.top_bar_on_top,on_top);assert_eq!(crate::model::Settings::load().top_bar_on_top,on_top,"切換後立即儲存");
            if let Some(dest)=std::env::var_os("DAOZAI_TOP_BAR_PREVIEW_DIR") {
                let output=draw(&mut h,vec![],&mut raster);raster.buf.fill(if dark{[0.08,0.08,0.09,1.0]}else{[0.96,0.96,0.97,1.0]});
                raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
                image::save_buffer(dest.join(format!("top-bar-{}-{}.png",if dark{"dark"}else{"light"},if on_top{"on-top"}else{"avoid"})),&rgba,690,1800,image::ColorType::Rgba8).unwrap();
            }
        }
        h.app.settings.avoid_top_bar=false;
        let output=draw(&mut h,vec![],&mut raster);
        assert!(!output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="島仔置頂")),"關閉適配後收起顯示方式選項");
    }
}

#[test]
fn v0649_settings_and_mail_accounts_survive_upgrade(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let _h=H::new();
    let directory=crate::model::config_dir();
    std::fs::create_dir_all(&directory).unwrap();
    let legacy=concat!("settings_theme=dark\npet=0\nminimal=1\nbrowser_minimal=1\n",
        "visible_pages=10111111\npage_order=5,2,0,1,3,4,6,7\nshow_battery=0\n",
        "system_hud=0\nread_notifications=1\nnotifications=0\nsound=1\n",
        "lyrics_enabled=0\nisland_lyrics=1\nlyric_delay_ms=-650\nhide_fullscreen=0\n",
        "focus_minutes=45\nrest_minutes=10\nwater_minutes=60\nserving_ml=300\nwater_start=8\nwater_end=23\n",
        "mail_enabled=0\nmail_code=0\nmail_interval=300\nmail_ics=30\ncalendar_prompt=0\ncalendar_auto_sync=0\n");
    std::fs::write(directory.join("settings.txt"),legacy).unwrap();
    let upgraded=crate::model::Settings::load();
    assert!(upgraded.display_id.is_empty() && upgraded.avoid_apps.is_empty());
    assert!(upgraded.avoid_browsers && upgraded.bluetooth_notifications);
    assert!(!upgraded.clock_12h && !upgraded.notification_preview);
    assert_eq!(upgraded.clock_period,0);assert_eq!(upgraded.mail_code_hide_seconds,60);
    for _ in 0..2 {
        crate::model::Settings::load().save();
        let saved=std::fs::read_to_string(directory.join("settings.txt")).unwrap();
        for line in legacy.lines().filter(|line|!line.starts_with("visible_pages=")&&!line.starts_with("page_order=")){assert!(saved.lines().any(|value|value==line),"升級儲存不能改變舊設定：{line}");}
        assert!(saved.lines().any(|value|value=="visible_pages=101111111"),"舊頁面開關應保留，並預設開啟天氣頁");
        assert!(saved.lines().any(|value|value=="page_order=5,2,0,1,3,4,6,7,8"),"舊順序應保留，並把天氣頁接在最後");
    }
    std::fs::write(directory.join("mail-accounts.json"),br#"[{"email":"OldUser@qq.com","provider":"qq","enabled":true},{"email":"old@icloud.com","provider":"icloud","enabled":false}]"#).unwrap();
    let accounts=crate::mail::load_accounts();
    assert_eq!(accounts.len(),2);assert!(accounts.iter().all(|account|account.custom.is_none()));
    assert_eq!(accounts[0].credential_key(),"olduser@qq.com");
    assert_eq!(accounts[0].endpoint().unwrap(),("imap.qq.com".into(),993,"OldUser@qq.com".into()));
    assert!(!accounts[1].enabled);
    crate::mail::save_accounts(&accounts);assert_eq!(crate::mail::load_accounts(),accounts);
}

fn editing_local_calendar_clears_outdated_reminders() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for reschedule in [false,true] {
        let mut h=H::new();
        let mut draft=crate::local_calendar::Draft::new(chrono::Local::now().date_naive()+chrono::Duration::days(1));
        draft.title="原日程".into();
        let entry=draft.entry().unwrap();
        h.app.calendar.local=vec![entry.clone()];
        h.app.alert("專注計時","休息",1,false);
        h.frame(vec![]);
        h.app.meeting_alert(entry.event.clone(),"本機日程".into(),7);
        let mut other=entry.event.clone();other.uid="另一日程".into();
        h.app.meeting_alert(other,"本機日程".into(),7);
        h.app.open_calendar_editor(Some(&entry.event.uid));
        let draft=h.app.calendar.draft.as_mut().unwrap();
        if reschedule {
            let day=(chrono::Local::now().date_naive()+chrono::Duration::days(2)).to_string();
            draft.start_date=day.clone();draft.end_date=day;
        } else {draft.reminder=None;}
        h.app.save_calendar_editor();
        assert!(h.app.calendar.draft.is_none());
        assert!(!h.app.notice.as_ref().is_some_and(|a|a.meeting.as_ref().is_some_and(|(m,_)|m.uid==entry.event.uid)),"修改後不能繼續顯示舊提醒");
        assert!(!h.app.alerts.iter().any(|a|a.meeting.as_ref().is_some_and(|(m,_)|m.uid==entry.event.uid)),"修改後不能彈出排隊中的舊提醒");
        assert!(h.app.alerts.iter().any(|a|a.meeting.as_ref().is_some_and(|(m,_)|m.uid=="另一日程")));
    }
}

#[test]
fn meeting_notice_can_close_without_deleting_event_or_other_reminders(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.frame(vec![]);
    let event=crate::mail::Meeting{all_day:false,title:"當前會議".into(),start:chrono::Local::now().timestamp()+600,end:None,at:String::new(),uid:"dismiss-current".into()};
    h.app.calendar.events.push(event.clone());
    h.app.mail.fired.insert(crate::calendar::dedupe_key(&event));
    h.app.meeting_alert(event,"飛書".into(),7);
    h.app.meeting_alert(crate::mail::Meeting{all_day:false,title:"下一場會議".into(),start:chrono::Local::now().timestamp()+1200,end:None,at:String::new(),uid:"dismiss-next".into()},"釘釘".into(),7);
    for _ in 0..45{h.frame(vec![]);}
    let page=h.app.page;
    let button=h.ctx.read_response(egui::Id::new(("card",true,"meeting-dismiss"))).expect("會議提醒必須有關閉按鈕").rect;
    h.click(button.center());
    assert!(h.app.notice.is_none());assert_eq!(h.app.page,page);
    assert_eq!(h.app.calendar.events.len(),1,"關閉提醒不能刪除日程");
    h.frame(vec![]);
    assert_eq!(h.app.notice.as_ref().unwrap().meeting.as_ref().unwrap().0.uid,"dismiss-next");
}

#[test]
fn calendar_hides_ended_events_and_keeps_all_day_out_of_featured_card() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::NaiveDate::from_ymd_opt(2026,10,28).unwrap();
    let at=|hour|day.and_hms_opt(hour,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp();
    h.app.calendar.view.day=day;h.app.calendar.view.month=crate::calendar::month_start(day);h.app.calendar.view.loaded=Some(h.app.calendar.view.month);
    h.app.calendar.view.events=vec![
        crate::mail::Meeting{uid:"all-day".into(),title:"全天安排".into(),start:at(0),end:None,all_day:true,at:String::new()},
        crate::mail::Meeting{uid:"past".into(),title:"過去的安排".into(),start:at(7),end:Some(at(8)),all_day:false,at:String::new()},
        crate::mail::Meeting{uid:"future".into(),title:"下一場安排".into(),start:at(11),end:Some(at(12)),all_day:false,at:String::new()},
    ];
    h.app.open_calendar(None);h.hover(H::local(300.0,40.0),1.0);
    let contains=|h:&H,text:&str|h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()==text));
    assert!(!contains(&h,"過去的安排"));assert!(contains(&h,"全天安排"));
    assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="11:00" && t.galley.job.sections[0].format.font_id.size>=23.0)));
    h.click(H::local(270.0,43.0));h.frame(vec![]);assert!(contains(&h,"過去的安排"));
    h.click(H::local(270.0,43.0));h.frame(vec![]);assert!(!contains(&h,"過去的安排"));
}

#[test]
fn local_calendar_failed_save_keeps_draft_and_does_not_change_events() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    std::fs::create_dir_all(crate::model::config_dir().join("local-calendar.json")).unwrap();
    h.app.open_calendar_editor(None);h.app.calendar.draft.as_mut().unwrap().title="不能丟失的草稿".into();
    h.app.save_calendar_editor();
    assert!(h.app.calendar.local.is_empty());let draft=h.app.calendar.draft.as_ref().unwrap();
    assert_eq!(draft.title,"不能丟失的草稿");assert!(!draft.error.is_empty());
}

#[test]
#[ignore="按需測量 ICS 匯入後日歷頁的繪製耗時"]
fn imported_ics_calendar_frame_cost() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (count,same_day) in [(0,false),(100,false),(1000,false),(1000,true),(5000,true)] {
        let mut h=H::new();h.app.settings.pet=false;
        let start=chrono::Local::now().date_naive().and_hms_opt(12,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().with_timezone(&chrono::Utc);
        let mut ics=String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
        for index in 0..count {
            let at=start+chrono::Duration::days(if same_day {0} else {index%365});
            ics.push_str(&format!("BEGIN:VEVENT\r\nUID:import-{index}\r\nSUMMARY:匯入日程 {index}\r\nDTSTART:{}\r\nDTEND:{}\r\nEND:VEVENT\r\n",at.format("%Y%m%dT%H%M%SZ"),(at+chrono::Duration::hours(1)).format("%Y%m%dT%H%M%SZ")));
        }
        ics.push_str("END:VCALENDAR\r\n");
        let at=std::time::Instant::now();h.app.import_ics(&[("sample.ics".into(),ics)]);let import=at.elapsed();
        assert_eq!(h.app.calendar.local.len(),count as usize);
        h.app.open_calendar(None);h.hover(H::local(300.0,40.0),0.2);
        let at=std::time::Instant::now();for _ in 0..3 {h.frame(vec![]);}let frames=at.elapsed()/3;
        h.app.show_page(0);h.hover(H::local(300.0,40.0),0.2);
        let at=std::time::Instant::now();for _ in 0..3 {h.frame(vec![]);}let other=at.elapsed()/3;
        println!("count={count} same_day={same_day} import_ms={:.2} calendar_frame_ms={:.2} other_frame_ms={:.2}",import.as_secs_f64()*1000.0,frames.as_secs_f64()*1000.0,other.as_secs_f64()*1000.0);
    }
}

#[test]
fn ics_import_preserves_recurring_overnight_and_all_day_events_on_reimport() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::Local::now().date_naive();
    let utc=|hour,minute|day.and_hms_opt(hour,minute,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().with_timezone(&chrono::Utc);
    let start=utc(12,0);let overnight=utc(23,30);
    let ics=format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:daily\r\nSUMMARY:每日安排\r\nDTSTART:{}\r\nDTEND:{}\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:overnight\r\nSUMMARY:跨夜安排\r\nDTSTART:{}\r\nDTEND:{}\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:all-day\r\nSUMMARY:兩天休假\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",start.format("%Y%m%dT%H%M%SZ"),(start+chrono::Duration::hours(1)).format("%Y%m%dT%H%M%SZ"),overnight.format("%Y%m%dT%H%M%SZ"),(overnight+chrono::Duration::hours(1)).format("%Y%m%dT%H%M%SZ"),day.format("%Y%m%d"),(day+chrono::Duration::days(2)).format("%Y%m%d"));
    let files=vec![("sample.ics".into(),ics)];
    h.app.import_ics(&files);assert_eq!(h.app.calendar.local.len(),5);
    h.app.import_ics(&files);assert_eq!(h.app.calendar.local.len(),5,"重複匯入應跳過已有日程");
    h.app.calendar.local=crate::local_calendar::load().unwrap();
    assert_eq!(h.app.calendar_entries(day).len(),3);
    assert_eq!(h.app.calendar_entries(day+chrono::Duration::days(1)).len(),3);
    assert_eq!(h.app.calendar_entries(day+chrono::Duration::days(2)).len(),1);
    assert!(h.app.calendar_entries(day+chrono::Duration::days(3)).is_empty());
}

#[test]
fn local_calendar_editor_makes_storage_scope_visible_with_connected_accounts() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let cfg=crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"test".into(),href:None,server:None};
    crate::calendar::save_config(&cfg).unwrap();h.app.calendar=crate::app::CalendarState::load();
    let remote=crate::mail::Meeting{all_day:false,title:"飛書會議".into(),start:chrono::Local::now().timestamp()+3600,end:None,at:String::new(),uid:"remote-event".into()};
    h.app.calendar.events.push(remote.clone());
    h.app.open_calendar(None);h.hover(H::local(300.0,40.0),1.0);
    h.click(H::local(124.0,18.0));h.hover(H::local(300.0,40.0),0.3);
    assert!(h.app.calendar.draft.is_some());
    let visible=|h:&H,text:&str|h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()==text));
    assert!(visible(&h,"僅儲存在本機"));assert!(visible(&h,"儲存到本機"));
    h.click(H::local(334.0,199.0));h.frame(vec![]);
    assert!(!h.app.calendar.draft.as_ref().unwrap().error.is_empty());
    assert!(visible(&h,"僅儲存在本機"),"填寫錯誤時仍需說明儲存範圍");
    h.app.calendar.draft.as_mut().unwrap().title="自己的安排".into();
    h.click(H::local(334.0,199.0));h.frame(vec![]);
    assert!(h.app.calendar.draft.is_none());assert_eq!(h.app.calendar.local.len(),1);
    assert_eq!(crate::local_calendar::load().unwrap()[0].event.title,"自己的安排");
    assert_eq!(h.app.calendar.events.len(),1);assert_eq!(h.app.calendar.events[0].uid,remote.uid);
    assert_eq!(crate::calendar::load_config().unwrap(),cfg);
    let uid=h.app.calendar.local[0].event.uid.clone();h.app.open_calendar_editor(Some(&uid));h.frame(vec![]);
    assert!(visible(&h,"僅儲存在本機"));assert!(visible(&h,"儲存到本機"));
}

#[test]
fn local_calendar_can_create_edit_reload_and_remind_without_accounts() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_calendar(None);h.app.open_calendar_editor(None);
    assert!(h.app.is_editing());
    h.app.save_calendar_editor();assert!(h.app.calendar.local.is_empty());
    assert!(!h.app.calendar.draft.as_ref().unwrap().error.is_empty());
    let start=chrono::Local::now()+chrono::Duration::minutes(5);
    let end=start+chrono::Duration::hours(1);
    let draft=h.app.calendar.draft.as_mut().unwrap();draft.title="自己的安排".into();
    draft.start_date=start.format("%Y-%m-%d").to_string();draft.start_time=start.format("%H:%M").to_string();
    draft.end_date=end.format("%Y-%m-%d").to_string();draft.end_time=end.format("%H:%M").to_string();
    h.app.save_calendar_editor();
    assert!(h.app.calendar.draft.is_none());assert_eq!(h.app.calendar.local.len(),1);
    let uid=h.app.calendar.local[0].event.uid.clone();
    assert_eq!(crate::local_calendar::load().unwrap()[0].event.title,"自己的安排");
    let entries=h.app.calendar_entries(start.date_naive());assert_eq!(entries.len(),1);assert!(matches!(entries[0].sources.as_slice(),[crate::calendar_ui::Source::Local]));
    h.frame(vec![]);
    assert!(h.app.history.iter().any(|alert|alert.meeting.as_ref().is_some_and(|(event,source)|event.uid==uid && source=="本機日程")));
    h.app.open_calendar_editor(Some(&uid));h.app.calendar.draft.as_mut().unwrap().title="改後的安排".into();h.app.save_calendar_editor();
    let saved=crate::local_calendar::load().unwrap();assert_eq!(saved.len(),1);assert_eq!(saved[0].event.uid,uid);assert_eq!(saved[0].event.title,"改後的安排");
    h.app.open_calendar_editor(Some(&uid));h.hover(H::local(300.0,40.0),1.0);
    h.click(H::local(84.0,199.0));assert_eq!(h.app.calendar.local.len(),1,"刪除先確認");
    h.click(H::local(84.0,199.0));assert!(h.app.calendar.local.is_empty());assert!(crate::local_calendar::load().unwrap().is_empty());
}

#[test]
fn calendar_month_errors_remain_visible_until_retry_or_month_change(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let cfg=crate::calendar::Config{provider:crate::calendar::Provider::WeCom,username:"test".into(),href:None,server:None};
    crate::calendar::save_config(&cfg).unwrap();h.app.calendar=crate::app::CalendarState::load();
    h.app.request_calendar_month(true);h.app.pending.clear();
    let month=h.app.calendar.view.month;
    h.app.calendar_event(crate::calendar::Event::MonthLoaded{config:cfg,month,result:Err("日程時間無法解析".into())});
    h.app.request_calendar_month(false);
    assert!(!h.app.calendar.view.loading,"失敗後不能逐幀重試，覆蓋錯誤狀態");
    assert!(h.app.pending.is_empty());
    h.app.request_calendar_month(true);
    assert!(h.app.calendar.view.loading,"使用者手動重試應立即生效");
}

#[test]
fn calendar_combines_platforms_and_settings_tabs_do_not_filter_events() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    let feishu=crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"飛書帳號".into(),href:None,server:None};
    let ding=crate::calendar::Config{provider:crate::calendar::Provider::DingTalk,username:"釘釘帳號".into(),..feishu.clone()};
    crate::calendar::save_config(&feishu).unwrap();crate::calendar::save_config(&ding).unwrap();
    h.app.calendar=crate::app::CalendarState::load();
    let day=chrono::Local::now().date_naive();
    let event=|uid:&str|crate::mail::Meeting{all_day:false,end:None,title:uid.into(),start:chrono::Local::now().timestamp()+60,at:"18:00".into(),uid:uid.into()};
    for (cfg,uid) in [(&feishu,"飛書會議"),(&ding,"釘釘會議")] {
        h.app.calendar_event(crate::calendar::Event::Synced{config:cfg.clone(),events:vec![event(uid)],at:"17:00".into()});
    }
    assert_eq!(h.app.calendar_entries(day).len(),2);
    h.frame(vec![]);
    for provider in [feishu.provider,ding.provider] {
        assert!(h.app.history.iter().any(|alert|alert.meeting.as_ref().is_some_and(|(_,source)|source==provider.label())),"所有平臺都應提醒");
    }
    h.app.select_calendar_account(feishu.clone());
    assert_eq!(h.app.calendar_entries(day).len(),2);
    assert_eq!(h.app.calendar.form.username,feishu.username);
    h.app.select_calendar_provider(crate::calendar::Provider::WeCom);
    assert_eq!(h.app.calendar_entries(day).len(),2);
    assert!(h.app.calendar.form.username.is_empty());
    h.app.request_calendar_month(true);
    h.app.calendar_event(crate::calendar::Event::MonthLoaded{config:feishu.clone(),month:crate::calendar::month_start(day),result:Ok(vec![event("飛書月曆")])});
    assert!(h.app.calendar.view.loading,"另一平臺尚未返回，仍應保持同步狀態");
    h.app.calendar_event(crate::calendar::Event::MonthLoaded{config:ding.clone(),month:crate::calendar::month_start(day),result:Err("釘釘暫時不可用".into())});
    assert!(!h.app.calendar.view.loading);
    assert_eq!(h.app.calendar_entries(day).len(),2,"單平臺失敗不能清掉另一平臺或自己的提醒快取");
    let shared=event("同一個會議");
    for config in [&feishu,&ding] {
        h.app.calendar_event(crate::calendar::Event::MonthLoaded{config:config.clone(),month:crate::calendar::month_start(day),result:Ok(vec![shared.clone()])});
    }
    let entries=h.app.calendar_entries(day);
    assert_eq!(entries.len(),1);
    assert_eq!(entries[0].sources.len(),2,"同一個會議只顯示一次，但保留兩個平臺的來源");
    h.app.select_calendar_account(feishu.clone());
    h.app.forget_calendar();
    assert_eq!(h.app.calendar_entries(day).len(),1);
    h.app.calendar_event(crate::calendar::Event::Synced{config:feishu,events:vec![event("舊連線結果")],at:"17:01".into()});
    assert_eq!(h.app.calendar_entries(day).len(),1);
}

#[test]
fn switching_music_source_discards_previous_lyrics() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
    h.frame(vec![]);
    for source in ["QQMusic.exe","cloudmusic.exe","汽水音樂","KuGou.exe","QQMusic.exe"] {
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:01]舊來源歌詞"));
        h.media.source=source.into();
        h.frame(vec![]);
        assert!(h.app.ly.lyrics.is_none(),"切換到 {source} 後必須重新選擇歌詞來源");
    }
}

#[test]
fn closing_a_player_then_starting_native_netease_restores_music_and_lyrics(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{source:"SodaMusic.exe".into(),title:"舊歌曲".into(),artist:"歌手".into(),playing:true,duration:120.0,..Default::default()};h.frame(vec![]);
    h.app.set_lyrics("舊歌曲","歌手",120.0,crate::lyrics::parse_lrc("[00:01]舊歌詞"));
    h.media=Default::default();h.frame(vec![]);assert!(h.app.media.title.is_empty());assert!(h.app.ly.lyrics.is_none());
    h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),song_id:Some(123),title:"新歌曲".into(),artist:"歌手".into(),playing:true,position:7.0,duration:120.0,..Default::default()};h.frame(vec![]);
    h.app.set_lyrics("新歌曲","歌手",120.0,crate::lyrics::parse_lrc("[00:05]新歌詞\n[00:10]下一句"));h.frame(vec![]);
    assert_eq!(h.app.media.title,"新歌曲");assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("新歌詞"));
    h.media.song_id=Some(124);h.frame(vec![]);assert!(h.app.ly.lyrics.is_none(),"歌名相同但歌曲 ID 不同仍需重新取詞");
}

#[test]
fn settings_keep_dark_buttons_when_system_theme_changes() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let h=H::new();
    for theme in [egui::Theme::Light,egui::Theme::Dark,egui::Theme::Light] {
        let _=h.ctx.run(egui::RawInput{system_theme:Some(theme),..Default::default()},|ctx| {
            assert!(ctx.style().visuals.widgets.inactive.weak_bg_fill.r()<100,"系統淺色模式不能把深色設定裡的按鈕變成白底");
        });
    }
}

#[test]
fn expanded_settings_routes_preserve_inline_controls(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (page,tab) in crate::app::PAGE_SETTINGS_TABS.into_iter().enumerate(){
        let mut h=H::new();h.app.show_page(page);h.hover(H::local(200.0,40.0),1.0);
        h.click(H::local(394.0,18.0));
        assert!(h.app.settings_open,"頁面 {page} 應開啟設定視窗");
        assert_eq!(h.app.settings_tab,tab);assert_eq!(h.app.page,page);
        assert!(h.app.subpage.is_none());
    }
    for (page,point) in [(1,H::local(111.0,18.0)),(2,H::local(149.0,110.0))]{
        let mut h=H::new();h.app.show_page(page);h.hover(H::local(200.0,40.0),1.0);
        h.click(point);assert_eq!(h.app.subpage,Some(page));assert!(!h.app.settings_open);
        h.click(H::local(394.0,18.0));assert!(h.app.settings_open);
        assert_eq!(h.app.subpage,Some(page),"開啟完整設定後應保留島內調整頁");
    }
    let mut h=H::new();h.app.show_page(3);h.hover(H::local(200.0,40.0),1.0);
    h.click(H::local(330.0,18.0));assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::AddFiles)));
    assert!(!h.app.settings_open);
    h.app.show_page(6);h.hover(H::local(200.0,40.0),1.0);h.click(H::local(319.0,18.0));
    assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::QuotaRefresh)));assert!(!h.app.settings_open);
}

#[test]
fn quality_late_calendar_response_does_not_restore_disconnected_account() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let config=crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"old".into(),href:None,server:None};
    h.app.forget_calendar();
    h.app.calendar_event(crate::calendar::Event::Synced{config:config.clone(),events:vec![],at:"12:00".into()});
    assert!(!h.app.calendar.connected && h.app.calendar.last_sync.is_empty());
    h.app.calendar.config=Some(crate::calendar::Config{username:"new".into(),..config.clone()});
    h.app.calendar.connected=true;
    h.app.calendar_event(crate::calendar::Event::Failed{config,message:"舊連線失敗".into()});
    assert!(h.app.calendar.connected && h.app.calendar.error.is_empty());
}

#[test]
fn quality_distinct_reminders_are_queued_instead_of_overwritten() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let meeting=crate::mail::Meeting{ all_day:false,title:"會議一".into(),start:chrono::Local::now().timestamp()+600,end:None,at:String::new(),uid:"one".into()};
    h.app.meeting_alert(meeting.clone(),"飛書".into(),7);
    h.app.meeting_alert(crate::mail::Meeting{ all_day:false,title:"會議二".into(),uid:"two".into(),..meeting.clone()},"飛書".into(),7);
    assert_eq!(h.app.alerts.len(),2,"同時到期的兩場會議都需要提醒");
    h.app.meeting_alert(meeting,"飛書".into(),7);assert_eq!(h.app.alerts.len(),2,"同一會議重複更新應合併");
    h.app.alert("待辦到時間了","任務一",4,true);h.app.alert("待辦到時間了","任務二",4,true);
    assert_eq!(h.app.alerts.iter().filter(|a|a.title=="待辦到時間了").count(),2);
    h.app.alert("系統音量","10%",0,false);h.app.alert("系統音量","20%",0,false);
    assert_eq!(h.app.alerts.iter().filter(|a|a.title=="系統音量").count(),1);
}

#[test]
fn quality_removed_mail_accounts_ignore_in_flight_results() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail_event(crate::mail::Event::Status{account:"removed@example.com".into(),status:crate::mail::Status::Ready});
    h.app.mail_event(crate::mail::Event::Mail{item:crate::mail::Item {received_unix: None, account:"removed@example.com".into(),from:"發件人".into(),subject:"延遲郵件".into(),time:String::new(),code:None,html: String::new().into(), summary:String::new().into(),links:vec![],meeting:None,uid:1},new:true});
    assert!(h.app.mail.status.is_empty() && h.app.mail.recent.is_empty());
    assert!(h.app.notification_bell_until<=h.t);
}

#[test]
fn glass_pill_rim_stays_on_outline_while_collapsing() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for ppp in [1.0,1.25,1.5,2.0] {
        let mut h=H::new();
        h.app.settings.skin=crate::model::Skin::GlassLight;
        h.app.settings.island_shape=crate::model::IslandShape::Pill;
        h.ctx.set_zoom_factor(ppp);
        h.hover(Pos2::new(WIN_W/2.0,15.0),1.5);
        assert!(h.app.progress()>0.99);
        h.cursor=None;
        let mut raster=crate::preview_raster::Raster::new((crate::app::CANVAS_W*ppp) as usize,(WIN_H*ppp) as usize);
        let mut checked=0;
        for frame in 0..100 {
            h.frame(vec![egui::Event::PointerGone]);
            let rims=h.shapes.iter().filter_map(|s|match &s.shape {
                egui::Shape::Rect(r) if r.rect.width()>100.0 && r.rect.height()>=10.0=>Some((s.clone(),r.rect,if r.stroke.is_empty(){r.fill.a() as f32/255.0}else{0.0})),
                _=>None,
            }).collect::<Vec<_>>();
            for (shape,rect,alpha) in rims {
                checked+=1;
                raster.buf.fill([0.0;4]);
                raster.draw(&h.ctx.tessellate(vec![shape],ppp),ppp);
                let y=(rect.center().y*ppp).floor() as usize;
                let left=((rect.left()+4.0)*ppp).ceil() as usize;
                let right=((rect.right()-4.0)*ppp).floor() as usize;
                for x in left..right {
                    assert!((raster.buf[y*raster.w+x][3]-alpha).abs()<0.02,"膠囊中心出現接縫：ppp={ppp}, frame={frame}, rect={rect:?}, x={x}, alpha={alpha}, actual={}",raster.buf[y*raster.w+x][3]);
                }
            }
        }
        assert!(checked>=100,"應覆蓋完整收起動畫");
    }
}

#[test]
fn oled_shift_waits_for_the_panel_to_finish_closing() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.oled_shift=true;
    h.app.show_page(0);
    h.hover(H::local(200.0,60.0),5.0);
    assert!(h.app.open);
    h.cursor=Some(Pos2::new(600.0,180.0));
    for _ in 0..120 {
        h.frame(vec![]);
        if !h.app.open {break;}
    }
    assert!(!h.app.open);
    assert!(h.app.progress()>0.1,"測試應停在收起動畫尚未完成時");
    assert!(!h.app.oled_shift_idle(&h.ctx),"收起動畫仍有 {:.1}% 展開程度時不能恢復位移",h.app.progress()*100.0);
    h.hover(Pos2::new(600.0,180.0),1.0);
    assert!(h.app.oled_shift_idle(&h.ctx),"動畫完成後應恢復位移");
}

#[test]
fn oled_shift_pauses_during_hover_intent() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.oled_shift=true;
    h.hover(Pos2::new(600.0,180.0),1.0);
    assert!(h.app.oled_shift_idle(&h.ctx));
    h.hover(Pos2::new(WIN_W/2.0,12.0),0.05);
    assert!(!h.app.open,"懸停意圖緩衝期間尚未展開");
    assert!(h.hit.contains(Pos2::new(WIN_W/2.0,12.0)),"滑鼠已進入島體互動區域");
    assert!(!h.app.oled_shift_idle(&h.ctx),"懸停互動期間不能移動島體");
}

#[test]
fn oled_shift_pauses_on_the_first_pointer_press_frame() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();let away=Pos2::new(600.0,180.0);
    h.hover(away,1.0);assert!(h.app.oled_shift_idle(&h.ctx));
    h.frame(vec![egui::Event::PointerButton{pos:away,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    assert!(!h.app.oled_shift_idle(&h.ctx),"按下的首幀必須暫停位移");
    h.frame(vec![]);assert!(!h.app.oled_shift_idle(&h.ctx),"按住期間必須暫停位移");
    h.frame(vec![egui::Event::PointerButton{pos:away,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    assert!(h.app.oled_shift_idle(&h.ctx),"鬆開且沒有互動後應恢復位移");
}

#[test]
fn oled_shift_waits_for_side_panels_to_finish_closing() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
        let mut h=H::new();h.app.settings.dock=dock;h.app.show_page(0);
        let inside=Some(Pos2::new(if dock==crate::model::Dock::Left {100.0}else{400.0},300.0));
        for _ in 0..300 {side_frame(&mut h,inside);}
        assert!(h.app.open);
        for _ in 0..120 {side_frame(&mut h,None);if !h.app.open {break;}}
        assert!(!h.app.open && h.app.progress()>0.1);
        assert!(!h.app.oled_shift_idle(&h.ctx),"{dock:?} 收起動畫期間不能移動");
        for _ in 0..120 {side_frame(&mut h,None);}
        assert!(h.app.oled_shift_idle(&h.ctx),"{dock:?} 動畫完成後應恢復位移");
    }
}

struct H {
    ctx: egui::Context,
    app: App,
    t: f64,
    cursor: Option<Pos2>,
    cmds:Vec<crate::app::Cmd>,
    region:Rect,
    hit:Rect,
    animating:bool,
    shapes:Vec<egui::epaint::ClippedShape>,
    bt: Vec<crate::bt::Device>,
    media: crate::smtc::NowPlaying,
    power: (Option<u8>, bool),
    network: Option<bool>,
    system_idle: Option<f64>,
    screen_locked: bool,
    fullscreen: bool,
    browser_at_top: Option<bool>,
    file_drop_zone:Option<Rect>,
    hovered_files:Vec<egui::HoveredFile>,
    dropped_files:Vec<egui::DroppedFile>,
}

impl H {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("daozai-test-{}-{}", std::process::id(), rand_id()));
        std::env::set_var("DAOZAI_DATA_DIR", &dir);
        let ctx = egui::Context::default();
        let app = App::new(&ctx, Options { fetch_lyrics: false, pet_idle: 1e9 });
        Self { ctx, app, t: 0.0, cursor: None,cmds:vec![],region:Rect::NOTHING,hit:Rect::NOTHING,animating:false,shapes:Vec::new(), bt:Vec::new(),media:Default::default(),power:(None,false),network:None,system_idle:None,screen_locked:false,fullscreen:false,browser_at_top:None,file_drop_zone:None,hovered_files:vec![],dropped_files:vec![] }
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.t += 1.0 / 60.0;
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(crate::app::CANVAS_W, WIN_H))),
            time: Some(self.t),
            events,
            hovered_files:self.hovered_files.clone(),
            dropped_files:std::mem::take(&mut self.dropped_files),
            focused: true,
            ..Default::default()
        };
        self.app.filter_todo_input(&mut raw);
        let env = Env {
            t: self.t,
            clock: "09:41:00".into(),
            today: "2026-10-28".into(),
            date: "10月28日 週三".into(),
            cursor: self.cursor,
            bt: self.bt.clone(),
            bt_ready: true,
            media:self.media.clone(),
            battery:self.power.0,
            charging:self.power.1,
            network:self.network,
            idle_seconds:self.system_idle,
            screen_locked:self.screen_locked,
            fullscreen_hidden:self.fullscreen && self.app.settings.hide_fullscreen,
            browser_at_top:self.browser_at_top,
            ..Default::default()
        };
        let cmds=&mut self.cmds;
        let region=&mut self.region;
        let hit=&mut self.hit;
        let animating=&mut self.animating;
        let app = &mut self.app;
        let file_drop_zone=&mut self.file_drop_zone;
        let output = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx, |ui| {
                let win = Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));
                let frame=app.frame(ui, win, &env);*region=frame.region;*hit=frame.hit;*animating=frame.animating;*file_drop_zone=frame.file_drop_zone;cmds.extend(frame.cmds);
            });
        });
        self.shapes=output.shapes;
    }
    fn hover(&mut self, p: Pos2, secs: f64) {
        self.cursor = Some(p);
        let n = (secs * 60.0) as usize;
        for _ in 0..n {
            self.frame(vec![egui::Event::PointerMoved(p)]);
        }
    }
    fn click(&mut self, p: Pos2) {
        self.cursor = Some(p);
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.frame(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
        self.frame(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
    }
    fn click_text(&mut self,label:&str){
        if !self.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==label))&&self.app.notice.is_some(){self.click_notice_badge();}
        let point=self.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label&&t.opacity_factor>0.9&&s.clip_rect.contains(t.visual_bounding_rect().center())=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("未找到可點選文字：{label}"));
        self.click(point);
    }
    fn click_notice_badge(&mut self){if self.app.compact_notices(){self.click(Pos2::new(WIN_W/2.0+(self.app.current_width()-44.0)/2.0+26.0,15.0));}}
    fn wheel(&mut self,delta:f32) {
        self.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:Vec2::new(0.0,delta),modifiers:Default::default()}]);
    }
    fn picker_frame(&mut self,events:Vec<egui::Event>){
        self.frame(events);
    }
    fn picker_click(&mut self,label:&str){
        for _ in 0..60 {self.picker_frame(vec![]);if (self.app.progress()-1.0).abs()<0.01 {break;}}
        self.picker_frame(vec![]);
        let find=|h:&H|h.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text) if text.galley.text()==label && shape.clip_rect.contains(text.visual_bounding_rect().center())=>Some(text.visual_bounding_rect().center()),_=>None,
        });
        let point=find(self).unwrap_or_else(||panic!("未找到可見選項：{label}"));
        assert!(self.hit.contains(point),"選項 {label} 超出島體命中區域");
        self.picker_frame(vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false]{self.picker_frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    }
    fn local(x: f32, y: f32) -> Pos2 {
        Pos2::new((WIN_W - PW) / 2.0 + x, y)
    }
    fn quota_accounts(&mut self,count:usize) {
        use crate::quota::{Account,Provider,Snapshot,Window};
        for i in 0..count {
            let id=format!("preview-{i}");
            self.app.quota.config.accounts.push(Account{id:id.clone(),provider:Provider::ALL[i%3],alias:format!("帳號 {}",i+1),source:format!("preview-{i}.json").into(),slot:None,enabled:true,pinned:true});
            self.app.quota_event(crate::quota::Event{id,result:Ok(Snapshot{windows:vec![Window{label:"本週".into(),seconds:Some(604800),remaining:100.0,reset:None,amount:None}],..Default::default()})});
        }
    }
    fn quota_centers(&self)->Vec<f32> {
        fn collect(shape:&egui::Shape,out:&mut Vec<f32>) {
            match shape {
                egui::Shape::Vec(shapes)=>for shape in shapes {collect(shape,out);},
                egui::Shape::Path(path) if path.points.len()>50=>{
                    let min=path.points.iter().map(|p|p.x).fold(f32::INFINITY,f32::min);
                    let max=path.points.iter().map(|p|p.x).fold(f32::NEG_INFINITY,f32::max);
                    if (20.0..28.0).contains(&(max-min)) {out.push((min+max)/2.0);}
                },
                egui::Shape::Circle(circle) if (10.0..14.0).contains(&circle.radius) && circle.stroke.width>0.0 && (circle.center.y-15.0).abs()<0.5=>out.push(circle.center.x),
                _=>{},
            }
        }
        let mut out=Vec::new();for shape in &self.shapes {collect(&shape.shape,&mut out);}
        out.sort_by(f32::total_cmp);out
    }
}

fn rand_id() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64
}

fn island_drag_frame(h:&mut H,bounds:crate::display_layout::Bounds,ppp:f32,pointer:Option<Pos2>,pressed:Option<bool>)->Rect {
    use crate::{display_layout,side_dock};
    let dock=h.app.settings.dock;
    let (width,height)=if dock.is_side(){(side_dock::SIDE_W,side_dock::SIDE_H)}else{(WIN_W,WIN_H)};
    let offset=h.app.placement_offset();
    let (x,y)=if dock.is_side(){display_layout::side_origin(bounds,dock,(width,height),side_dock::SIDE_HALF_EXTENT,ppp,offset)}
        else{display_layout::island_origin_offset(bounds,width,ppp,offset)}.unwrap();
    let screen=Rect::from_min_max(Pos2::new((bounds.left-x) as f32/ppp,(bounds.top-y) as f32/ppp),Pos2::new((bounds.right-x) as f32/ppp,(bounds.bottom-y) as f32/ppp));
    let cursor=pointer.map(|p|p+screen.min.to_vec2());
    let mut events=cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default();
    if let (Some(pos),Some(pressed))=(cursor,pressed){events.push(egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()});}
    h.t+=1.0/60.0;
    let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-10-28".into(),date:"10月28日 週三".into(),cursor,screen:Some(screen),bt_ready:true,..Default::default()};
    let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W.max(width),height))),time:Some(h.t),events,focused:true,
        viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
    let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
        let out=h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(width,height)),&env);h.region=out.region;h.hit=out.hit;h.cmds.extend(out.cmds);
    });});
    h.shapes=output.shapes;
    screen
}

fn island_drag_header(h:&mut H,bounds:crate::display_layout::Bounds,ppp:f32)->Pos2 {
    h.app.show_page(1);
    let mut screen=Rect::NOTHING;
    for _ in 0..90{screen=island_drag_frame(h,bounds,ppp,None,None);}
    h.shapes.iter().find_map(|shape|match &shape.shape{
        egui::Shape::Text(text) if text.galley.text()=="10月28日 週三"=>Some(text.visual_bounding_rect().center()-screen.min.to_vec2()),_=>None,
    }).expect("展開後的日期應可見")
}

#[test]
fn island_position_drag_tracks_screen_coordinates_and_saves_on_release(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:-2560,top:-400,right:0,bottom:1040};
    for dock in crate::model::Dock::ALL {for ppp in [1.0,1.5] {
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.island_offset=-40;h.app.settings.save();
        // 起點離居中足夠遠，90 點位移不會落進居中吸附範圍。
        let start=island_drag_header(&mut h,bounds,ppp);
        let before=island_drag_frame(&mut h,bounds,ppp,Some(start),Some(true));
        let delta=if dock.is_side(){Vec2::new(0.0,90.0)}else{Vec2::new(90.0,0.0)};
        let end=start+delta;
        island_drag_frame(&mut h,bounds,ppp,Some(end),None);
        assert!(h.app.settings.island_offset>-40,"{dock:?} / {ppp}：拖動應同步現有位置引數");
        assert_eq!(crate::model::Settings::load().island_offset,-40,"拖動途中不能反覆寫配置");
        let offset=h.app.settings.island_offset;
        for _ in 0..30{island_drag_frame(&mut h,bounds,ppp,Some(end),None);}
        let after=island_drag_frame(&mut h,bounds,ppp,Some(end),None);
        let movement=before.min-after.min;
        assert!(((if dock.is_side(){movement.y}else{movement.x})-90.0).abs()<6.0,"{dock:?} / {ppp}：視窗應跟隨實際滑鼠位移，允許整數位置刻度帶來的取整誤差");
        assert_eq!(if dock.is_side(){movement.x}else{movement.y},0.0,"拖動不能脫離停靠邊緣");
        assert_eq!(h.app.settings.island_offset,offset,"視窗跟隨滑鼠移動後，原地持有不能發生累積漂移");
        island_drag_frame(&mut h,bounds,ppp,Some(end),Some(false));
        assert_eq!(crate::model::Settings::load().island_offset,offset,"鬆手後應儲存位置");
        assert_eq!((h.app.settings.dock,h.app.settings.display_id.as_str()),(dock,""));
    }}
}

#[test]
fn island_position_drag_clamps_to_existing_limits_and_stays_resident(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    for dock in crate::model::Dock::ALL {for shape in crate::model::IslandShape::ALL {
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.island_shape=shape;
        let start=island_drag_header(&mut h,bounds,1.0);
        island_drag_frame(&mut h,bounds,1.0,Some(start),Some(true));
        let end=if dock.is_side(){Pos2::new(start.x,3000.0)}else{Pos2::new(1855.0,start.y)};
        for _ in 0..120{island_drag_frame(&mut h,bounds,1.0,Some(end),None);}
        assert!(!h.app.open,"{dock:?} 拖動期間應保持常駐形態");
        assert!(h.app.progress()<0.01,"拖動期間不應保留展開面板");
        assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="10月28日 週三")),"拖動期間不應顯示展開頂欄");
        assert_eq!(h.app.settings.island_offset,crate::model::ISLAND_OFFSET_MAX);
        let end=if dock.is_side(){Pos2::new(start.x,65.0)}else{Pos2::new(65.0,start.y)};
        island_drag_frame(&mut h,bounds,1.0,Some(end),None);
        assert_eq!(h.app.settings.island_offset,-crate::model::ISLAND_OFFSET_MAX);
        assert_eq!(h.app.settings.dock,dock);
        island_drag_frame(&mut h,bounds,1.0,Some(end),Some(false));
        let offset=h.app.settings.island_offset;
        for _ in 0..40{island_drag_frame(&mut h,bounds,1.0,Some(start),None);}
        assert_eq!(h.app.settings.island_offset,offset,"鬆手後不能繼續跟著滑鼠移動");
    }}
}

#[test]
fn island_position_drag_below_a_top_bar_switches_edges_like_the_bare_screen(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::model::{Dock,Settings};
    let monitor=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    let display=crate::display_layout::Display{id:"d".into(),label:"d".into(),bounds:monitor,work_top:28,primary:true};
    let bounds=crate::display_layout::placement_bounds(&display,true);
    for to in [Dock::Left,Dock::Right] {
        let mut h=H::new();h.app.settings.save();
        let start=island_drag_header(&mut h,bounds,1.0);
        island_drag_frame(&mut h,bounds,1.0,Some(start),Some(true));
        let size=Vec2::new(1920.0,1052.0);
        let end=if to==Dock::Left{Pos2::new(0.0,size.y*0.6)}else{Pos2::new(size.x,size.y*0.6)};
        for _ in 0..30{island_drag_frame(&mut h,bounds,1.0,Some(end),None);}
        assert_eq!(h.app.settings.dock,to);
        let back=Pos2::new(size.x*0.5,0.0);
        for _ in 0..30{island_drag_frame(&mut h,bounds,1.0,Some(back),None);}
        assert_eq!(h.app.settings.dock,Dock::Top,"拖回頂欄下沿應能回到頂部");
        island_drag_frame(&mut h,bounds,1.0,Some(back),Some(false));
        assert_eq!(Settings::load().dock,Dock::Top);
        assert!(Settings::load().avoid_top_bar,"拖動不改變避開頂欄設定");
    }
}

#[test]
fn island_position_drag_switches_between_all_three_edges_and_saves_on_release(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::model::{Dock,IslandShape,Settings};
    let bounds=crate::display_layout::Bounds{left:-2560,top:-400,right:0,bottom:1040};
    for from in Dock::ALL {for to in Dock::ALL.into_iter().filter(|to|*to!=from) {for ppp in [1.0,1.5] {for shape in IslandShape::ALL {
        let mut h=H::new();h.app.settings.dock=from;h.app.settings.island_shape=shape;h.app.settings.save();
        let start=island_drag_header(&mut h,bounds,ppp);
        island_drag_frame(&mut h,bounds,ppp,Some(start),Some(true));
        let size=Vec2::new(2560.0,1440.0)/ppp;
        let end=match to {Dock::Top=>Pos2::new(size.x*0.6,0.0),Dock::Left=>Pos2::new(0.0,size.y*0.6),Dock::Right=>Pos2::new(size.x,size.y*0.6)};
        for _ in 0..30{island_drag_frame(&mut h,bounds,ppp,Some(end),None);}
        assert_eq!(h.app.settings.dock,to,"{from:?} → {to:?} / {ppp} / {shape:?}");
        assert!(!h.app.open && h.app.progress()<0.01,"切換停靠邊後仍應顯示常駐形態");
        assert_eq!(Settings::load().dock,from,"拖動中不能寫入停靠設定");
        let offset=h.app.settings.island_offset;
        for _ in 0..15{island_drag_frame(&mut h,bounds,ppp,Some(end),None);}
        assert_eq!(h.app.settings.island_offset,offset,"換邊後持有不能漂移");
        island_drag_frame(&mut h,bounds,ppp,Some(end),Some(false));
        let saved=Settings::load();assert_eq!((saved.dock,saved.island_offset),(to,offset));
        for _ in 0..30{island_drag_frame(&mut h,bounds,ppp,Some(end),None);}
        assert!(!h.app.open,"鬆手後應留在常駐形態");
        let screen=island_drag_frame(&mut h,bounds,ppp,Some(end),None);
        let resident=h.hit.center()-screen.min.to_vec2();
        for _ in 0..30{island_drag_frame(&mut h,bounds,ppp,Some(resident),None);}
        assert!(!h.app.open,"鬆手後遊標留在島上不能立即展開");
        for _ in 0..30{island_drag_frame(&mut h,bounds,ppp,None,None);}
        for _ in 0..60{island_drag_frame(&mut h,bounds,ppp,Some(resident),None);}
        assert!(h.app.open,"遊標離開再懸停，應能正常展開");
        assert_eq!(h.app.settings.display_id,"");
    }}}}
}

#[test]
fn island_position_drag_keeps_the_edge_in_the_middle_and_does_not_flap_at_corners(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::model::Dock;
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    let mut h=H::new();let start=island_drag_header(&mut h,bounds,1.0);
    island_drag_frame(&mut h,bounds,1.0,Some(start),Some(true));
    for (point,dock) in [(Pos2::new(500.0,500.0),Dock::Top),(Pos2::new(0.0,500.0),Dock::Left),
        (Pos2::new(960.0,540.0),Dock::Left),(Pos2::new(1250.0,540.0),Dock::Right),(Pos2::new(1000.0,540.0),Dock::Right),
        (Pos2::new(700.0,540.0),Dock::Left),(Pos2::new(4.0,3.0),Dock::Left),
        (Pos2::new(3.0,4.0),Dock::Left),(Pos2::new(100.0,0.0),Dock::Top),
        (Pos2::new(1920.0,500.0),Dock::Right),(Pos2::new(1918.0,3.0),Dock::Right),
        (Pos2::new(1500.0,0.0),Dock::Top)] {
        for _ in 0..5{island_drag_frame(&mut h,bounds,1.0,Some(point),None);}
        assert_eq!(h.app.settings.dock,dock,"遊標={point:?}");
        assert!(!h.app.open);
    }
    island_drag_frame(&mut h,bounds,1.0,None,Some(false));
    island_drag_frame(&mut h,bounds,1.0,Some(start),Some(false));
    let dock=h.app.settings.dock;let offset=h.app.settings.island_offset;
    island_drag_frame(&mut h,bounds,1.0,Some(Pos2::new(0.0,500.0)),None);
    assert_eq!((h.app.settings.dock,h.app.settings.island_offset),(dock,offset),"鬆手後不能繼續拖動");
}

#[test]
fn island_position_drag_lifts_on_hold_glides_smoothly_and_snaps_to_center(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    let mut h=H::new();h.app.settings.island_offset=-30;h.app.settings.save();
    let start=island_drag_header(&mut h,bounds,1.0);
    island_drag_frame(&mut h,bounds,1.0,Some(start),Some(true));
    // 原地按住片刻即拿起：面板開始收起，位置不變。
    for _ in 0..20{island_drag_frame(&mut h,bounds,1.0,Some(start),None);}
    assert!(!h.app.open && h.app.progress()<0.5,"按住不動也應拿起島體");
    assert_eq!(h.app.settings.island_offset,-30);
    // 視窗平滑跟隨：一幀跳過去的遊標，視窗要分幾幀追上，且不按整數刻度跳。
    let travel=1920.0/2.0-crate::display_layout::ISLAND_HALF_EXTENT;
    let to=|units:f32|Pos2::new(start.x+(units+30.0)/100.0*travel,start.y);
    island_drag_frame(&mut h,bounds,1.0,Some(to(-10.0)),None);
    island_drag_frame(&mut h,bounds,1.0,Some(to(-10.0)),None);
    let mid=h.app.placement_offset();
    assert!(mid> -0.30 && mid< -0.10,"視窗應在途中，而不是瞬間到位：{mid}");
    for _ in 0..40{island_drag_frame(&mut h,bounds,1.0,Some(to(-10.0)),None);}
    assert!((h.app.placement_offset()+0.10).abs()<0.003,"持有後應追上游標");
    // 居中附近吸附，離開吸附範圍後恢復跟隨。
    assert!(h.app.center_guide>0.2 && h.app.center_guide<0.9 && h.app.center_lock_a<0.05,"接近居中時準線漸顯，但還沒點亮：{}",h.app.center_guide);
    island_drag_frame(&mut h,bounds,1.0,Some(to(2.0)),None);
    assert_eq!(h.app.settings.island_offset,0,"靠近居中應吸附");
    assert!(h.app.center_locked,"吸附後應標記為已居中");
    for _ in 0..30{island_drag_frame(&mut h,bounds,1.0,Some(to(2.0)),None);}
    assert!(h.app.center_guide>0.9 && h.app.center_lock_a>0.9,"居中時準線應清晰可見並切換為已居中配色");
    // 吸住後有滯回：稍微拖出一點仍停在中間，拖得更遠才鬆開。
    island_drag_frame(&mut h,bounds,1.0,Some(to(7.0)),None);
    assert_eq!(h.app.settings.island_offset,0,"居中後輕微偏移不應鬆開");
    island_drag_frame(&mut h,bounds,1.0,Some(to(12.0)),None);
    assert_eq!(h.app.settings.island_offset,12);
    assert!(!h.app.center_locked);
    island_drag_frame(&mut h,bounds,1.0,Some(to(-28.5)),None);
    assert_eq!(h.app.settings.island_offset,-30,"靠近原位置應吸附回去");
    island_drag_frame(&mut h,bounds,1.0,Some(to(2.0)),None);
    island_drag_frame(&mut h,bounds,1.0,Some(to(2.0)),Some(false));
    assert_eq!(crate::model::Settings::load().island_offset,0,"鬆手儲存吸附後的位置");
    for _ in 0..90{island_drag_frame(&mut h,bounds,1.0,Some(to(2.0)),None);}
    assert_eq!(h.app.placement_offset(),0.0,"落位後回到儲存的位置");
}

#[test]
fn island_position_drag_ignores_jitter_and_preserves_header_buttons(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    let mut h=H::new();let start=island_drag_header(&mut h,bounds,1.0);
    island_drag_frame(&mut h,bounds,1.0,Some(start),Some(true));
    island_drag_frame(&mut h,bounds,1.0,Some(start+Vec2::splat(1.0)),None);
    island_drag_frame(&mut h,bounds,1.0,Some(start+Vec2::splat(1.0)),Some(false));
    assert_eq!(h.app.settings.island_offset,0,"輕微手抖不能改變位置");
    let screen=island_drag_frame(&mut h,bounds,1.0,Some(start),None);
    let gear=H::local(394.0,18.0)-screen.min.to_vec2();
    island_drag_frame(&mut h,bounds,1.0,Some(gear),Some(true));
    island_drag_frame(&mut h,bounds,1.0,Some(gear),Some(false));
    assert!(h.app.settings_open,"時間區域拖動不能攔截設定按鈕");
    assert_eq!(h.app.settings.island_offset,0);
    h.app.settings_open=false;
    for (page,x,expected) in [(3,330.0,crate::app::Cmd::AddFiles),(6,319.0,crate::app::Cmd::QuotaRefresh)] {
        h.app.show_page(page);
        for _ in 0..60{island_drag_frame(&mut h,bounds,1.0,Some(start),None);}
        let screen=island_drag_frame(&mut h,bounds,1.0,Some(start),None);
        let point=H::local(x,18.0)-screen.min.to_vec2();
        island_drag_frame(&mut h,bounds,1.0,Some(point),Some(true));
        island_drag_frame(&mut h,bounds,1.0,Some(point),Some(false));
        assert!(h.cmds.contains(&expected),"頂部操作按鈕應正常響應");
        assert_eq!(h.app.settings.island_offset,0);
    }
}
#[test]
fn shelf_drop_zone_leaves_normal_hover_and_text_selection_unchanged(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for _ in 0..90{h.frame(vec![]);}let zone=h.file_drop_zone.unwrap();
    assert_eq!(zone.width(),PW);assert_eq!(zone.height(),96.0);
    let point=zone.right_bottom()-Vec2::splat(12.0);assert!(!h.hit.contains(point));
    h.hover(point,0.8);assert!(!h.app.open);
    h.frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    h.hover(point,0.8);assert!(!h.app.open,"按住滑鼠拖選文字不應開啟置物架");
    h.frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    h.app.settings.visible_pages[3]=false;h.frame(vec![]);assert!(h.file_drop_zone.is_none());
}

#[test]
fn shelf_file_drag_opens_below_the_island_and_cancellation_restores_the_page(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.go(1,1.0);for _ in 0..90{h.frame(vec![]);}
    let point=h.file_drop_zone.unwrap().right_bottom()-Vec2::splat(12.0);
    assert!(!h.hit.contains(point));h.cursor=Some(point);
    h.hovered_files.push(egui::HoveredFile{path:Some(crate::model::config_dir().join("拖入檔案.txt")),..Default::default()});
    h.frame(vec![]);assert!(h.app.open && h.app.page==3 && h.app.drag_hot,"檔案進入感應區應立即展開置物架");
    for _ in 0..60{h.frame(vec![]);}assert!(h.app.open);
    h.hovered_files.clear();h.cursor=None;for _ in 0..60{h.frame(vec![]);}
    assert!(!h.app.drag_hot && !h.app.open);assert_eq!(h.app.page,1);
    assert!(h.app.shelf.items.is_empty(),"取消拖入不應儲存檔案");
}

#[test]
fn shelf_quick_drop_in_the_capture_zone_shows_the_added_file(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    let path=crate::model::config_dir().join("快速拖入.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,"test").unwrap();
    h.cursor=Some(h.file_drop_zone.unwrap().right_bottom()-Vec2::splat(12.0));
    h.dropped_files.push(egui::DroppedFile{path:Some(path.clone()),..Default::default()});
    for _ in 0..90{h.frame(vec![]);}
    assert!(h.app.open && h.app.page==3);assert_eq!(h.app.shelf.items,vec![path.clone()]);assert!(path.exists());
    let mut h=H::new();for _ in 0..90{h.frame(vec![]);}
    h.cursor=Some(h.file_drop_zone.unwrap().right_bottom()-Vec2::splat(12.0));h.app.settings.visible_pages[3]=false;
    h.dropped_files.push(egui::DroppedFile{path:Some(path),..Default::default()});h.frame(vec![]);
    assert!(!h.app.open && !h.app.settings_open,"關閉置物架後應忽略拖入");
}

#[test]
fn settings_opening_restores_focus_once_without_becoming_topmost() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.settings_theme=crate::settings_theme::SettingsTheme::Dark;
    h.ctx.set_embed_viewports(false);
    let frame=|h:&mut H,focused:bool| {
        egui::Context::set_immediate_viewport_renderer(move |ctx,mut viewport| {
            let id=viewport.ids.this;
            let mut raw=egui::RawInput{viewport_id:id,focused,system_theme:Some(egui::Theme::Light),..Default::default()};
            raw.viewports.insert(id,egui::ViewportInfo{focused:Some(focused),minimized:Some(!focused),..Default::default()});
            let _=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));
        });
        let id=egui::ViewportId::from_hash_of("daozai-settings");
        let mut raw=egui::RawInput::default();
        raw.viewports.insert(id,egui::ViewportInfo{focused:Some(focused),..Default::default()});
        let mut output=h.ctx.run(raw,|ctx|h.app.settings_ui(ctx,false));
        let viewport=output.viewport_output.remove(&id).unwrap();
        assert!(viewport.commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark))),"淺色系統下視窗標題欄和邊框也應使用深色");
        (viewport.builder,viewport.commands)
    };
    h.app.open_mail_settings();
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.visible,Some(false),"設定視窗應隱藏建立，避免首幀繪製前露出白色背景");
    assert_eq!(builder.active,Some(false),"隱藏視窗不能提前啟用");
    let show=commands.iter().position(|cmd|matches!(cmd,egui::ViewportCommand::Visible(true))).expect("首幀繪製後應顯示設定視窗");
    let focus=commands.iter().position(|cmd|matches!(cmd,egui::ViewportCommand::Focus)).unwrap();
    let theme=commands.iter().position(|cmd|matches!(cmd,egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark))).unwrap();
    assert!(theme<show,"顯示視窗前應設定深色邊框，避免白邊閃現");
    assert!(show<focus,"應先顯示已繪製的視窗，再獲取焦點");
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"開啟設定時應保持普通視窗層級");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Minimized(false))),"應恢復已最小化的設定視窗：{commands:?}");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)),"應將焦點交給設定視窗");
    let (builder,commands)=frame(&mut h,true);
    assert!(!commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Visible(_))),"正常重繪不應重複顯示視窗，避免打斷最小化");
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"操作設定時也不能切換到置頂層");
    assert!(!commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)),"正常繪製不應反覆搶焦點");
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"切回其他程式後應解除設定置頂");
    assert!(!commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)));
    h.app.open_mail_settings();
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"再次喚起設定仍須保持普通視窗層級");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Minimized(false))));
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)),"設定已開啟時仍應響應再次喚起");
    h.app.settings_open=false;
    let _=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));
    h.app.open_settings();
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.visible,Some(false),"關閉後重新建立也應隱藏到首幀完成");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Visible(true))),"重開設定應正常顯示");
}

#[test]
fn web_video_is_hidden_without_changing_music_preferences(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.opts.fetch_lyrics=true;h.app.show_lyrics=true;
    h.app.ly.key="Previous Song|Artist|120".into();
    h.app.ly.lyrics=Some(crate::lyrics::parse_lrc("[00:01.00]previous song"));
    h.app.ly.error=Some(crate::lyrics::FetchError::Unavailable("503".into()));
    let (tx,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    h.media=crate::smtc::NowPlaying{title:"小唐的Token的抖音 - 抖音".into(),playing:true,duration:17.0,..Default::default()};
    h.frame(vec![]);
    assert!(h.app.media.title.is_empty() && !h.app.media.playing,"影片標題和播放狀態不應進入島內音樂頁面");
    assert!(h.app.ly.key.is_empty(),"影片不應建立歌曲取詞任務");
    assert!(h.app.ly.lyrics.is_none() && h.app.ly.error.is_none(),"切到影片應清除上一首歌詞和歌詞錯誤");
    assert!(tx.send(Ok(crate::lyrics::parse_lrc("[00:01.00]late song"))).is_err(),"影片不能接收舊歌曲遲到的查詢結果");
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.click(H::local(74.0,118.0));
    assert!(h.app.show_lyrics,"影片頁面的標籤不應切換使用者的歌曲歌詞偏好");
    h.click(H::local(210.5,118.0));
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Media(_))),"音樂頁不應控制被隱藏的影片");
    assert!(!matches!(h.app.pet_state(),crate::extras::PetState::Music),"影片不應觸發聽歌表情");
    h.app.opts.fetch_lyrics=false;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),0.5);
    h.click(H::local(74.0,118.0));
    assert!(!h.app.show_lyrics,"回到歌曲時應恢復原來的詞按鈕");
}

#[test]
fn web_video_does_not_override_the_minimal_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;
    h.media=crate::smtc::NowPlaying{title:"小唐的Token的抖音 - 抖音".into(),playing:true,duration:17.0,..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"影片不能觸發歌曲專用的收起態");
}

#[test]
fn minimal_mode_stays_a_line_with_music_and_after_media_changes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"播放音樂時仍應遵守極簡開關");
    assert!(h.hit.height()<=crate::app::BAR_H_MIN+0.1,"滑鼠命中範圍也應收回細線");
    h.media.title="Another Track".into();h.media.playing=false;
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"暫停或切歌不能覆蓋極簡開關");
    h.media=Default::default();
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"沒有媒體時恢復使用者選擇的極簡狀態");
}

#[test]
fn toggling_minimal_changes_the_island_while_media_is_present(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}assert!(h.region.height()>30.0);
    h.app.settings.minimal=true;for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"開啟極簡後應收成細線");
    h.app.settings.minimal=false;for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()>30.0,"關閉極簡後應恢復普通島體");
}

#[test]
fn minimal_mode_keeps_focus_running_and_returns_after_notifications(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.minimal=true;h.app.pomo.running=true;h.app.pomo.remaining=120.0;
    for _ in 0..90 {h.frame(vec![]);}assert!(h.region.height()<20.0,"專注不能覆蓋極簡開關");
    h.app.alert("系統音量","音量 50%",5,false);
    for _ in 0..60 {h.frame(vec![]);}assert!(h.region.height()>30.0,"臨時提醒仍應可見");
    for _ in 0..240 {h.frame(vec![]);}assert!(h.region.height()<20.0,"提醒結束後應回到細線");
    assert!(h.app.pomo.running&&h.app.pomo.remaining<115.0,"極簡僅改變顯示，計時繼續執行");
}

#[test]
fn delayed_media_samples_keep_the_live_position_and_follow_pause_and_seek(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.2,duration:120.0,playing:true,sampled_at:Some(std::time::Instant::now()-std::time::Duration::from_millis(600)),..Default::default()};
    h.frame(vec![]);
    assert!(h.app.media.position>=9.79,"媒體位置必須包含送到介面之前經過的時間");
    h.media.playing=false;h.media.position=9.4;h.frame(vec![]);
    assert_eq!(h.app.media.position,9.4,"暫停不能繼續外推");
    h.media.playing=true;h.media.position=4.0;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
    assert!((4.0..4.2).contains(&h.app.media.position),"跳轉後應使用新的播放位置");
    h.media.position=119.9;h.media.sampled_at=Some(std::time::Instant::now()-std::time::Duration::from_secs(1));h.frame(vec![]);
    assert_eq!(h.app.media.position,120.0,"外推不能超過歌曲結尾");
}

#[test]
fn kugou_external_seek_updates_lyrics_in_both_directions(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{source:"KuGou.exe".into(),title:"Track".into(),artist:"Artist".into(),duration:120.0,playing:true,..Default::default()};
    h.frame(vec![]);
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]開頭\n[00:30]中段\n[01:00]後段"));
    for (position,line) in [(65.0,"後段"),(10.0,"開頭"),(35.0,"中段")] {
        h.media.position=position;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
        assert_eq!(h.app.island_lyric().map(|(_,text)|text),Some(line),"酷狗跳轉後應立即切換到對應歌詞");
    }
}

#[test]
fn music_clients_keep_lyrics_across_two_repeat_cycles(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for source in ["kugou","QQMusic.exe","cloudmusic.exe","汽水音樂","Spotify.exe","kwmusic.exe"] {
        let mut h=H::new();h.app.settings.island_lyrics=true;
        h.media=crate::smtc::NowPlaying{source:source.into(),title:"迴圈測試".into(),artist:"歌手".into(),duration:120.0,playing:true,..Default::default()};
        h.frame(vec![]);h.app.set_lyrics("迴圈測試","歌手",120.0,crate::lyrics::parse_lrc("[00:00]開頭\n[00:05]第一段\n[01:50]尾聲"));
        for (position,line) in [(119.0,"尾聲"),(0.0,"開頭"),(8.0,"第一段"),(119.0,"尾聲"),(0.0,"開頭"),(8.0,"第一段")] {
            h.media.position=position;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
            assert_eq!(h.app.island_lyric().map(|(_,text)|text),Some(line),"{source} 第二、三遍也必須顯示相應歌詞");
        }
    }
}

#[test]
fn music_without_a_timeline_does_not_invent_elapsed_time(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"歌曲".into(),playing:true,playback_type:crate::smtc::MediaKind::Music,..Default::default()};
    for _ in 0..240{h.frame(vec![]);}
    assert_eq!(h.app.media.title,"歌曲");assert_eq!(h.app.media.position,0.0);assert_eq!(h.app.media.duration,0.0);
}

#[test]
fn kugou_position_advances_without_duration_and_respects_pause_and_seek(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"kugou".into(),title:"Track".into(),artist:"Artist".into(),position:42.0,duration:0.0,playing:true,sampled_at:Some(std::time::Instant::now()-std::time::Duration::from_millis(600)),..Default::default()};
    h.frame(vec![]);
    assert!(h.app.media.position>=42.59,"未知總時長不能阻止真實進度外推");
    h.media.playing=false;h.frame(vec![]);assert_eq!(h.app.media.position,42.0);
    h.media.playing=true;h.media.position=12.0;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
    assert!((12.0..12.2).contains(&h.app.media.position),"拖動後必須跟隨新的實際位置");
}

#[test]
fn music_volume_popup_opens_and_mute_restores_the_selected_players_level(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),duration:120.0,volume:Some((60,false)),..Default::default()};
    h.app.volume_ui=Some((45,false));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.click(H::local(367.0,118.0));
    assert!(h.app.music_volume_open);
    assert!(h.cmds.is_empty(),"開啟浮條不應改變音量或靜音");
    h.hover(H::local(367.0,92.0),0.2);h.click(H::local(367.0,92.0));
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetMute(true),"QQMusic.exe".into())));
    assert!(!h.cmds.contains(&crate::app::Cmd::ToggleMute));
    assert_eq!(h.app.music_volume_ui,Some((60,true)));
    assert_eq!(h.app.volume_ui,Some((45,false)));
    h.click(H::local(367.0,92.0));
    assert_eq!(h.app.music_volume_ui,Some((60,false)),"恢復時沿用靜音前的音量");
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetMute(false),"QQMusic.exe".into())));
}

#[test]
fn music_volume_popup_slider_unmutes_and_scrolls_without_changing_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),volume:Some((60,true)),..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.click(H::local(367.0,118.0));h.hover(H::local(367.0,53.5),0.2);
    h.click(H::local(367.0,53.5));
    assert_eq!(h.app.music_volume_ui,Some((50,false)));
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetVolume(50),"QQMusic.exe".into())));
    h.wheel(120.0);assert_eq!(h.app.music_volume_ui,Some((54,false)));assert_eq!(h.app.page,0);
    h.click(H::local(367.0,32.0));assert_eq!(h.app.music_volume_ui,Some((100,false)));
    h.click(H::local(367.0,75.0));assert_eq!(h.app.music_volume_ui,Some((0,false)));
    h.click(H::local(387.0,68.0));assert_eq!(h.app.page,0,"浮條遮擋部分不能穿透觸發翻頁");
    h.click(H::local(387.0,18.0));assert!(!h.app.settings_open,"浮條遮擋部分不能穿透開啟設定");
}

#[test]
fn music_volume_popup_closes_on_escape_outside_click_source_change_and_collapse(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),volume:Some((60,false)),..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.click(H::local(367.0,118.0));
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert!(!h.app.music_volume_open);assert!(h.app.open,"Esc 只關閉浮條，保留音樂面板");
    h.click(H::local(367.0,118.0));h.click(H::local(180.0,45.0));assert!(!h.app.music_volume_open);
    h.click(H::local(367.0,118.0));h.media.source="cloudmusic.exe".into();h.frame(vec![]);assert!(!h.app.music_volume_open);
    h.click(H::local(367.0,118.0));h.app.show_page(1);assert!(!h.app.music_volume_open);
    h.app.show_page(0);h.hover(H::local(180.0,45.0),1.0);h.click(H::local(367.0,118.0));
    h.hover(Pos2::new(0.0,250.0),5.0);assert!(!h.app.music_volume_open);
}

#[test]
#[ignore="按需匯出音樂音量浮條實際畫面：DAOZAI_MUSIC_VOLUME_PREVIEW=目錄"]
fn music_volume_popup_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_MUSIC_VOLUME_PREVIEW").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for (name,muted,lyrics) in [("volume",false,false),("muted",true,false),("lyrics",false,true)] {
        let mut h=H::new();h.ctx.set_pixels_per_point(2.0);h.app.settings.pet=false;h.app.show_lyrics=lyrics;
        h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"晴天".into(),artist:"周杰倫".into(),position:92.0,duration:269.0,volume:Some((62,muted)),..Default::default()};
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
        if lyrics{h.app.set_lyrics("晴天","周杰倫",269.0,crate::lyrics::parse_lrc("[00:00]午後的陽光落在窗邊\n[01:30]風把回憶吹向遠方\n[01:40]慢慢走過這段時光"));}
        h.click(H::local(367.0,118.0));h.hover(H::local(330.0,112.0),0.25);
        let ppp=h.ctx.pixels_per_point();let mut raster=crate::preview_raster::Raster::new((crate::app::CANVAS_W*ppp) as usize,(WIN_H*ppp) as usize);
        raster.update_textures(&egui::TexturesDelta{set:vec![(egui::TextureId::default(),egui::epaint::ImageDelta::full(h.ctx.fonts(|f|f.image()),egui::TextureOptions::LINEAR))],..Default::default()});
        raster.buf.fill([0.10,0.12,0.16,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),ppp),ppp);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
        let image=image::RgbaImage::from_raw(raster.w as u32,raster.h as u32,rgba).unwrap();
        image::imageops::crop_imm(&image,((WIN_W-crate::app::PW)/2.0*ppp-12.0) as u32,0,((crate::app::PW+12.0)*ppp+12.0) as u32,((crate::app::PH+12.0)*ppp) as u32).to_image().save(dest.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn music_volume_wheel_and_player_switch_keep_system_volume_untouched(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),volume:Some((60,true)),..Default::default()};
    h.app.volume_ui=Some((45,false));h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.hover(H::local(367.0,118.0),0.2);h.wheel(120.0);
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetVolume(64),"QQMusic.exe".into())),"音量={:?} 命令={:?} 頁={} 指標={:?}",h.app.music_volume_ui,h.cmds,h.app.page,h.cursor);
    assert_eq!(h.app.music_volume_ui,Some((64,false)));
    h.media.source="cloudmusic.exe".into();h.media.volume=Some((22,false));h.frame(vec![]);
    assert_eq!(h.app.music_volume_ui,Some((22,false)),"換播放器立即更新獨立音量");
    assert_eq!(h.app.volume_ui,Some((45,false)));
    h.media.volume=None;h.cmds.clear();h.click(H::local(367.0,118.0));h.wheel(120.0);
    assert!(h.cmds.is_empty(),"沒有應用會話時不可回退到系統音量");
}

#[test]
fn music_uses_system_artwork_and_clears_it_when_the_next_song_has_none(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    let image=std::sync::Arc::new(egui::ColorImage::new([32,32],egui::Color32::BLUE));
    h.media=crate::smtc::NowPlaying{title:"First".into(),artist:"Artist".into(),artwork:Some(image.clone()),..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    let (cached,texture)=h.app.music_artwork.as_ref().expect("播放頁應使用系統封面");
    assert!(std::sync::Arc::ptr_eq(cached,&image));
    let texture_id=texture.id();
    h.frame(vec![]);
    assert_eq!(h.app.music_artwork.as_ref().unwrap().1.id(),texture_id,"進度更新不應重複上傳紋理");
    h.media=crate::smtc::NowPlaying{title:"Second".into(),artist:"Artist".into(),..Default::default()};
    h.frame(vec![]);
    assert!(h.app.music_artwork.is_none(),"缺少封面的下一首歌應恢復佔點陣圖");
}

#[test]
fn failed_lyrics_can_be_retried_from_the_music_page(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
    h.app.ly.key="Track|Artist|120".into();
    h.app.ly.error=Some(crate::lyrics::FetchError::Unavailable("HTTP 502".into()));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="暫未獲取歌詞，請重試")),"展開內應保留歌詞錯誤提示");
    h.click(H::local(310.0,68.0));
    assert!(h.app.ly.key.is_empty(),"點選錯誤提示後應允許重新請求當前歌曲");
}

#[test]
fn transient_lyric_failures_retry_but_missing_songs_do_not_loop(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (error,retry) in [(crate::lyrics::FetchError::Unavailable("503".into()),true),(crate::lyrics::FetchError::NotFound,false),(crate::lyrics::FetchError::IncompleteMetadata,false)] {
        let mut h=H::new();
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
        h.app.opts.fetch_lyrics=true;h.app.show_lyrics=true;
        h.app.ly.key="Track|Artist|120".into();
        let (tx,rx)=std::sync::mpsc::channel();
        h.app.ly.rx=Some(rx);h.app.ly.loading=true;
        tx.send(Err(error.clone())).unwrap();
        h.frame(vec![]);
        assert_eq!(h.app.ly.error,Some(error));
        assert_eq!(h.app.ly.retry_at.is_some(),retry);
        assert!(!h.app.ly.loading);
        if let Some(at)=h.app.ly.retry_at {assert!((at-h.t-30.0).abs()<0.001,"網路恢復重試應有間隔");}
    }
}

#[test]
fn music_lyrics_show_in_a_separate_right_column() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:01]上一句\n[00:05]當前歌詞\n[00:10]下一句"));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    let current=h.shapes.iter().find_map(|shape|match &shape.shape {
        egui::Shape::Text(text) if text.galley.text()=="當前歌詞"=>Some(text.pos),_=>None,
    }).expect("展開頁應顯示同步歌詞");
    assert!(current.x>=H::local(243.0,0.0).x,"歌詞必須位於播放器右側");
    let title=h.shapes.iter().find_map(|shape|match &shape.shape {
        egui::Shape::Text(text) if text.galley.text()=="Track"=>Some(text.pos),_=>None,
    }).unwrap();
    assert!(title.x<H::local(233.0,0.0).x);
    h.click(H::local(74.0,118.0));
    assert!(!h.app.show_lyrics);
    h.frame(vec![]);
    assert!(!h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="當前歌詞")));
}

#[test]
fn disabling_lyrics_disconnects_pending_workers(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"KuGou.exe".into(),title:"測試歌曲".into(),artist:"歌手".into(),..Default::default()};
    h.frame(vec![]);h.app.opts.fetch_lyrics=true;
    h.app.show_lyrics=false;h.app.settings.island_lyrics=false;
    h.app.ly.key="測試歌曲|歌手|0".into();h.app.ly.loading=true;
    let (network,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    h.frame(vec![]);
    assert!(network.send(Err(crate::lyrics::FetchError::NotFound)).is_err());
    assert!(h.app.ly.key.is_empty(),"再次開啟時應能重新載入歌詞");
}




#[test]
fn island_lyrics_pause_returns_to_resident_and_resume_follows_current_position(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let visible=|h:&H,label:&str|h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()==label));
    for minimal in [false,true] {for synced in [true,false] {
        let mut h=H::new();h.app.settings.minimal=minimal;
        h.power=(Some(75),false);h.app.pomo.running=true;h.app.pomo.remaining=600.0;
        h.app.weather.snapshot=Some(crate::weather::Snapshot{temp:21.0,..Default::default()});
        h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
        for _ in 0..90 {h.frame(vec![]);}
        let resident_width=h.app.current_width();let resident_height=h.hit.height();
        h.app.settings.island_lyrics=true;h.media.playing=true;
        let lyrics=if synced {crate::lyrics::parse_lrc("[00:05]當前歌詞\n[00:10]下一句歌詞")}else{crate::lyrics::from_plain("沒有時間戳的歌詞")};
        h.app.set_lyrics("Track","Artist",120.0,lyrics);
        for _ in 0..90 {h.frame(vec![]);}
        assert!(visible(&h,if synced{"當前歌詞"}else{"Track"}));
        assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_LYRICS);
        h.media.playing=false;h.frame(vec![]);
        assert!(h.app.island_lyric().is_none(),"暫停後應退出收起態歌詞顯示");
        for _ in 0..90 {h.frame(vec![]);}
        assert!(!visible(&h,"當前歌詞") && !visible(&h,"Track"));
        assert!((h.app.current_width()-resident_width).abs()<2.0,"暫停後應恢復原常駐寬度");
        assert!((h.hit.height()-resident_height).abs()<0.1);
        assert_eq!(visible(&h,"09:41"),!minimal);
        if !minimal {
            assert!(visible(&h,"21°"),"普通常駐仍應顯示天氣");
            let secs=h.app.pomo.remaining.ceil() as u32;
            assert!(visible(&h,&format!("{:02}:{:02}",secs/60,secs%60)),"暫停音樂不能隱藏專注倒計時");
        }
        assert!(h.app.settings.island_lyrics && h.app.ly.lyrics.is_some(),"暫停只切換顯示形態，應保留開關和歌詞快取");
        assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_BACKGROUND);
        h.media.position=12.0;for _ in 0..30 {h.frame(vec![]);}
        assert!(!visible(&h,"下一句歌詞"));
        h.media.playing=true;for _ in 0..90 {h.frame(vec![]);}
        assert!(visible(&h,if synced{"下一句歌詞"}else{"Track"}),"繼續播放後應按最新進度恢復歌詞或歌名");
        assert!(!visible(&h,"09:41"));
        assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_LYRICS);
    }}
}

#[test]
fn music_lyrics_island_toggle_is_independent_and_persistent() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.show_lyrics=false;h.app.settings.lyrics_enabled=false;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]當前歌詞\n[00:10]下一句"));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.click(H::local(313.0,118.0));
    assert!(h.app.settings.island_lyrics);
    assert!(!h.app.show_lyrics && !h.app.settings.lyrics_enabled);
    let saved=crate::model::Settings::load();
    assert!(saved.island_lyrics && !saved.lyrics_enabled);
    h.hover(Pos2::new(WIN_W/2.0,PH+30.0),1.0);
    assert!(!h.app.open);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("當前歌詞"));
    let pet_width=if h.app.pet_enabled(){48.0}else{0.0};
    assert!((h.app.current_width()-crate::app::LYRIC_BAR_W-pet_width).abs()<2.0);
}

#[test]
fn music_lyric_timing_follows_saved_delay_and_preserves_playback_position(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.8,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]第一句\n[00:05]當前歌詞\n[00:10]下一句"));
    h.frame(vec![]);
    assert_eq!(h.app.settings.lyric_delay_ms,-350,"未設定時應保留原有提前 0.35 秒的預設");
    assert!((h.app.lyric_position()-10.15).abs()<0.001,"預設應提前 0.35 秒顯示，且只應用一次");
    for (delay,position,expected) in [(500,10.2,Some("當前歌詞")),(-500,9.8,Some("下一句")),(0,9.8,Some("當前歌詞")),(500,0.2,None),(500,0.6,Some("第一句"))]{
        std::fs::write(crate::model::config_dir().join("settings.txt"),format!("lyric_delay_ms={delay}\nisland_lyrics=1\n")).unwrap();
        h.app.settings=crate::model::Settings::load();
        h.media.position=position;h.frame(vec![]);
        assert_eq!(h.app.island_lyric().map(|(_,line)|line),expected,"延時 {delay} ms，播放位置 {position}");
        assert!((h.app.media.position-position).abs()<0.001,"調整歌詞不應改變播放進度");
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
        for text in ["第一句","當前歌詞","下一句"]{
            let active=h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()==text && t.galley.job.sections[0].format.font_id.size==12.0));
            assert_eq!(active,expected==Some(text),"展開歌詞也應使用相同的顯示時間");
        }
        h.app.settings.save();
        assert_eq!(crate::model::Settings::load().lyric_delay_ms,delay,"重啟後應保留時間調整");
    }
    for (input,expected) in [("invalid",-350),("999999",5000),("-999999",-5000),("",-350)]{
        std::fs::write(crate::model::config_dir().join("settings.txt"),format!("lyric_delay_ms={input}\n")).unwrap();
        assert_eq!(crate::model::Settings::load().lyric_delay_ms,expected);
    }
}

#[test]
fn music_lyric_timing_controls_match_focus_save_and_fit_the_small_settings_window(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings_tab=1;h.app.open_settings();h.ctx.set_embed_viewports(false);
    fn frame(h:&mut H,events:Vec<egui::Event>)->Vec<egui::epaint::ClippedShape>{
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(600.0,450.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.25),focused:Some(true),..Default::default()})).collect(),
                focused:true,events:events.clone(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let raw=egui::RawInput{viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(1.25),..Default::default()})).collect(),..Default::default()};
        let _=h.ctx.run(raw,|ctx|h.app.settings_ui(ctx,false));
        let child=output.lock().unwrap().take().unwrap();child.shapes
    }
    let text_rect=|shapes:&[egui::epaint::ClippedShape],text:&str|shapes.iter().find_map(|shape|match &shape.shape{
        egui::Shape::Text(t)if t.galley.text()==text=>Some(t.visual_bounding_rect()),_=>None,
    }).unwrap_or_else(||panic!("設定中缺少 {text}"));
    for _ in 0..3{frame(&mut h,vec![]);}
    let music=frame(&mut h,vec![]);
    text_rect(&music,"0.35 秒");
    assert_eq!(h.app.settings.lyric_delay_ms,-350,"僅開啟設定不能將 0.35 秒舍入為 0.4 秒");
    for button in ["−","+"]{text_rect(&music,button);}
    h.app.settings_tab=2;
    for _ in 0..3{frame(&mut h,vec![]);}
    let focus=frame(&mut h,vec![]);
    for button in ["−","+"]{
        let a=text_rect(&music,button);let b=text_rect(&focus,button);
        assert!((a.left()-b.left()).abs()<0.1 && (a.width()-b.width()).abs()<0.1 && (a.height()-b.height()).abs()<0.1,"歌詞和專注應共用同樣的數值控制元件及右側對齊");
    }
    h.app.settings_tab=1;
    for _ in 0..3{frame(&mut h,vec![]);}
    for (button,expected) in [("+",-450),("−",-350),("−",-250),("+",-350)]{
        let shapes=frame(&mut h,vec![]);
        let rect=text_rect(&shapes,button);
        assert!(Rect::from_min_max(egui::pos2(180.0,0.0),egui::pos2(600.0,450.0)).contains_rect(rect),"時間調整按鈕應完整顯示");
        let pos=rect.center();
        frame(&mut h,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        frame(&mut h,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert_eq!(h.app.settings.lyric_delay_ms,expected);
        assert_eq!(crate::model::Settings::load().lyric_delay_ms,expected,"按鈕調整後應自動儲存");
        text_rect(&frame(&mut h,vec![]),&format!("{:.2} 秒",-expected as f64/1000.0));
    }
}

#[test]
fn island_lyrics_arrive_just_before_vocals_and_finish_fading_before_the_timestamp(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.4,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]當前歌詞\n[00:10]下一句"));
    for _ in 0..60 {h.frame(vec![]);}
    for step in 1..=30 {
        h.media.position=9.4+step as f64/60.0;
        h.frame(vec![]);
        if step==6 {assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("當前歌詞"),"不能提前半秒就跳到下一句");}
    }
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("下一句"),"下一句應在開唱前提前出現");
    let text=h.shapes.iter().find_map(|shape|match &shape.shape {
        egui::Shape::Text(text) if text.galley.text()=="下一句"=>Some(text),_=>None,
    }).expect("開唱前應能看到下一句");
    assert!(text.opacity_factor>0.95,"開唱前應完成淡入，不能等到開唱後才清晰");
    h.media.playing=false;h.frame(vec![]);
    assert!(h.app.island_lyric().is_none(),"暫停時應退出收起態歌詞顯示");
    h.media.playing=true;h.frame(vec![]);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("下一句"),"繼續播放時不應跳回上一句");
    h.media.position=4.5;h.frame(vec![]);
    assert!(h.app.island_lyric().is_none(),"前奏不能過早顯示第一句");
    h.media.position=4.75;h.frame(vec![]);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("當前歌詞"),"第一句也應提前出現");
}

#[test]
fn expanded_and_segmented_lyrics_share_the_early_display_time(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.8,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]當前歌詞\n[00:10]下一句"));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    let next=h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()=="下一句"=>Some(text),_=>None}).unwrap();
    assert_eq!(next.galley.job.sections[0].format.font_id.size,12.0,"展開歌詞也應在開唱前高亮下一句");
    h.hover(Pos2::new(WIN_W/2.0,PH+30.0),1.0);
    let line="願每一個認真生活的人，都能在平凡的日子裡找到屬於自己的光";
    h.media.position=6.0;h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc(&format!("[00:05]{line}\n[00:15]下一句")));
    for _ in 0..60{h.frame(vec![]);}
    let shown=|h:&H|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if line.contains(t.galley.text())&&!t.galley.text().is_empty()=>Some(t.galley.text().to_owned()),_=>None}).unwrap();
    let first=shown(&h);
    let boundary=5.0+10.0*first.chars().count()as f64/line.chars().count()as f64;
    h.media.position=boundary-0.2;for _ in 0..30{h.frame(vec![]);}
    assert_ne!(shown(&h),first,"長句的後半段也應使用相同的提前量");
}

#[test]
fn lyric_short_gap_and_line_switch_keep_text_visible(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:2.0,playing:true,..Default::default()};
    h.app.set_lyrics("Track","Artist",0.0,crate::lyrics::parse_lrc("[00:01]上一句\n[00:05]下一句"));
    for _ in 0..60{h.frame(vec![]);}
    for (position,expected) in [(3.0,"上一句"),(4.64,"上一句"),(4.7,"下一句"),(4.72,"下一句"),(4.9,"下一句")]{
        h.media.position=position;h.frame(vec![]);
        let text=h.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text)if text.galley.text()==expected=>Some(text),_=>None,
        }).expect("短停頓及換句時必須持續顯示歌詞");
        assert!(text.opacity_factor>0.95,"換句不能淡出歌詞，位置 {position}，透明度 {}",text.opacity_factor);
    }
}

#[test]
fn island_lyrics_center_segments_in_a_fixed_height(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,playing:true,..Default::default()};
    for line in ["晚風","願每一個認真生活的人，都能在平凡的日子裡找到屬於自己的光","晚風"] {
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc(&format!("[00:05]{line}\n[00:15]下一句")));
        for _ in 0..90 {
            h.frame(vec![]);
            assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="09:41")),"切換顯示段時應保持歌詞佈局");
        }
        let text=h.shapes.iter().find_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if !text.galley.text().is_empty() && line.contains(text.galley.text())=>Some(text),_=>None,
        }).expect("島上應顯示當前歌詞的一段");
        let center=text.pos.x+text.galley.rect.center().x;
        let offset=if h.app.pet_enabled(){24.0}else{0.0};
        assert!((center-WIN_W/2.0-offset).abs()<1.5,"歌詞應在內容區居中");
        assert!(!text.galley.elided,"顯示段應完整顯示");
        assert_eq!(text.galley.rows.len(),1,"每次只顯示一段歌詞");
        assert!((h.hit.height()-(crate::app::BAR_H+4.0)).abs()<0.1,"島應保持固定高度");
        assert!(text.visual_bounding_rect().bottom()<=h.hit.bottom(),"文字應完整顯示在島內");
    }
}

#[test]
fn island_lyric_segments_follow_playback_pause_and_seek(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    let line="願每一個認真生活的人，都能在平凡的日子裡找到屬於自己的光";
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:6.0,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc(&format!("[00:05]{line}\n[00:15]下一句")));
    let shown=|h:&H|h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t)=>Some(t.galley.text().to_owned()),_=>None}).unwrap();
    for _ in 0..90 {h.frame(vec![]);}let first=shown(&h);
    h.media.position=14.0;for _ in 0..30 {h.frame(vec![]);}let last=shown(&h);
    assert_ne!(first,last,"播放到後半句時應切換顯示段");
    assert_eq!(format!("{first}{last}"),line,"前後顯示段應覆蓋完整歌詞");
    h.media.playing=false;for _ in 0..120 {h.frame(vec![]);}
    assert!(h.app.island_lyric().is_none(),"暫停時應恢復常駐形態");
    h.media.playing=true;for _ in 0..30 {h.frame(vec![]);}assert_eq!(shown(&h),last,"繼續播放時應恢復對應顯示段");
    h.media.position=6.0;for _ in 0..30 {h.frame(vec![]);}assert_eq!(shown(&h),first,"拖回前半句應恢復對應顯示段");
    h.media.position=16.0;for _ in 0..30 {h.frame(vec![]);}assert_eq!(shown(&h),"下一句");
}

#[test]
fn island_lyrics_keep_title_fallback_and_intro_and_gaps(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:1.0,duration:120.0,playing:true,..Default::default()};
    for state in 0..6 {
        h.app.ly=Default::default();
        h.app.ly.key="Track|Artist|120".into();
        h.media.position=if state==1 {7.0}else{1.0};
        match state {
            0=>h.app.ly.lyrics=Some(crate::lyrics::parse_lrc("[00:05]第一句")),
            1=>h.app.ly.lyrics=Some(crate::lyrics::parse_lrc("[00:01]第一句\n[00:05]\n[00:10]下一句")),
            2=>h.app.ly.loading=true,
            3=>h.app.ly.error=Some(crate::lyrics::FetchError::NotFound),
            4=>h.app.ly.error=Some(crate::lyrics::FetchError::Unavailable("503".into())),
            _=>h.app.ly.lyrics=Some(crate::lyrics::from_plain("沒有時間戳")),
        }
        for _ in 0..90 {h.frame(vec![]);}
        if let Some(error)=&h.app.ly.error {
            assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==error.label())),"歌詞錯誤提示只應在展開內顯示");
        }
        assert!((h.app.current_width()-crate::app::LYRIC_BAR_W).abs()<2.0,"狀態 {state} 應保持音樂訊息島寬度");
        assert!(h.region.height()>30.0);
        if state>=2 {
            assert!(h.app.compact_notices(),"歌名兜底仍屬於島上歌詞模式");
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Track"&&t.opacity_factor>0.95)),"狀態 {state} 應顯示當前歌名");
        } else {
            let label=if state==0{"前奏"}else{"間奏"};
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==label)),"同步歌詞的前奏和間奏仍應顯示狀態");
        }
    }
}

#[test]
fn island_lyrics_title_fallback_shows_cover_and_elides_long_names(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"沒有歌詞的鋼琴曲".repeat(20),artist:"示例演奏者".into(),playing:true,artwork:Some(std::sync::Arc::new(egui::ColorImage::new([8,8],egui::Color32::BLUE))),..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}
    let text=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==h.media.title=>Some(t),_=>None}).expect("無歌詞時應繪製當前歌名");
    assert_eq!(text.galley.rows.len(),1,"長歌名只顯示一行");
    assert!(text.galley.elided,"長歌名應在末尾省略");
    assert!(h.region.contains_rect(text.visual_bounding_rect()),"長歌名不能超出島體");
    let cover=h.app.music_artwork.as_ref().unwrap().1.id();
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r)if r.brush.as_ref().is_some_and(|b|b.fill_texture_id==cover)&&h.region.contains_rect(r.rect))),"無歌詞時仍應繪製封面");
}

#[test]
fn island_lyrics_title_fallback_follows_track_changes_and_normal_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"上一首".into(),artist:"示例歌手".into(),position:7.0,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("上一首","示例歌手",120.0,crate::lyrics::parse_lrc("[00:05]上一首的歌詞"));
    for _ in 0..90 {h.frame(vec![]);}
    h.media.title="下一首純音樂".into();h.media.playing=false;
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.island_lyric().is_none(),"換歌后不能顯示上一首歌詞");
    assert!(h.region.height()<20.0,"暫停後應恢復極簡常駐");
    h.media.playing=true;for _ in 0..60 {h.frame(vec![]);}
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==h.media.title)),"繼續播放且歌詞未到達時應顯示新歌名");
    h.media.playing=false;
    h.app.set_lyrics("下一首純音樂","示例歌手",120.0,crate::lyrics::parse_lrc("[00:05]新歌的當前歌詞"));
    for _ in 0..30 {h.frame(vec![]);}
    assert!(h.app.island_lyric().is_none(),"暫停時歌詞到達也應保持常駐形態");
    h.media.playing=true;for _ in 0..30 {h.frame(vec![]);}
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="新歌的當前歌詞")),"繼續播放後應自動恢復當前歌詞");
    for mode in 0..3 {
        h.app.settings.island_lyrics=mode!=0;
        h.app.settings.visible_pages[0]=mode!=1;
        if mode==2 {h.media=Default::default();}
        for _ in 0..90 {h.frame(vec![]);}
        assert!(h.region.height()<20.0,"關閉歌詞、隱藏音樂頁或播放器退出後應恢復極簡常駐，狀態 {mode}");
        assert!(!h.app.compact_notices());
    }
}

#[test]
#[ignore="按需匯出島上歌詞的實際渲染畫面"]
fn island_lyrics_layout_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_LYRIC_PREVIEW_DIR").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    let mut cover=egui::ColorImage::new([64,64],egui::Color32::BLACK);
    for y in 0..64 {for x in 0..64 {
        let moon=(x as i32-44).pow(2)+(y as i32-18).pow(2)<64;
        cover[(x,y)]=if moon {egui::Color32::from_rgb(255,226,174)}
            else if y>44+x/5 {egui::Color32::from_rgb(29,61,70)}
            else {egui::Color32::from_rgb(68+y as u8,114+y as u8,151+y as u8/2)};
    }}
    let cover=std::sync::Arc::new(cover);
    for name in ["lyrics","loading","missing","no-cover","long-title","paused","intro","gap","idle"] {
        let mut h=H::new();h.app.settings.island_lyrics=true;
        h.media=crate::smtc::NowPlaying{title:"晚風與星光".into(),artist:"示例歌手".into(),position:7.0,duration:120.0,playing:name!="paused",artwork:Some(cover.clone()),..Default::default()};
        if name=="long-title" {h.media.title="願每一個認真生活的人都能在平凡的日子裡找到屬於自己的光（鋼琴獨奏版）".into();}
        h.app.ly.key=format!("{}|{}|120",h.media.title,h.media.artist);
        match name {
            "lyrics"|"paused"=>h.app.set_lyrics("晚風與星光","示例歌手",120.0,crate::lyrics::parse_lrc("[00:05]願你在平凡的日子裡，找到自己的光")),
            "loading"=>h.app.ly.loading=true,
            "intro"=>h.app.set_lyrics("晚風與星光","示例歌手",120.0,crate::lyrics::parse_lrc("[00:20]晚風")),
            "gap"=>h.app.set_lyrics("晚風與星光","示例歌手",120.0,crate::lyrics::parse_lrc("[00:01]第一句\n[00:05]\n[00:10]下一句")),
            "idle"=>h.media=Default::default(),
            _=>h.app.ly.error=Some(crate::lyrics::FetchError::NotFound),
        }
        if name=="no-cover" {h.media.artwork=None;}
        let mut raster=crate::preview_raster::Raster::new(1280,540);
        for frame in 0..90 {
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),today:"2026-11-06".into(),date:"11月6日 週五".into(),media:h.media.clone(),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))),time:Some(t),
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});
            });
            raster.update_textures(&output.textures_delta);
            if frame==89 {
                raster.buf.fill([0.95,0.96,0.98,1.0]);
                raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw(1280,540,rgba).unwrap();
                image::imageops::crop_imm(&image,220,0,840,110).to_image().save(dest.join(format!("{name}.png"))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
fn music_lyrics_keep_syncing_when_only_the_collapsed_lyrics_are_enabled() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.opts.fetch_lyrics=true;h.app.show_lyrics=false;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,playing:true,..Default::default()};
    let (tx,rx)=std::sync::mpsc::channel();
    h.app.ly.key="Track|Artist|120".into();h.app.ly.loading=true;h.app.ly.rx=Some(rx);
    tx.send(Ok(crate::lyrics::parse_lrc("[00:05]當前歌詞\n[00:10]下一句"))).unwrap();
    h.frame(vec![]);
    assert!(!h.app.ly.loading);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("當前歌詞"));
    h.media.position=12.0;h.frame(vec![]);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("下一句"));
}

#[test]
fn music_lyrics_respect_the_switch_and_do_not_show_stale_or_unsynced_lines() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]當前歌詞"));
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()>30.0,"主動開啟島上歌詞後應可在收起狀態閱讀");
    h.app.settings.island_lyrics=false;
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.island_lyric().is_none() && h.region.height()<20.0);
    h.app.settings.island_lyrics=true;
    h.media.title="Another track".into();h.frame(vec![]);
    assert!(h.app.island_lyric().is_none(),"換歌后不能顯示上一首的歌詞");
    h.app.set_lyrics("Another track","Artist",120.0,crate::lyrics::from_plain("沒有時間戳的歌詞"));h.frame(vec![]);
    assert!(h.app.island_lyric().is_none(),"不能將純文字歌詞偽裝成當前句");
}

#[test]
fn hover_expands_with_overshoot_and_leave_collapses() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 0.1);
    assert!(h.app.progress() < 0.1, "意圖緩衝期內不應展開");
    let mut peak: f32 = 0.0;
    for _ in 0..60 {
        h.hover(Pos2::new(WIN_W / 2.0, 12.0), 1.0 / 60.0);
        peak = peak.max(h.app.progress());
    }
    assert!(peak > 1.01 && peak < 1.12, "展開應有輕微過沖, peak={peak}");
    assert!((h.app.progress() - 1.0).abs() < 0.02);
    h.cursor = Some(Pos2::new(WIN_W / 2.0, PH + 30.0));
    for _ in 0..40 {
        h.frame(vec![]);
    }
    assert!(h.app.progress() < 0.02, "移開後應收起");
}

#[test]
fn browser_minimal_preserves_preferences_and_restores_the_previous_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=true;h.app.settings.minimal=false;h.browser_at_top=Some(true);
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.hit.height()>20.0,"開關預設關閉時不應自動改變模式");
    h.app.settings.browser_minimal=true;h.app.settings.save();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]當前歌詞"));
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.hit.height()<10.0,"瀏覽器頂部受遮擋時應收為細劉海，歌詞和吉祥物不能撐大它");
    assert!(h.app.settings.pet && !h.app.settings.minimal,"自動極簡不能修改手動模式");
    let saved=crate::model::Settings::load();
    assert!(saved.browser_minimal && saved.pet && !saved.minimal,"應只持久化開關，保留原模式");
    h.browser_at_top=None;for _ in 0..30 {h.frame(vec![]);}
    assert!(h.hit.height()<10.0,"開啟島仔設定或系統匣時應保留瀏覽器避讓狀態");
    h.browser_at_top=Some(false);for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.pet_enabled() && h.hit.height()>20.0,"離開瀏覽器或還原視窗後應恢復");
    h.browser_at_top=Some(true);for _ in 0..90 {h.frame(vec![]);}
    h.app.settings.browser_minimal=false;for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.pet_enabled() && h.hit.height()>20.0,"關閉開關後應立即恢復");
    h.app.settings.island_lyrics=false;h.app.set_minimal(true);h.browser_at_top=Some(false);for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.settings.minimal && h.hit.height()<10.0,"離開瀏覽器不能撤銷手動極簡");
}

#[test]
fn browser_minimal_closes_the_open_island_but_still_allows_deliberate_hover(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.browser_minimal=true;
    h.app.show_page(0);h.hover(H::local(300.0,60.0),0.8);
    assert!(h.app.open);
    h.browser_at_top=Some(true);h.frame(vec![]);
    assert!(!h.app.open,"切到瀏覽器時不能繼續保留展開狀態遮擋標籤欄");
    assert!(!h.hit.contains(Pos2::new(WIN_W/2.0,12.0)),"收起動畫期間也應讓標籤欄接收點選");
    h.hover(Pos2::new(WIN_W/2.0,2.0),0.25);
    assert!(!h.app.open,"短暫碰到劉海不能誤觸");
    h.hover(Pos2::new(WIN_W/2.0,2.0),0.6);
    assert!(h.app.open,"仍應能主動懸停開啟島");
}

#[test]
fn minimal_hover_excludes_the_surrounding_browser_tabs() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    for _ in 0..90 {h.frame(vec![]);}
    for cursor in [
        Pos2::new(WIN_W/2.0,crate::app::BAR_H_MIN+1.0),
        Pos2::new(WIN_W/2.0+crate::app::BAR_W_MIN/2.0+2.0,2.0),
        Pos2::new(WIN_W/2.0-crate::app::BAR_W_MIN/2.0-2.0,2.0),
    ] {
        assert!(!h.hit.contains(cursor),"劉海外的標籤欄應接收滑鼠操作");
        h.hover(cursor,1.0);
        assert!(!h.app.open,"滑鼠未觸碰極簡劉海時不得展開");
    }
}

#[test]
fn minimal_hover_requires_continuous_deliberate_contact() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    for _ in 0..90 {h.frame(vec![]);}
    let inside=Pos2::new(WIN_W/2.0,2.0);
    h.hover(inside,0.25);
    assert!(!h.app.open,"短暫經過極簡劉海不得展開");
    h.hover(Pos2::new(WIN_W/2.0,8.0),0.1);
    h.hover(inside,0.25);
    assert!(!h.app.open,"移出劉海應重新計算停留時間");
    h.hover(inside,0.7);
    assert!(h.app.open,"在劉海內持續停留仍應展開");
    h.click(H::local(398.0,68.0));
    assert_eq!(h.app.page,1,"展開後應能正常操作面板");
}

#[test]
fn minimal_hover_does_not_catch_tabs_during_collapse() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    h.hover(Pos2::new(WIN_W/2.0,2.0),1.0);
    assert!(h.app.open);
    h.cursor=Some(Pos2::new(WIN_W/2.0,PH+30.0));
    for _ in 0..30 {
        h.frame(vec![]);
        if !h.app.open {break;}
    }
    assert!(!h.app.open);
    assert!(h.app.progress()>0.04,"檢查收起動畫仍在進行時的命中範圍");
    let tab=Pos2::new(WIN_W/2.0,12.0);
    assert!(!h.hit.contains(tab),"收起動畫不得繼續攔截標籤欄");
    h.hover(tab,1.0);
    assert!(!h.app.open);
}

#[test]
fn music_header_shortcut_returns_from_every_page_without_controlling_playback() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for playing in [true,false] {
        let mut h=H::new();h.app.settings.page_order=[5,2,0,1,3,4,6,7,8];
        h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"Track".into(),artist:"Artist".into(),playing,duration:120.0,..Default::default()};
        for page in 1..crate::app::PAGE_NAMES.len() {
            h.app.show_page(page);h.hover(H::local(300.0,60.0),0.8);h.cmds.clear();
            h.click(H::local(367.0,18.0));
            assert_eq!(h.app.page,0,"播放狀態 {playing} 時應從頁面 {page} 直達音樂頁");
            assert!(h.app.open);assert!(!h.app.settings_open);assert_eq!(h.app.media.playing,playing);
            assert!(h.cmds.iter().all(|cmd|!matches!(cmd,crate::app::Cmd::Media(_)|crate::app::Cmd::MusicVolume(..))),"返回音樂頁不能改變播放或音量");
        }
    }
}

#[test]
fn music_header_shortcut_keeps_hidden_music_page_disabled() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    h.app.set_page_visible(0,false);h.app.show_page(4);h.hover(H::local(300.0,60.0),0.8);
    h.click(H::local(367.0,18.0));
    assert_eq!(h.app.page,4);assert!(!h.app.settings.visible_pages[0]);
    assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,1);
}

#[test]
fn music_header_shortcut_is_absent_without_music_and_keeps_settings_clickable() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for media in [crate::smtc::NowPlaying::default(),crate::smtc::NowPlaying{title:"Video".into(),artist:"Channel".into(),playing:true,playback_type:crate::smtc::MediaKind::Video,..Default::default()}] {
        let mut h=H::new();h.media=media;h.app.show_page(4);h.hover(H::local(300.0,60.0),0.8);
        h.click(H::local(367.0,18.0));assert_eq!(h.app.page,4);assert!(!h.app.settings_open);
        h.click(H::local(394.0,18.0));assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,4);
    }
}

#[test]
fn arrows_and_dots_switch_pages() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 1.0);
    h.click(H::local(398.0, 68.0));
    assert_eq!(h.app.page, 1);
    h.hover(H::local(300.0, 60.0), 0.8);
    h.click(H::local(21.0, 68.0));
    assert_eq!(h.app.page, 0);
    h.hover(H::local(300.0, 60.0), 0.8);
    h.click(H::local(398.0, 68.0));
    h.hover(H::local(300.0, 60.0), 0.8);
    h.click(H::local(398.0, 68.0));
    assert_eq!(h.app.page, 2);
}

#[test]
fn wheel_navigation_reaches_every_page_in_both_directions(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.shelf.items.clear();h.app.todos.items.clear();
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.hover(H::local(210.0,18.0),0.3);
    for (delta,pages) in [(-40.0,[1,2,3,4,5,6,7,8,0]),(40.0,[8,7,6,5,4,3,2,1,0])] {
        for page in pages {
            h.wheel(delta);
            assert_eq!(h.app.page,page,"滾輪翻頁不能被空的置物架或待辦卡住");
            h.hover(H::local(210.0,18.0),0.25);
        }
    }
}

#[test]
fn wheel_scrolls_lists_only_inside_content_and_respects_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=(0..4).map(|i|crate::model::TodoItem{text:format!("任務 {i}"),done_on:None,due:None,reminded:false,..Default::default()}).collect();
    h.app.shelf.items=(0..6).map(|i|std::path::PathBuf::from(format!("scroll-test-{i}.txt"))).collect();
    for page in [3,4] {
        h.app.show_page(page);h.hover(H::local(300.0,60.0),1.0);
        h.wheel(-40.0);
        assert_eq!(h.app.page,page,"有更多內容時應先滾動列表");
        assert_eq!(if page==3 {h.app.shelf_first}else{h.app.todo_first},1);
        h.hover(H::local(300.0,60.0),0.25);h.wheel(-40.0);
        assert_eq!(h.app.page,page,"內容溢位時滾到底也應留在本頁");
        h.app.show_page(page);h.hover(H::local(210.0,18.0),0.3);h.wheel(-40.0);
        assert_eq!(h.app.page,page+1,"頂欄滾輪不應被內容列表攔截");
    }
}

#[test]
fn wheel_overflowing_shelf_stays_at_both_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for count in [6,9]{
        h.app.shelf.items=(0..count).map(|i|std::path::PathBuf::from(format!("scroll-test-{i}.txt"))).collect();
        h.app.shelf_first=0;h.app.show_page(3);
        let last=count-5;
        for (delta,first) in [(40.0_f32,0),(-40.0,1),(-400.0,last),(-40.0,last),(40.0,last-1),(400.0,0),(40.0,0)]{
            h.hover(H::local(300.0,60.0),0.7);
            for _ in 0..(delta.abs() as usize/40){h.wheel(delta.signum()*40.0);}
            assert_eq!(h.app.page,3,"置物架溢位後連續滾動或停頓再滾都應留在本頁，條數 {count}");
            assert_eq!(h.app.shelf_first,first);
        }
        for (delta,page) in [(-40.0,4),(40.0,2)]{
            h.hover(H::local(300.0,60.0),0.7);
            h.click(H::local(if delta<0.0 {398.0}else{21.0},68.0));
            assert_eq!(h.app.page,page,"置物架溢位後仍應允許箭頭切頁");
            h.app.show_page(3);h.hover(H::local(210.0,18.0),0.3);h.wheel(delta);
            assert_eq!(h.app.page,page,"置物架頂欄仍應允許滾輪翻頁");
            h.app.show_page(3);
        }
    }
}

#[test]
fn wheel_overflowing_todos_stay_at_both_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for count in [4,8]{
        h.app.todos.items=(0..count).map(|i|crate::model::TodoItem{text:format!("任務 {i}"),..Default::default()}).collect();
        h.app.todo_first=0;h.app.show_page(4);
        let last=count-3;
        for (delta,first) in [(40.0_f32,0),(-40.0,1),(-400.0,last),(-40.0,last),(40.0,last-1),(400.0,0),(40.0,0)]{
            h.hover(H::local(300.0,60.0),0.3);
            for _ in 0..(delta.abs() as usize/40){h.wheel(delta.signum()*40.0);}
            assert_eq!(h.app.page,4,"待辦溢位後滾動到首尾都不應翻頁，條數 {count}");
            assert_eq!(h.app.todo_first,first);
        }
        for (delta,page) in [(-40.0,5),(40.0,3)]{
            h.app.show_page(4);h.hover(H::local(100.0,60.0),0.3);h.wheel(delta);
            assert_eq!(h.app.page,page,"待辦列表外仍應允許滾輪翻頁");
        }
    }
}

#[test]
fn wheel_clipboard_stays_at_both_edges() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.show_page(3);
    h.app.shelf_tab=1;
    for count in [crate::pages::CLIP_ROWS+1] {
        h.app.clipboard.lock().items.clear();
        for i in 0..count { h.app.clipboard.lock().push(&i.to_string(), "09:00"); }
        let last=count.saturating_sub(crate::pages::CLIP_ROWS);
        for (delta, first) in [(40.0,0),(-40.0,last),(-40.0,last),(40.0,0),(40.0,0)] {
            h.hover(H::local(300.0,60.0),0.4);
            h.wheel(delta);
            assert_eq!(h.app.page,3);
            assert_eq!(h.app.clip_first,first);
        }
        for (delta,page) in [(-40.0,4),(40.0,2)]{
            h.app.show_page(3);h.hover(H::local(210.0,18.0),0.4);h.wheel(delta);
            assert_eq!(h.app.page,page,"剪貼簿列表外仍應允許滾輪翻頁");
        }
    }
}
#[test]
fn wheel_handles_small_deltas_without_skipping_pages_on_bursts(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.hover(H::local(210.0,18.0),0.3);
    for _ in 0..16 {h.wheel(-0.5);}
    assert_eq!(h.app.page,1,"高精度滾輪的小幅輸入應能累積成一次翻頁");
    h.hover(H::local(210.0,18.0),0.3);
    for _ in 0..5 {h.wheel(-40.0);}
    assert_eq!(h.app.page,2,"一次連續滾動不能瞬間跳過多個頁面");
}

#[test]
fn navigation_wraps_between_nine_pages_and_notification_settings_remain_accessible() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.click(H::local(21.0,68.0));assert_eq!(h.app.page,8);
    h.hover(H::local(300.0,60.0),0.8);h.click(H::local(398.0,88.0));assert_eq!(h.app.page,0);
    h.app.open_settings_category(3);assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,6);
}

#[test]
fn resident_quota_moves_left_and_releases_battery_space(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=false;
    h.power=(Some(80),true);h.quota_accounts(1);
    for _ in 0..90 {h.frame(vec![]);}
    let before=h.quota_centers()[0];let width=h.app.current_width();assert!(before>WIN_W/2.0);
    h.app.settings.show_battery=false;for _ in 0..90 {h.frame(vec![]);}
    let after=h.quota_centers()[0];assert!(after<WIN_W/2.0,"關閉電量後，用量應補到時間左側");
    assert!(h.app.current_width()<width-20.0,"關閉元件後應釋放實際佔位");
    h.app.settings.show_battery=true;h.power=(None,false);for _ in 0..90 {h.frame(vec![]);}
    assert!((h.quota_centers()[0]-after).abs()<0.2,"無電池裝置應採用相同補位佈局");
    h.click(Pos2::new(after,15.0));assert_eq!(h.app.page(),6,"補位後仍可點選用量圓環");
}

#[test]
fn resident_quotas_coexist_with_music_and_focus(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=false;h.quota_accounts(3);
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    h.app.pomo.running=true;h.app.pomo.remaining=600.0;
    for _ in 0..90 {h.frame(vec![]);}
    assert_eq!(h.quota_centers().len(),3,"播放和專注狀態不應擠掉常駐用量");
    assert!(h.app.current_width()<PW,"多元件應在單行內排布");
    h.app.settings.minimal=true;for _ in 0..90 {h.frame(vec![]);}
    assert!(h.quota_centers().is_empty());assert!(h.region.height()<20.0,"仍須尊重極簡開關");
}

#[test]
fn resident_fifth_account_stays_clear_of_clock_and_opens(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=false;h.app.settings.show_battery=false;h.quota_accounts(5);
    for _ in 0..90 {h.frame(vec![]);}
    let text_rect=|label:&str|h.shapes.iter().find_map(|shape|match &shape.shape{
        egui::Shape::Text(text) if text.galley.text()==label=>Some(text.visual_bounding_rect()),_=>None,
    }).expect("常駐文字應可見");
    let last=h.app.resident.current().x[crate::bar_layout::quota_slot(4)].unwrap()+WIN_W/2.0;
    assert!(last+13.5+8.0<text_rect("09:41").left());
    h.click(Pos2::new(last,h.region.center().y));assert_eq!(h.app.page(),6);
}

#[test]
#[ignore = "按需匯出常駐元件排布與補位動畫"]
fn resident_layout_gallery(){
    fn capture(h:&mut H,raster:&mut crate::preview_raster::Raster,t:f64,draw:bool)->Vec<u8>{
        let env=Env{t,clock:"09:41:00".into(),today:"2026-10-31".into(),date:"10月31日 週六".into(),battery:h.power.0,charging:h.power.1,media:h.media.clone(),bt_ready:true,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))),time:Some(t),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});});
        raster.update_textures(&output.textures_delta);let mut rgba=Vec::new();
        if draw {raster.buf.fill([0.09,0.11,0.18,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);raster.to_rgba8(&mut rgba);}
        raster.free_textures(&output.textures_delta);rgba
    }
    fn demo(battery:bool,accounts:usize,music:bool,focus:bool,pet:bool)->H{
        let mut h=H::new();h.app.settings.show_battery=battery;h.app.settings.pet=pet;h.power=(Some(80),true);h.quota_accounts(accounts);
        for (i,a) in h.app.quota.config.accounts.iter().enumerate(){
            if let Some(Ok(snapshot))=h.app.quota.rows.get_mut(&a.id){
                snapshot.windows[0].remaining=82.0-i as f32*11.0;
                if i%2==0 {snapshot.windows.push(crate::quota::Window{label:"5 小時".into(),seconds:Some(18000),remaining:58.0,reset:None,amount:None});}
            }
        }
        if music {h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};}
        if focus {h.app.pomo.running=true;h.app.pomo.remaining=1283.0;}
        if pet {h.app.pet.react(crate::extras::PetState::Happy);}
        h
    }
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_RESIDENT_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let scenes=[(true,1,false,false,false),(false,1,false,false,false),(true,3,false,false,false),(false,3,true,false,false),
        (true,3,true,false,false),(false,3,true,true,false),(false,5,false,false,false),(false,2,false,false,true)];
    for (i,(battery,accounts,music,focus,pet)) in scenes.into_iter().enumerate(){
        let mut h=demo(battery,accounts,music,focus,pet);let mut raster=crate::preview_raster::Raster::new(1280,380);
        for frame in 0..90 {
            let rgba=capture(&mut h,&mut raster,frame as f64/60.0,frame==89);
            if frame==89 {image::save_buffer(dest.join(format!("layout-{i}.png")),&rgba,1280,380,image::ColorType::Rgba8).unwrap();}
        }
    }
    let mut h=demo(true,3,true,false,false);let mut raster=crate::preview_raster::Raster::new(1280,380);
    for frame in 0..240 {
        if frame==60 {h.app.settings.show_battery=false;}if frame==180 {h.app.settings.show_battery=true;}
        let rgba=capture(&mut h,&mut raster,frame as f64/60.0,frame%3==0);
        if frame%3==0 {
            let image=image::RgbaImage::from_raw(1280,380,rgba).unwrap();
            image::imageops::crop_imm(&image,200,0,880,80).to_image().save(dest.join(format!("frame-{frame:03}.png"))).unwrap();
        }
    }
    eprintln!("常駐元件預覽：{}",dest.display());
}

#[test]
fn quota_text_stays_inside_the_inner_ring_at_zero_and_full_usage() {
    use crate::quota::{Event,Snapshot,Window};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dual in [true,false] {for remaining in [72.0,0.0,100.0] {
        let mut h=H::new();h.quota_accounts(1);
        let reset=Some((chrono::Local::now()+chrono::Duration::days(400)).timestamp());
        let mut windows=vec![Window{label:"本週".into(),seconds:Some(604800),remaining,reset,amount:None}];
        if dual {windows.insert(0,Window{label:"5 小時".into(),seconds:Some(18000),remaining,reset,amount:None});}
        h.app.quota_event(Event{id:"preview-0".into(),result:Ok(Snapshot{windows,..Default::default()})});
        h.app.show_page(6);h.hover(H::local(300.0,60.0),1.0);
        let center=H::local(103.0,105.0);
        let inner=h.shapes.iter().filter_map(|shape|match &shape.shape {
            egui::Shape::Circle(circle) if circle.center.distance(center)<0.5 && circle.radius>15.0 && circle.stroke.width>0.0=>Some(circle.radius-circle.stroke.width/2.0),
            egui::Shape::Path(path) if path.points.len()>2 && path.stroke.width>0.0=>{
                let radius=path.points[0].distance(center);
                ((15.0..35.0).contains(&radius) && path.points.iter().all(|point|(point.distance(center)-radius).abs()<0.5)).then_some(radius-path.stroke.width/2.0)
            },
            _=>None,
        }).reduce(f32::min).unwrap_or(35.0);
        for shape in &h.shapes {if let egui::Shape::Text(text)=&shape.shape {
            let rect=text.visual_bounding_rect();
            if rect.center().distance(center)<35.0 {
                for corner in [rect.left_top(),rect.right_top(),rect.left_bottom(),rect.right_bottom()] {
                    assert!(corner.distance(center)+0.5<inner,"{} 的文字不得碰到用量環，雙週期={dual}，剩餘={remaining}",text.galley.text());
                }
            }
        }}
        let texts:Vec<_>=h.shapes.iter().filter_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if text.visual_bounding_rect().left()>=H::local(157.0,0.0).x && text.visual_bounding_rect().top()>=68.0 && text.visual_bounding_rect().bottom()<=144.0=>Some((text.galley.text().to_owned(),text.visual_bounding_rect())),_=>None,
        }).collect();
        for (i,(a,rect)) in texts.iter().enumerate(){for (b,other) in &texts[i+1..]{assert!(!rect.intersects(*other),"{a} 與 {b} 不得重疊");}}
        let label=if dual {"5 小時"}else{"本週剩餘"};
        assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()==label)),"週期說明應完整可見");
    }}
}

fn kiro_snapshot(remaining:f64,limit:f64)->crate::quota::Snapshot{
    crate::quota::parse(crate::quota::Provider::Kiro,&serde_json::json!({"userInfo":{"userId":"ui-kiro-user"},"subscriptionInfo":{"subscriptionTitle":"KIRO PRO"},
        "usageBreakdownList":[{"resourceType":"CREDIT","usageLimitWithPrecision":limit,"currentUsageWithPrecision":limit-remaining,"nextDateReset":4102444800_i64}]})).unwrap()
}

#[test]
fn kiro_cancelled_or_closed_connections_cannot_save_late_results(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for close_settings in [false,true]{
        let mut h=H::new();let saved=h.app.quota.config.clone();h.app.settings_open=true;h.app.quota.connect_open=true;
        let(connect,tx)=crate::quota::kiro::Connect::pending(None);h.app.quota.kiro_connecting=Some(connect);
        if close_settings{h.app.settings_open=false;}else{h.app.quota.connect_open=false;}
        h.app.quota_tick();assert!(h.app.quota.kiro_connecting.as_ref().unwrap().cancelled());
        tx.send(Ok(crate::quota::kiro::Connected{source:"kiro.json".into(),label:"Kiro".into(),snapshot:kiro_snapshot(900.0,1000.0)})).unwrap();
        h.app.quota_tick();assert_eq!(h.app.quota.config,saved);assert!(h.app.quota.kiro_connecting.is_none());assert!(h.app.quota.rows.is_empty());
    }
}

#[test]
fn kiro_connection_save_failure_keeps_the_original_account_and_cache(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings_open=true;h.app.quota.connect_open=true;
    let snapshot=kiro_snapshot(900.0,1000.0);let owner=snapshot.kiro.as_ref().unwrap().owner.clone();
    let account=crate::quota::Account{id:"kiro".into(),provider:crate::quota::Provider::Kiro,alias:"工作".into(),source:"old.json".into(),slot:Some(owner),enabled:true,pinned:true};
    h.app.quota.config.accounts.push(account.clone());h.app.quota.rows.insert(account.id.clone(),Ok(snapshot.clone()));let saved=h.app.quota.config.clone();
    let(connect,tx)=crate::quota::kiro::Connect::pending(Some(account));h.app.quota.kiro_connecting=Some(connect);
    tx.send(Ok(crate::quota::kiro::Connected{source:"new.json".into(),label:"新備註".into(),snapshot})).unwrap();
    let original=std::env::var_os("DAOZAI_DATA_DIR").unwrap();std::fs::create_dir_all(&original).unwrap();let blocked=std::path::PathBuf::from(&original).join("kiro-blocked");std::fs::write(&blocked,"blocked").unwrap();std::env::set_var("DAOZAI_DATA_DIR",&blocked);
    h.app.quota_tick();std::env::set_var("DAOZAI_DATA_DIR",original);
    assert_eq!(h.app.quota.config,saved);assert!(h.app.quota.note.contains("配置目錄"));assert_eq!(h.app.quota.rows["kiro"].as_ref().unwrap().windows[0].amount.as_ref().unwrap().remaining,900.0);
}

#[test]
fn kiro_ring_uses_logo_for_all_point_balances_and_skins(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for skin in crate::model::Skin::ALL{for remaining in [0.0,0.01,99.9,1000.0,10000.0]{
        let mut h=H::new();h.app.settings.skin=skin;let snapshot=kiro_snapshot(remaining,10000.0);let full=format!("{} 點",snapshot.windows[0].value_text());
        h.app.quota.config.accounts.push(crate::quota::Account{id:"kiro".into(),provider:crate::quota::Provider::Kiro,alias:"Kiro".into(),source:"kiro.json".into(),slot:None,enabled:true,pinned:true});h.app.quota.config.selected="kiro".into();
        h.app.quota_event(crate::quota::Event{id:"kiro".into(),result:Ok(snapshot)});h.app.show_page(6);h.hover(H::local(300.0,60.0),0.7);
        let find=|wanted:&str|h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(t)if t.galley.text()==wanted=>Some(t.visual_bounding_rect()),_=>None}).unwrap_or_else(||panic!("缺少文字 {wanted}"));
        let center=H::local(103.0,105.0);
        let logo=h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Mesh(mesh)=>{let rect=mesh.calc_bounds();(rect.center().distance(center)<0.8&&(30.0..36.0).contains(&rect.width())).then_some(rect)},_=>None}).expect("圓環中心應顯示 Kiro 標識");
        for corner in [logo.left_top(),logo.right_top(),logo.left_bottom(),logo.right_bottom()]{assert!(corner.distance(center)<27.5,"{skin:?} 的標識應位於圓環內");}
        assert!(!find("月度額度剩餘").intersects(find(&full)),"{skin:?} {remaining} 點的標籤重疊");
    }}
}

#[test]
fn kiro_logo_and_point_details_fit_the_card(){
    use crate::quota::{Amount,Event,Provider,Snapshot,Window};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.quota.config.accounts.push(crate::quota::Account{id:"kiro".into(),provider:Provider::Kiro,alias:"Kiro".into(),source:"kiro.json".into(),slot:None,enabled:true,pinned:true});
    h.app.quota.config.selected="kiro".into();
    let reset=Some((chrono::Local::now()+chrono::Duration::days(20)).timestamp());
    h.app.quota_event(Event{id:"kiro".into(),result:Ok(Snapshot{kiro:kiro_snapshot(904.09,1000.0).kiro,plan:"KIRO PRO".into(),at:chrono::Utc::now().timestamp(),
        windows:vec![Window{label:"月度額度".into(),seconds:None,remaining:90.4,reset,amount:Some(Amount::new(904.09,1000.0))}],..Default::default()})});
    h.app.show_page(6);h.hover(H::local(300.0,60.0),1.0);
    let center=H::local(103.0,105.0);
    let inner=h.shapes.iter().filter_map(|shape|match &shape.shape{
        egui::Shape::Circle(circle) if circle.center.distance(center)<0.5 && circle.radius>15.0 && circle.stroke.width>0.0=>Some(circle.radius-circle.stroke.width/2.0),
        egui::Shape::Path(path) if path.points.len()>2 && path.stroke.width>0.0=>{let radius=path.points[0].distance(center);((15.0..35.0).contains(&radius) && path.points.iter().all(|point|(point.distance(center)-radius).abs()<0.5)).then_some(radius-path.stroke.width/2.0)},
        _=>None}).reduce(f32::min).unwrap_or(35.0);
    let text=|wanted:&str|h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()==wanted=>Some(text.visual_bounding_rect()),_=>None});
    let logo=h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Mesh(mesh)=>{let rect=mesh.calc_bounds();(rect.center().distance(center)<0.8&&(30.0..36.0).contains(&rect.width())).then_some(rect)},_=>None}).expect("圓環中心應顯示 Kiro 標識");
    for corner in [logo.left_top(),logo.right_top(),logo.left_bottom(),logo.right_bottom()]{assert!(corner.distance(center)+0.5<inner,"標識應位於用量環內");}
    let center_area=Rect::from_center_size(center,Vec2::splat(40.0));
    assert!(!h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if center_area.intersects(t.visual_bounding_rect()))),"圓環中心應只顯示 Kiro 標識");
    let label=text("月度額度剩餘").expect("週期說明");let value=text("904.1 / 1000 點").expect("剩餘量與總額");
    assert!(!label.intersects(value),"說明與數值不得重疊");
    assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="Kiro · KIRO PRO")),"套餐名應顯示");
    if let Some(dest)=std::env::var_os("DAOZAI_KIRO_PREVIEW_DIR"){
        let mut raster=crate::preview_raster::Raster::new((WIN_W*2.0) as usize,(WIN_H*2.0) as usize);
        let t=h.t+1.0/60.0;
        let env=Env{t,clock:"09:41:00".into(),today:"2026-10-31".into(),date:"10月31日 週六".into(),battery:h.power.0,charging:h.power.1,media:h.media.clone(),bt_ready:true,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))),time:Some(t),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});});
        raster.update_textures(&output.textures_delta);
        raster.buf.fill([0.09,0.11,0.18,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
        image::save_buffer(std::path::PathBuf::from(&dest).join("kiro-credits.png"),&rgba,raster.w as u32,raster.h as u32,image::ColorType::Rgba8).unwrap();
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
fn kiro_credits_follow_the_ring_on_the_resident_bar(){
    use crate::quota::{Amount,Event,Provider,Snapshot,Window};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.pet=false;
    h.app.quota.config.accounts.push(crate::quota::Account{id:"kiro".into(),provider:Provider::Kiro,alias:"Kiro".into(),source:"kiro.json".into(),slot:None,enabled:true,pinned:true});
    h.app.quota.config.selected="kiro".into();
    h.app.quota_event(Event{id:"kiro".into(),result:Ok(Snapshot{kiro:kiro_snapshot(904.09,1000.0).kiro,plan:"KIRO PRO".into(),at:chrono::Utc::now().timestamp(),
        windows:vec![Window{label:"月度額度".into(),seconds:None,remaining:90.4,reset:None,amount:Some(Amount::new(904.09,1000.0))}],..Default::default()})});
    for _ in 0..60{h.frame(vec![]);}
    assert_eq!(h.app.quota_bar_indices().len(),1,"點數的 Kiro 帳號應常駐");
    let slot=crate::bar_layout::quota_slot(0);
    let layout=h.app.resident.current();
    let extra=layout.widths[slot]-27.0;
    assert!(extra>10.0,"環右側要為點數讓出位置：{:?}",layout.widths[slot]);
    let ring=WIN_W/2.0+layout.x[slot].unwrap()-extra/2.0;
    let text=h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(t)if t.galley.text()=="904"=>Some(t.visual_bounding_rect()),_=>None}).expect("常駐欄應在環右側顯示剩餘點數");
    assert!(text.center().y<30.0,"點數應與圖示同一行");
    assert!((text.left()-ring-15.5).abs()<2.5,"點數緊跟環右側：ring={ring} text={:?}",text);
    h.click(text.center());
    assert_eq!(h.app.page(),6,"點數字也能開啟用量頁");
}

#[test]
#[ignore="按需匯出 Kiro 積分池、萬元額套餐及設定明細"]
fn kiro_credit_pools_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_KIRO_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for(skin,name)in [(crate::model::Skin::Classic,"dark"),(crate::model::Skin::GlassLight,"light")]{for pools in [false,true]{
        let mut h=H::new();h.app.settings.skin=skin;let mut snapshot=kiro_snapshot(10000.0,10000.0);
        if pools{snapshot=crate::quota::parse(crate::quota::Provider::Kiro,&serde_json::json!({"userInfo":{"userId":"ui-kiro-user"},"subscriptionInfo":{"subscriptionTitle":"KIRO PRO"},"usageBreakdownList":[{"resourceType":"CREDIT","usageLimitWithPrecision":1000,"currentUsageWithPrecision":1000,"nextDateReset":1796083200_i64,"freeTrialInfo":{"freeTrialStatus":"ACTIVE","usageLimit":500,"currentUsage":100,"freeTrialExpiry":1827619200_i64},"bonuses":[{"status":"ACTIVE","displayName":"加購點數","usageLimit":125,"currentUsage":25,"expiresAt":1827619200_i64}]}]})).unwrap();}
        let id="kiro".to_owned();h.app.quota.config.accounts.push(crate::quota::Account{id:id.clone(),provider:crate::quota::Provider::Kiro,alias:"Kiro".into(),source:"kiro.json".into(),slot:None,enabled:true,pinned:true});h.app.quota.config.selected=id.clone();h.app.quota_event(crate::quota::Event{id,result:Ok(snapshot)});h.app.show_page(6);
        let mut raster=feature_raster();shoot_feature(&mut h,&mut raster,&dest,&format!("kiro-{name}-{}",if pools{"pools"}else{"power"}),75,Some(H::local(300.0,60.0)),"2026-11-11");
        if pools{
            h.app.settings.settings_theme=if name=="light"{crate::settings_theme::SettingsTheme::Light}else{crate::settings_theme::SettingsTheme::Dark};h.app.settings_tab=11;h.app.open_settings();h.ctx.set_embed_viewports(false);
            let size=egui::vec2(650.0,650.0);let ppp=1.5;let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};*capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));});
            let mut raster=crate::preview_raster::Raster::new(975,975);
            for frame in 0..6{let root=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
                if frame==5{raster.buf.fill([0.1,0.1,0.1,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("kiro-{name}-settings.png")),&rgba,975,975,image::ColorType::Rgba8).unwrap();}raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
        }
    }}
}

#[test]
#[ignore = "按需匯出 Kiro 常駐欄預覽"]
fn kiro_resident_bar_preview(){
    use crate::quota::{Amount,Event,Provider,Snapshot,Window};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_KIRO_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let mut h=H::new();h.app.settings.pet=false;h.app.settings.show_battery=true;h.quota_accounts(1);
    h.app.quota.config.accounts.push(crate::quota::Account{id:"kiro".into(),provider:Provider::Kiro,alias:"Kiro".into(),source:"kiro.json".into(),slot:None,enabled:true,pinned:true});
    h.app.quota_event(Event{id:"kiro".into(),result:Ok(Snapshot{kiro:kiro_snapshot(904.09,1000.0).kiro,plan:"KIRO PRO".into(),at:chrono::Utc::now().timestamp(),
        windows:vec![Window{label:"月度額度".into(),seconds:None,remaining:90.4,reset:None,amount:Some(Amount::new(904.09,1000.0))}],..Default::default()})});
    let wb=h.app.quota.config.connect_workbuddy("preview".into(),"WorkBuddy".into());
    h.app.quota_event(Event{id:wb,result:crate::quota::parse(Provider::WorkBuddy,&serde_json::json!({"code":0,"data":{"Packages":[{"CycleRemainCapacity":"1970"}],"CreditGroups":[{"Kind":"plan_base","CycleRemainCapacity":"1970"}]}}))});
    let mut raster=crate::preview_raster::Raster::new(1280,380);
    for frame in 0..90{
        let t=frame as f64/60.0;let env=Env{t,clock:"12:17:00".into(),battery:Some(80),charging:true,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,190.0))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});
        raster.update_textures(&output.textures_delta);
        if frame==89{
            raster.buf.fill([0.09,0.11,0.18,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dest.join("kiro-bar.png"),&rgba,1280,380,image::ColorType::Rgba8).unwrap();
        }
    }
}

#[test]
#[ignore="按需匯出 Kiro 模擬資料與連線狀態介面，不讀取真實帳號"]
fn kiro_simulated_states_gallery(){
    use crate::quota::{Account,Event,Provider};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_KIRO_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let at=chrono::DateTime::parse_from_rfc3339("2026-11-11T01:41:00Z").unwrap().timestamp();
    let expires=chrono::DateTime::parse_from_rfc3339("2027-11-01T00:00:00Z").unwrap().timestamp();
    for(skin,theme)in [(crate::model::Skin::Classic,"dark"),(crate::model::Skin::GlassLight,"light")]{
        for scene in ["monthly","pools","overage","small","power","loading","expired","stale","details","connecting","cancelled","identity"]{
            let mut h=H::new();h.app.settings.skin=skin;h.app.settings.pet=false;
            let mut value=serde_json::json!({"userInfo":{"userId":"simulation-user-a"},"subscriptionInfo":{"subscriptionTitle":"KIRO PRO"},"usageBreakdownList":[{"resourceType":"CREDIT","usageLimitWithPrecision":1000.0,"currentUsageWithPrecision":95.91,"nextDateReset":1796083200_i64}]});
            if matches!(scene,"pools"|"details"){
                value["usageBreakdownList"][0]["currentUsageWithPrecision"]=serde_json::json!(1000.0);
                value["usageBreakdownList"][0]["freeTrialInfo"]=serde_json::json!({"freeTrialStatus":"ACTIVE","usageLimit":500,"currentUsage":100,"freeTrialExpiry":expires});
                value["usageBreakdownList"][0]["bonuses"]=serde_json::json!([{"status":"ACTIVE","displayName":"加購點數","usageLimit":125,"currentUsage":25,"expiresAt":expires}]);
            }else if scene=="overage"{
                value["usageBreakdownList"][0]["currentUsageWithPrecision"]=serde_json::json!(1080.0);
                value["usageBreakdownList"][0]["currentOveragesWithPrecision"]=serde_json::json!(80.0);
                value["usageBreakdownList"][0]["overageCapWithPrecision"]=serde_json::json!(200.0);
                value["overageConfiguration"]=serde_json::json!({"overageStatus":"ENABLED"});
            }else if scene=="small"{value["usageBreakdownList"][0]["currentUsageWithPrecision"]=serde_json::json!(999.99);}
            else if scene=="power"{value["subscriptionInfo"]["subscriptionTitle"]=serde_json::json!("KIRO POWER");value["usageBreakdownList"][0]["usageLimitWithPrecision"]=serde_json::json!(10000.0);value["usageBreakdownList"][0]["currentUsageWithPrecision"]=serde_json::json!(0.0);}
            let mut snapshot=crate::quota::parse(Provider::Kiro,&value).unwrap();snapshot.at=at;
            let id="kiro-simulation".to_owned();let owner=snapshot.kiro.as_ref().unwrap().owner.clone();
            h.app.quota.config.accounts.push(Account{id:id.clone(),provider:Provider::Kiro,alias:"Kiro".into(),source:"simulation-kiro.json".into(),slot:Some(owner),enabled:true,pinned:true});h.app.quota.config.selected=id.clone();
            if !matches!(scene,"loading"|"expired"|"connecting"|"cancelled"){
                h.app.quota_event(Event{id:id.clone(),result:Ok(snapshot)});
            }
            match scene{
                "loading"=>{h.app.quota.busy.insert(id.clone());},
                "expired"=>h.app.quota_event(Event{id:id.clone(),result:Err("Kiro 登入已過期，請開啟 Kiro 重新登入後重新整理用量".into())}),
                "stale"=>{h.app.quota_event(Event{id:id.clone(),result:Err("無法連線 Kiro 用量服務，請檢查網路或代理".into())});assert_eq!(h.app.quota.rows[&id].as_ref().unwrap().at,at);assert!(h.app.quota.stale.contains_key(&id));},
                "identity"=>{
                    let account=h.app.quota.config.accounts[0].clone();let mut other=value.clone();other["userInfo"]["userId"]=serde_json::json!("simulation-user-b");let other=crate::quota::parse(Provider::Kiro,&other).unwrap();
                    let error=crate::quota::kiro::check_owner(&account,&other).unwrap_err();h.app.quota_event(Event{id:id.clone(),result:Err(error)});
                    assert_eq!(h.app.quota.rows[&id].as_ref().unwrap().kiro.as_ref().unwrap().owner,account.slot.unwrap());
                },
                _=>{}
            }
            let settings=matches!(scene,"details"|"connecting"|"cancelled"|"identity");
            if settings{
                h.app.settings.settings_theme=if theme=="light"{crate::settings_theme::SettingsTheme::Light}else{crate::settings_theme::SettingsTheme::Dark};h.app.settings_tab=11;h.app.open_settings();h.ctx.set_embed_viewports(false);
                let _pending=if matches!(scene,"connecting"|"cancelled"){
                    h.app.quota.config.accounts.clear();h.app.quota.provider=Provider::Kiro;h.app.quota.alias="個人帳號".into();h.app.quota.connect_open=true;
                    let(mut connect,tx)=crate::quota::kiro::Connect::pending(None);if scene=="cancelled"{connect.cancel();}h.app.quota.kiro_connecting=Some(connect);Some(tx)
                }else{None};
                let size=egui::vec2(690.0,620.0);let ppp=1.5;let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
                egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};*capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));});
                let mut raster=crate::preview_raster::Raster::new(1035,930);
                for frame in 0..8{
                    let root=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
                    if frame==7{
                        if scene=="identity"{let count=child.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t.galley.text().matches("月度額度").count()),_=>None}).sum::<usize>();assert_eq!(count,1,"普通套餐只顯示一行月度額度");}
                        raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{theme}-{scene}.png")),&rgba,1035,930,image::ColorType::Rgba8).unwrap();}
                    raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
                }
            }else{
                h.app.show_page(6);let mut raster=feature_raster();
                for frame in 0..75{
                    h.t+=1.0/60.0;let cursor=Some(H::local(300.0,60.0));
                    let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-11".into(),date:"11月11日 週三".into(),cursor,bt_ready:true,..Default::default()};
                    let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),events:vec![egui::Event::PointerMoved(cursor.unwrap())],viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                    let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);});});raster.update_textures(&output.textures_delta);
                    if frame==74{
                        let texts=output.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t.galley.text()),_=>None}).collect::<Vec<_>>().join(" ");
                        if scene=="small"{assert!(texts.contains("0.01"));}if scene=="loading"{assert!(texts.contains("正在更新"));}if scene=="expired"{assert!(texts.contains("登入已過期"));}
                        raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                        let image=image::RgbaImage::from_raw(raster.w as u32,raster.h as u32,rgba).unwrap();image::imageops::crop_imm(&image,200,0,880,380).to_image().save(dest.join(format!("{theme}-{scene}.png"))).unwrap();
                    }raster.free_textures(&output.textures_delta);
                }
            }
        }
    }
}

#[test]
#[ignore="按需匯出 Kiro 常駐劉海島和膠囊島，不讀取真實帳號"]
fn kiro_resident_states_gallery(){
    use crate::quota::{Account,Event,Provider};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_KIRO_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for(skin,theme)in [(crate::model::Skin::Classic,"dark"),(crate::model::Skin::GlassLight,"light")]{for mixed in [false,true]{
        let mut h=H::new();h.app.settings.skin=skin;h.app.settings.pet=false;h.app.settings.show_battery=true;
        h.app.settings.island_shape=if mixed{crate::model::IslandShape::Pill}else{crate::model::IslandShape::Notch};
        if mixed{h.quota_accounts(1);}
        let id="kiro-resident".to_owned();let snapshot=kiro_snapshot(904.09,1000.0);
        h.app.quota.config.accounts.push(Account{id:id.clone(),provider:Provider::Kiro,alias:"Kiro".into(),source:"simulation-kiro.json".into(),slot:Some(snapshot.kiro.as_ref().unwrap().owner.clone()),enabled:true,pinned:true});
        h.app.quota_event(Event{id:id.clone(),result:Ok(snapshot)});
        if mixed{let wb=h.app.quota.config.connect_workbuddy("preview".into(),"WorkBuddy".into());h.app.quota_event(Event{id:wb,result:crate::quota::parse(Provider::WorkBuddy,&serde_json::json!({"code":0,"data":{"Packages":[{"CycleRemainCapacity":"1970"}],"CreditGroups":[{"Kind":"plan_base","CycleRemainCapacity":"1970"}]}}))});}
        h.app.quota.config.selected=id.clone();let mut raster=crate::preview_raster::Raster::new(1280,380);
        for frame in 0..90{
            let t=frame as f64/60.0;let env=Env{t,clock:"12:17:00".into(),today:"2026-11-11".into(),date:"11月11日 週三".into(),battery:Some(80),charging:true,..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,190.0))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});raster.update_textures(&output.textures_delta);
            if frame==89{
                assert_eq!(h.app.quota_bar_indices().len(),if mixed{3}else{1});
                assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="904")),"常駐欄應顯示剩餘點數");
                raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw(1280,380,rgba).unwrap();let width=(h.app.current_width()*2.0+72.0).ceil().clamp(300.0,1280.0) as u32;
                image::imageops::crop_imm(&image,(1280-width)/2,0,width,130).to_image().save(dest.join(format!("{theme}-{}.png",if mixed{"residentmixed"}else{"resident"}))).unwrap();
            }raster.free_textures(&output.textures_delta);
        }
    }}
}

#[test]
fn quota_update_status_is_visible_for_every_provider_and_keeps_failed_data(){
    use crate::quota::{Provider,Event,Snapshot,Window};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let at=chrono::Utc::now().timestamp()-60;
    let time=chrono::DateTime::from_timestamp(at,0).unwrap().with_timezone(&chrono::Local).format("%H:%M").to_string();
    for provider in Provider::ALL{
        let mut h=H::new();h.quota_accounts(1);h.app.quota.config.accounts[0].provider=provider;
        let mut snapshot=match provider{
            Provider::Kiro=>kiro_snapshot(720.0,1000.0),
            Provider::DeepSeek=>crate::quota::parse(provider,&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"4.18","granted_balance":"0","topped_up_balance":"4.18"}]})).unwrap(),
            Provider::WorkBuddy=>crate::quota::parse(provider,&serde_json::json!({"code":0,"data":{"Packages":[{"CycleRemainCapacity":"2000"}]}})).unwrap(),
            _=>Snapshot{windows:vec![Window{label:"5 小時".into(),seconds:Some(18000),remaining:72.0,reset:None,amount:None},Window{label:"本週".into(),seconds:Some(604800),remaining:44.0,reset:None,amount:None}],..Default::default()},
        };
        snapshot.at=at;h.app.quota_event(Event{id:"preview-0".into(),result:Ok(snapshot)});
        h.app.show_page(6);
        if let Some(dest)=std::env::var_os("DAOZAI_QUOTA_STATUS_PREVIEW_DIR"){
            let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();let mut raster=feature_raster();
            shoot_feature(&mut h,&mut raster,&dest,&format!("quota-{}",provider.name()),90,Some(H::local(300.0,60.0)),"2026-11-07");
        }
        h.hover(H::local(300.0,60.0),1.0);
        assert_eq!(h.app.expanded_height(),176.0,"{provider:?} 用量頁應採用同寬加高的佈局");
        assert!((h.app.current_width()-PW).abs()<0.1,"{provider:?} 用量頁應保持原有寬度：{}",h.app.current_width());
        let expected=if provider==Provider::Claude{format!("本地記錄 · {time}")}else if provider==Provider::DeepSeek{format!("CNY · {time} 更新")}else{format!("{time} 更新")};
        let status=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==expected=>Some(t.visual_bounding_rect()),_=>None}).unwrap_or_else(||panic!("{provider:?} 缺少更新時間：{expected}"));
        assert!(status.bottom()+6.0<=H::local(0.0,h.app.expanded_height()-9.2).y,"{provider:?} 更新時間 {status:?} 應與分頁圓點留出間距");
        for shape in &h.shapes{if let egui::Shape::Text(text)=&shape.shape{
            if !text.galley.text().is_empty()&&text.galley.text()!=expected&&text.visual_bounding_rect().left()>=H::local(125.0,0.0).x{
                assert!(shape.clip_rect.contains(text.visual_bounding_rect().center()),"{provider:?} 的 {} 應完整顯示在面板內",text.galley.text());
                assert!(status.top()-text.visual_bounding_rect().bottom()>=8.0,"{provider:?} 的更新時間 {status:?} 應與 {} {:?} 留出獨立行距",text.galley.text(),text.visual_bounding_rect());
            }
        }}
        h.app.quota.busy.insert("preview-0".into());h.frame(vec![]);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(&time)&&t.galley.text().contains(if provider==Provider::Claude{"讀取中"}else{"正在更新"}))),"重新整理中仍須保留已有記錄時間");
        h.app.quota_event(Event{id:"preview-0".into(),result:Err("連線超時".into())});h.frame(vec![]);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==format!("上次記錄 · {time}"))),"{provider:?} 重新整理失敗後應明確舊資料時間");
        assert_eq!(h.app.quota.rows["preview-0"].as_ref().unwrap().at,at,"重新整理失敗不能重新整理記錄時間");
    }
}

#[test]
fn quota_page_navigation_tracks_the_taller_panel(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.quota_accounts(1);
    h.app.show_page(6);h.hover(H::local(300.0,60.0),0.8);
    assert!((h.hit.height()-178.0).abs()<0.2,"用量頁新增的底部空間應屬於命中區域");
    let dot=h.shapes.iter().find_map(|shape|match &shape.shape{
        egui::Shape::Rect(rect) if (rect.rect.width()-4.0).abs()<0.1&&(rect.rect.height()-4.0).abs()<0.1&&(rect.rect.center().y-168.8).abs()<0.1=>Some(rect.rect.center()),_=>None,
    }).expect("分頁圓點應隨加高的面板下移");
    h.click(dot);assert_ne!(h.app.page,6,"新增底部區域中的分頁應可點選");
    h.app.show_page(6);h.hover(H::local(300.0,60.0),0.8);
    h.click(H::local(21.0,88.0));assert_eq!(h.app.page,5);
    h.hover(H::local(300.0,60.0),0.8);assert_eq!(h.app.expanded_height(),PH);
    h.click(H::local(398.0,68.0));assert_eq!(h.app.page,6);
    h.hover(H::local(300.0,60.0),0.8);
    h.click(H::local(398.0,88.0));assert_eq!(h.app.page,crate::app::CALENDAR_PAGE);
}

#[test]
fn quota_update_status_marks_previous_dates_and_never_shows_epoch_time(){
    use crate::quota::{Event,Snapshot,Window};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.quota_accounts(1);
    let at=chrono::Local::now()-chrono::Duration::days(1);
    let snapshot=Snapshot{at:at.timestamp(),windows:vec![Window{label:"本週".into(),seconds:Some(604800),remaining:44.0,reset:None,amount:None}],..Default::default()};
    h.app.quota_event(Event{id:"preview-0".into(),result:Ok(snapshot.clone())});h.app.show_page(6);h.hover(H::local(300.0,60.0),0.6);
    let expected=format!("{} 更新",at.format("%m/%d %H:%M"));
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==expected)),"跨天記錄應包含日期");
    h.app.quota_event(Event{id:"preview-0".into(),result:Ok(Snapshot{at:0,..snapshot})});h.frame(vec![]);
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("1970")||t.galley.text().ends_with(" 更新"))),"缺少時間時不能偽造更新時間");
}

#[test]
fn claude_local_quota_shows_the_record_time_and_does_not_invent_resets(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.quota_accounts(1);h.app.quota.config.accounts[0].provider=crate::quota::Provider::Claude;
    let at=chrono::Utc::now().timestamp()-60;h.app.quota_event(crate::quota::Event{id:"preview-0".into(),result:Ok(crate::quota::Snapshot{at,windows:vec![
        crate::quota::Window{label:"5 小時".into(),seconds:Some(18000),remaining:89.0,reset:None,amount:None},crate::quota::Window{label:"本週".into(),seconds:Some(604800),remaining:24.0,reset:None,amount:None}],..Default::default()})});
    h.app.show_page(6);let mut raster=feature_raster();let mut shapes=Vec::new();
    for _ in 0..90{
        h.t+=1.0/60.0;let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let env=Env{t:h.t,clock:"20:30:00".into(),date:"11月6日 週五".into(),today:"2026-11-06".into(),cursor:Some(Pos2::new(WIN_W/2.0,12.0)),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);});});raster.update_textures(&output.textures_delta);shapes=output.shapes;raster.free_textures(&output.textures_delta);
    }
    let texts:Vec<_>=shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t),_=>None}).collect();
    let label=texts.iter().find(|t|t.galley.text()=="Claude · 本地記錄").expect("應明確標註本地來源");
    let time=chrono::DateTime::from_timestamp(at,0).unwrap().with_timezone(&chrono::Local).format("%H:%M").to_string();
    let timestamp=texts.iter().find(|t|t.galley.text()==format!("本地記錄 · {time}")).expect("應顯示官方快取的實際記錄時間");assert!(!label.visual_bounding_rect().intersects(timestamp.visual_bounding_rect()));assert_eq!(texts.iter().filter(|t|t.galley.text()=="重置時間未知").count(),2);
    if let Some(path)=std::env::var_os("DAOZAI_CLAUDE_LOCAL_PREVIEW"){
        raster.buf.fill([0.15,0.15,0.15,1.0]);raster.draw(&h.ctx.tessellate(shapes,2.0),2.0);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw((crate::app::CANVAS_W*2.0) as u32,(WIN_H*2.0) as u32,rgba).unwrap();image::imageops::crop_imm(&image,200,0,880,350).to_image().save(path).unwrap();
    }
}

#[test]
fn quota_accounts_switch_independently_and_hide_with_the_page() {
    use crate::quota::{Account,Provider};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for id in ["personal","work"] {h.app.quota.config.accounts.push(Account{id:id.into(),provider:Provider::Codex,alias:id.into(),source:format!("{id}.json").into(),slot:None,enabled:true,pinned:true});}
    h.app.quota.config.selected="personal".into();
    h.app.quota_event(crate::quota::Event{id:"personal".into(),result:crate::quota::parse(Provider::Codex,&serde_json::json!({"rate_limit":{"secondary_window":{"used_percent":20,"limit_window_seconds":604800}}}))});
    h.app.quota_event(crate::quota::Event{id:"work".into(),result:Err("登入已失效".into())});
    assert_eq!(h.app.quota.rows["personal"].as_ref().unwrap().windows[0].remaining,80.0);
    assert!(h.app.quota.rows["work"].is_err());assert_eq!(h.app.quota_bar_indices().len(),2);
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.show_page(6);h.hover(H::local(300.0,60.0),0.7);
    h.click(H::local(202.0,39.0));assert_eq!(h.app.quota.config.selected,"work");
    assert_eq!(crate::quota::Config::load().selected,"work");
    h.app.set_page_visible(6,false);assert!(h.app.quota_bar_indices().is_empty());assert_ne!(h.app.page,6);
    h.app.quota.config.accounts.retain(|a|a.id!="work");h.app.quota_event(crate::quota::Event{id:"work".into(),result:Ok(Default::default())});
    assert!(h.app.quota.rows["work"].is_err(),"移除帳號後不得接收晚到的用量資料");
}

#[test]
fn page_visibility_is_saved_and_sets_the_starting_page() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    assert_eq!(h.app.settings.visible_pages,[true;9]);
    assert!(h.app.settings.show_battery);
    h.app.settings.show_battery=false;
    h.app.set_page_visible(0,false);
    h.app.set_page_visible(3,false);
    h.app.settings.save();
    let settings=crate::model::Settings::load();
    assert!(!settings.show_battery);
    assert_eq!(settings.visible_pages,[false,true,true,false,true,true,true,true,true]);
    let restarted=App::new(&h.ctx,Options{fetch_lyrics:false,pet_idle:1e9});
    assert_eq!(restarted.page(),1);
    assert_eq!(restarted.settings.visible_pages,settings.visible_pages);
    assert!(!restarted.settings.show_battery);
    std::fs::write(crate::model::config_dir().join("settings.txt"),"visible_pages=000000\n").unwrap();
    assert_eq!(crate::model::Settings::load().visible_pages,[true,false,false,false,false,false,false,false,false]);
    std::fs::write(crate::model::config_dir().join("settings.txt"),"visible_pages=1000000\n").unwrap();
    assert_eq!(crate::model::Settings::load().visible_pages,[true,false,false,false,false,false,false,true,true],"舊版頁面開關應保留，並開啟新增的日程頁與天氣頁");
    std::fs::write(crate::model::config_dir().join("settings.txt"),"visible_pages=invalid\n").unwrap();
    assert_eq!(crate::model::Settings::load().visible_pages,[true;9]);
}

#[test]
fn battery_module_hides_power_in_collapsed_and_expanded_headers() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for expanded in [false,true] {
        let mut h=H::new();
        h.app.settings.pet=false;
        h.app.settings.show_battery=false;
        if expanded {h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);} else {h.frame(vec![]);}
        let baseline=h.shapes.clone();
        h.power=(Some(80),true);
        h.t-=1.0/60.0;h.frame(vec![]);
        assert!(h.shapes==baseline,"關閉電量後，本機電池狀態不應影響介面佈局");
        h.app.settings.show_battery=true;
        h.t-=1.0/60.0;h.frame(vec![]);
        assert!(h.shapes!=baseline,"開啟後應顯示電量");
        h.power=(None,true);
        h.t-=1.0/60.0;h.frame(vec![]);
        assert!(h.shapes==baseline,"沒有電池的裝置應自動隱藏電量並釋放佔位");
    }
}

#[test]
fn battery_module_can_disable_only_the_power_warning() {
    use crate::extras::PetState;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.show_battery=false;
    h.power=(Some(10),false);h.frame(vec![]);
    assert_eq!(h.app.pet_state(),PetState::Awake);
    h.network=Some(false);h.frame(vec![]);
    assert_eq!(h.app.pet_state(),PetState::Worried);
}

#[test]
fn page_visibility_skips_hidden_pages_in_every_navigation_control() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    for page in [1,3,4,6,7,8] {h.app.set_page_visible(page,false);}
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    for expected in [2,5,0] {
        h.click(H::local(398.0,68.0));
        assert_eq!(h.app.page,expected);
        h.hover(H::local(300.0,60.0),0.7);
    }
    h.click(H::local(PW/2.0+6.0,132.8));
    assert_eq!(h.app.page,2,"底部圓點應僅包含啟用的頁面");
    h.hover(H::local(300.0,60.0),0.7);
    h.frame(vec![egui::Event::Key{key:egui::Key::ArrowRight,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!(h.app.page,5);
    h.frame(vec![egui::Event::Key{key:egui::Key::ArrowRight,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
    h.hover(H::local(300.0,60.0),0.7);
    h.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:Vec2::new(0.0,40.0),modifiers:Default::default()}]);
    assert_eq!(h.app.page,2,"滾輪應跳過隱藏頁面");
    h.click(H::local(21.0,68.0));
    assert_eq!(h.app.page,0);
}

#[test]
fn page_visibility_moves_away_from_hidden_page_and_keeps_one_page() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.go(4,1.0);
    h.app.adding=true;
    h.app.set_page_visible(4,false);
    assert_eq!(h.app.page,0);
    assert!(!h.app.is_editing());
    for page in [0,1,3,5,6,7,8] {h.app.set_page_visible(page,false);}
    assert_eq!(h.app.page,2);
    h.app.set_page_visible(2,false);
    assert!(h.app.settings.visible_pages[2],"不能關閉最後一個頁面");
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.click(H::local(398.0,68.0));
    h.click(H::local(21.0,68.0));
    assert_eq!(h.app.page,2);
    h.app.set_page_visible(5,true);
    h.hover(H::local(300.0,60.0),0.7);
    h.click(H::local(398.0,68.0));
    assert_eq!(h.app.page,5,"重新開啟的頁面應恢復導航");
}

#[test]
fn page_visibility_does_not_reopen_hidden_pages_through_shortcuts_or_drops() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.set_page_visible(5,false);
    h.app.show_page(5);
    assert_eq!(h.app.page,0);
    assert!(h.app.settings_open);
    assert_eq!(h.app.settings_tab,7,"隱藏頁面的入口應進入對應設定");
    h.app.set_page_visible(3,false);
    let file=crate::model::config_dir().join("shelf-hidden.txt");
    std::fs::create_dir_all(crate::model::config_dir()).unwrap();
    std::fs::write(&file,"test").unwrap();
    h.app.on_drop(vec![file]);
    assert_eq!(h.app.page,0);
    assert!(h.app.shelf.items.is_empty());
}

#[test]
fn alphabetic_mail_codes_reach_the_card_without_changing_case() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (kind,value) in ["text/plain","text/html"].into_iter().flat_map(|kind|["AbCdEf","WWWWWWWW"].map(|value|(kind,value))) {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        let body=if kind=="text/html"{format!("<p>Your verification code:</p><p><b>{value}</b></p>")}else{format!("Your verification code:\n{value}")};
        let raw=format!("From: Example <security@example.invalid>\r\nSubject: Your verification code\r\nContent-Type: {kind}; charset=utf-8\r\n\r\n{body}");
        let parsed=crate::mail::parse_message(raw.as_bytes());
        let code=crate::mail::code_of(&parsed);
        assert_eq!(code.as_deref(),Some(value));
        h.app.mail_event(crate::mail::Event::Mail{new:true,item:crate::mail::Item{html:String::new().into(),received_unix:None,account:"test@qq.com".into(),from:"Example".into(),subject:parsed.subject.clone(),time:"09:41".into(),code,summary:crate::mail::body_of(&parsed).into(),links:vec![],meeting:None,uid:43}});
        h.app.show_page(5);h.hover(H::local(300.0,70.0),1.0);
        let text=h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.text()==value=>Some(t),_=>None}).expect("字母驗證碼應出現在大卡上");
        assert!(text.galley.size().x<=120.5,"最長字母碼不能擠入郵件列表：{}",text.galley.size().x);
        if value=="AbCdEf"{assert!(text.galley.job.sections.iter().any(|s|s.format.font_id.size>=23.0));}
        h.cmds.clear();h.click(H::local(60.0,70.0));
        assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Copy(copied) if copied==value)));
        h.t+=61.0;h.frame(vec![]);
        assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==value)));
    }
}

#[test]
fn regression_microsoft_traditional_mail_creates_code_card() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for kind in ["text/plain","text/html"] {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        let body="我們收到您要申請一次性驗證碼來與 Microsoft 帳戶搭配使用。\n您的一次性驗證碼為：431872\n僅在官方網站或應用程式上輸入此程式碼。不要與任何人分享。";
        let body=if kind=="text/html" {format!("<p>{}</p>",body.replace('\n',"</p><p>"))}else{body.into()};
        let raw=format!("From: Microsoft <account-security@example.invalid>\r\nSubject: 您的一次性驗證碼\r\nMIME-Version: 1.0\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}");
        let parsed=crate::mail::parse_message(raw.as_bytes());
        let code=crate::mail::code_of(&parsed);
        assert_eq!(code.as_deref(),Some("431872"),"{kind} 郵件應提取出驗證碼");
        h.app.mail_event(crate::mail::Event::Mail{new:true,item:crate::mail::Item{html:String::new().into(),received_unix:None,account:"test@qq.com".into(),from:parsed.from.clone(),subject:parsed.subject.clone(),time:"08:59".into(),code,summary:crate::mail::body_of(&parsed).into(),links:vec![],meeting:None,uid:42}});
        h.app.show_page(5);h.hover(H::local(300.0,70.0),1.0);
        assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="431872"&&t.galley.job.sections.iter().any(|s|s.format.font_id.size>=23.0))),"新收到的繁體驗證碼應出現在大卡里");
        h.cmds.clear();h.click(H::local(60.0,70.0));
        assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Copy(value) if value=="431872")),"大卡應支援點選複製");
        h.t+=61.0;h.frame(vec![]);
        assert!(!h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="431872")),"繁體驗證碼同樣應按隱私設定自動隱藏");
    }
}

#[test]
fn mail_page_shows_code_and_copies_on_click() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.mail.accounts = vec![crate::mail::Account {
        custom: None, email: "a@qq.com".into(),
        provider: crate::mail::Provider::Qq,
        enabled: true,
    }];
    h.app.mail_event(crate::mail::Event::Mail {
        item: crate::mail::Item {
            received_unix: None, account: "a@qq.com".into(),
            from: "GitHub".into(),
            subject: "verify".into(),
            time: "09:41".into(),
            code: Some("482913".into()),
            html: String::new().into(), summary: "Enter 482913 to finish signing in.".into(),
            links: vec![],
            meeting: None,
            uid: 42,
        },
        new: true,
    });
    assert!(h.app.notification_bell_until > 0.0, "新郵件應點亮鈴鐺");
    assert!(h.app.alerts.iter().any(|a|a.title=="新郵件"&&a.page==5),"新郵件應進入通知佇列，不能只有收起時才可見的鈴鐺");
    h.app.show_page(5);
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 0.6);
    h.hover(H::local(300.0, 60.0), 0.4);
    h.click(H::local(60.0, 70.0));
    assert!(
        h.cmds.iter().any(|c| matches!(c, crate::app::Cmd::Copy(t) if t == "482913")),
        "點驗證碼應複製: {:?}",
        h.cmds
    );
}

#[test]
fn mail_row_opens_summary_and_back_returns() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.mail.accounts = vec![crate::mail::Account {
        custom: None, email: "a@qq.com".into(),
        provider: crate::mail::Provider::Qq,
        enabled: true,
    }];
    h.app.mail_event(crate::mail::Event::Mail {
        item: crate::mail::Item {
            received_unix: None, account: "a@qq.com".into(),
            from: "GitHub".into(),
            subject: "verify".into(),
            time: "09:41".into(),
            code: Some("482913".into()),
            html: String::new().into(), summary: "Enter 482913 to finish signing in.".into(),
            links: vec![],
            meeting: None,
            uid: 42,
        },
        new: true,
    });
    h.app.show_page(5);
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 0.6);
    h.hover(H::local(300.0, 50.0), 0.4);
    h.click(H::local(300.0, 50.0));
    assert_eq!(h.app.mail.open_uid, Some(("a@qq.com".to_string(), 42)), "點郵件行應展開摘要");
    h.hover(H::local(350.0, 48.0), 0.4);
    h.click(H::local(350.0, 48.0));
    assert_eq!(h.app.mail.open_uid, None, "點返回應回到列表");
}

#[test]
fn empty_todo_draft_can_be_cancelled_by_clicking_elsewhere(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for text in ["","   "]{
        let mut h=H::new();let mut r=feature_raster();h.app.todos.items=vec![crate::model::TodoItem{text:"保留任務".into(),due:None,done_on:None,reminded:false,..Default::default()}];h.app.todos.save();
        h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        assert!(h.app.adding,"點選新增後應保留空白輸入框");
        if !text.is_empty(){h.frame(vec![egui::Event::Text(text.into())]);}
        h.cmds.clear();h.click(H::local(65.0,60.0));
        assert!(!h.app.is_editing(),"空白草稿點選島內其他位置應退出");
        assert!(h.app.buf.is_empty());assert_eq!(h.app.todos.items.len(),1);assert_eq!(h.app.todos.items[0].text,"保留任務");
        assert_eq!(crate::model::TodoList::load().items.len(),1,"不能儲存空白待辦");
        h.hover(H::local(65.0,60.0),0.3);
        assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"取消後不能反覆啟用視窗");
    }
}

#[test]
fn todo_ime_first_backspace_keeps_committed_text(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
    h.frame(vec![egui::Event::Text("保留文字".into())]);
    h.frame(vec![egui::Event::Key{key:egui::Key::Backspace,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()},egui::Event::Ime(egui::ImeEvent::Enabled)]);
    assert_eq!(h.app.buf,"保留文字","開始組合輸入的同一幀，退格只能交給輸入法");
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Disabled)]);
    h.frame(vec![egui::Event::Key{key:egui::Key::Backspace,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!(h.app.buf,"保留文","結束組合輸入後普通退格仍應正常刪除");
}

#[test]
fn todo_ime_focus_change_preserves_empty_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));h.cmds.clear();
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled),egui::Event::WindowFocused(false)]);
    assert!(h.app.adding,"輸入法候選視窗造成的失焦不能取消空白待辦");
    assert!(h.app.open);
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"不能和輸入法視窗反覆爭搶焦點");
    h.app.foreground_changed();h.frame(vec![]);
    assert!(h.app.adding,"輸入法組合輸入期間的前臺切換應保留草稿");
    h.frame(vec![egui::Event::WindowFocused(true),egui::Event::Ime(egui::ImeEvent::Preedit("zhong".into()))]);
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Commit("中文".into())),egui::Event::Ime(egui::ImeEvent::Disabled)]);
    assert_eq!(h.app.buf,"中文");assert!(h.app.todos.items.is_empty());
}

#[test]
fn todo_ime_confirmation_does_not_submit_or_cancel_the_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for key in [egui::Key::Enter,egui::Key::Escape]{
        let mut h=H::new();h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        h.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled)]);
        h.frame(vec![egui::Event::Key{key,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()},egui::Event::Ime(egui::ImeEvent::Commit("中文".into())),egui::Event::Ime(egui::ImeEvent::Disabled)]);
        assert!(h.app.adding,"輸入法確認或取消候選不能同時結束待辦編輯");
        assert_eq!(h.app.buf,"中文","輸入法確認後應保留正文，按鍵 {key:?}");assert!(h.app.todos.items.is_empty());
        h.frame(vec![egui::Event::Key{key,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.todos.items.len(),1,"退出組合輸入後回車應正常儲存");assert_eq!(h.app.todos.items[0].text,"中文");
    }
}

#[test]
fn todo_ime_candidate_closing_does_not_discard_the_empty_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled)]);
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Disabled),egui::Event::WindowFocused(false)]);
    assert!(h.app.adding,"關閉輸入法候選視窗的同一幀不能取消空白草稿");
    h.frame(vec![egui::Event::WindowFocused(true)]);h.app.foreground_changed();h.frame(vec![]);
    assert!(!h.app.adding,"之後真正切換應用時仍應取消空白草稿");
}

#[test]
fn todo_ime_event_variants_preserve_mixed_text_and_resume_plain_input(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let cases=vec![
        ("無顯式開始事件",vec![vec![egui::ImeEvent::Preedit("ni".into())],vec![egui::ImeEvent::Commit("你".into()),egui::ImeEvent::Disabled]],"你"),
        ("一次提交多個字元",vec![vec![egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("にほん".into())],vec![egui::ImeEvent::Commit("日本".into()),egui::ImeEvent::Disabled]],"日本"),
        ("同幀提交後繼續組合",vec![vec![egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("하".into())],vec![egui::ImeEvent::Commit("한".into()),egui::ImeEvent::Disabled,egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("그".into())],vec![egui::ImeEvent::Commit("글".into()),egui::ImeEvent::Disabled]],"한글"),
        ("取消候選",vec![vec![egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("pin".into())],vec![egui::ImeEvent::Preedit(String::new()),egui::ImeEvent::Disabled]],""),
    ];
    for (name,frames,expected) in cases {
        let mut h=H::new();h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        h.frame(vec![egui::Event::Text("中文 ABC ".into())]);
        for events in frames {h.frame(events.into_iter().map(egui::Event::Ime).collect());}
        assert_eq!(h.app.buf,format!("中文 ABC {expected}"),"事件模式：{name}");
        assert!(h.app.adding);assert!(h.app.todos.items.is_empty());
        h.frame(vec![egui::Event::Text("x".into())]);
        h.frame(vec![egui::Event::Key{key:egui::Key::Backspace,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.buf,format!("中文 ABC {expected}"),"組合輸入結束後應恢復普通退格：{name}");
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.todos.items.len(),1,"組合輸入結束後應能儲存：{name}");
    }
}

#[test]
fn blank_todo_draft_closes_when_window_focus_is_lost(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
    h.cmds.clear();h.frame(vec![egui::Event::WindowFocused(false)]);
    assert!(!h.app.is_editing(),"點選其他軟體後空白草稿應自動退出");
    assert!(h.app.todos.items.is_empty());
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"失去焦點時不能把視窗再次啟用");
}

#[test]
fn todo_input_requests_window_focus_once_and_preserves_typed_drafts(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.cmds.clear();h.click(H::local(70.0,109.0));
    assert_eq!(h.cmds.iter().filter(|cmd|matches!(cmd,crate::app::Cmd::Focus)).count(),1,"新建時只應請求一次視窗焦點");
    h.click(H::local(280.0,46.0));assert!(h.app.adding,"點選輸入框不能取消草稿");
    h.frame(vec![egui::Event::Text("尚未儲存的內容".into())]);h.cmds.clear();
    h.click(H::local(65.0,60.0));h.frame(vec![egui::Event::WindowFocused(false)]);
    for _ in 0..20{h.frame(vec![]);}
    assert!(h.app.adding);assert_eq!(h.app.buf,"尚未儲存的內容");assert!(h.app.todos.items.is_empty());
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"已有內容失焦後應保留草稿，不能搶回焦點");
}

#[test]
fn switching_apps_cancels_only_empty_new_todos(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for text in ["","   ","保留未提交的內容"]{
        let mut h=H::new();h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        if !text.is_empty(){h.frame(vec![egui::Event::Text(text.into())]);}
        h.cmds.clear();h.app.foreground_changed();h.frame(vec![]);
        assert_eq!(h.app.adding,!text.trim().is_empty());
        assert_eq!(h.app.buf,if text.trim().is_empty(){""}else{text});
        assert!(h.app.todos.items.is_empty());
        assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)));
    }
}

#[test]
fn drink_adds_water_and_todo_can_be_added() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 1.0);
    h.app.go(2, 1.0);
    h.hover(H::local(300.0, 60.0), 0.8);
    h.click(H::local(70.0, 110.0));
    assert_eq!(h.app.water.drunk_ml, 250);
    assert_eq!(h.app.water.hours[9], 250);

    h.app.go(4, 1.0);
    h.hover(H::local(300.0, 60.0), 0.8);
    h.click(H::local(70.0, 109.0));
    assert!(h.app.adding);
    h.hover(H::local(300.0, 60.0), 0.4);
    h.frame(vec![egui::Event::Text("寫週報".into())]);
    h.frame(vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]);
    h.hover(H::local(300.0, 60.0), 0.1);
    assert_eq!(h.app.todos.items.first().map(|t| t.text.as_str()), Some("寫週報"));
    assert!(!h.app.adding);
}

#[test]
fn timer_counts_elapsed_time_after_sleep() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.frame(vec![]);
    h.app.pomo.running = true;
    h.app.pomo.remaining = 120.0;
    h.t += 60.0;
    h.frame(vec![]);
    assert!(h.app.pomo.remaining < 61.0, "掛起恢復後必須補算實際流逝時間");
}

#[test]
fn shelf_remove_keeps_original_file() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let file=crate::model::config_dir().join("保留檔案.txt");std::fs::create_dir_all(file.parent().unwrap()).unwrap();std::fs::write(&file,"內容").unwrap();
    h.app.shelf.add(&file).unwrap();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(3,1.0);h.hover(H::local(300.0,60.0),0.8);
    h.click(H::local(233.0,39.0));assert!(h.app.shelf.items.is_empty());assert!(file.exists());assert!(crate::model::Shelf::load().items.is_empty());
}

#[test]
fn shelf_duplicate_drop_reports_existing_files_and_preserves_the_current_page() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let dir=crate::model::config_dir();std::fs::create_dir_all(&dir).unwrap();
    let files:Vec<_>=(0..7).map(|i|{let path=dir.join(format!("檔案-{i}.txt"));std::fs::write(&path,"內容").unwrap();path}).collect();
    h.app.on_drop(files.clone());h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.shelf_first=0;
    h.app.on_drop(vec![files[0].clone()]);h.hover(H::local(300.0,60.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="該檔案已在置物架中")));
    h.app.on_drop(vec![files[0].clone(),files[1].clone()]);h.hover(H::local(300.0,60.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="這些檔案已在置物架中")));
    assert_eq!(h.app.shelf_first,0,"重複新增不應將列表跳到最後");
    assert_eq!(h.app.shelf.items,files);assert_eq!(crate::model::Shelf::load().items,files);
}

#[test]
fn shelf_drop_distinguishes_new_duplicate_and_missing_files() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let dir=crate::model::config_dir();std::fs::create_dir_all(&dir).unwrap();
    let old=dir.join("已新增.txt");let new=dir.join("新檔案.txt");let missing=dir.join("已移動.txt");
    for file in [&old,&new]{std::fs::write(file,"內容").unwrap();}
    h.app.on_drop(vec![old.clone()]);h.app.on_drop(vec![old.clone(),new.clone(),new.clone(),missing.clone()]);
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="已新增 1 個檔案，2 個已存在，1 個無法新增")));
    assert_eq!(h.app.shelf.items,vec![old.clone(),new]);
    std::fs::remove_file(&old).unwrap();h.app.on_drop(vec![old]);h.hover(H::local(300.0,60.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="檔案不存在或無法訪問")));
}
#[test]
fn reminders_are_visible_while_collapsed_and_can_be_disabled(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.t+=2701.0;h.frame(vec![]);assert!(h.app.notice.is_some());assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Notify(t,_)if t.contains("喝水"))));
    h.app.water.remind=false;h.app.settings.sit_remind=false;h.app.notice=None;h.cmds.clear();h.t+=2800.0;h.frame(vec![]);assert!(h.app.notice.is_none());
}
#[test]
fn water_serving_and_undo_are_consistent(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(2,1.0);h.hover(H::local(300.0,60.0),0.8);
    h.app.settings.serving_ml=350;h.click(H::local(70.0,110.0));assert_eq!(h.app.water.drunk_ml,350);assert_eq!(h.app.water.hours[9],350);
    h.click(H::local(191.0,110.0));assert_eq!(h.app.water.drunk_ml,0);assert_eq!(h.app.water.hours[9],0);
}
#[test]
fn mascot_rest_and_focus_have_distinct_states(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.opts.pet_idle=45.0;h.frame(vec![]);
    assert_eq!(h.app.pet_state(),crate::extras::PetState::Awake);
    h.t+=50.0;h.frame(vec![]);assert_eq!(h.app.pet_state(),crate::extras::PetState::Dozing);
    h.t+=140.0;h.frame(vec![]);assert_eq!(h.app.pet_state(),crate::extras::PetState::Sleeping);
    h.app.pomo.running=true;assert_eq!(h.app.pet_state(),crate::extras::PetState::Focused);
}

#[test]
fn mascot_stays_beside_the_clock_and_all_resident_components(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=false;h.power=(Some(80),true);h.quota_accounts(5);
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    h.app.pomo.running=true;h.app.pomo.remaining=1200.0;
    for _ in 0..120 {h.frame(vec![]);}let width=h.app.current_width();
    h.app.settings.pet=true;for _ in 0..120 {h.frame(vec![]);}
    assert!((h.app.current_width()-width-44.0).abs()<0.1,"島仔應有獨立常駐位置");
    assert_eq!(h.quota_centers().len(),5,"五個常駐帳號均顯示用量環");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="09:41")),"島仔與時鐘共存");
    assert!(h.shapes.iter().filter(|s|matches!(&s.shape,egui::Shape::Path(p) if p.points.len()==13 && (p.stroke.width-3.6).abs()<0.01)).count()>=2,"沒有閒置或懸停也應顯示眼睛");
}

#[test]
fn mascot_and_album_share_the_single_line_lyric_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,
        artwork:Some(std::sync::Arc::new(egui::ColorImage::new([32,32],egui::Color32::from_rgb(70,120,220)))),..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]晚風吹過了山崗\n[00:15]下一句"));
    for _ in 0..120 {h.frame(vec![]);}
    assert!((h.app.current_width()-348.0).abs()<0.1,"專輯、島仔與原有歌詞寬度一起保留");
    let texture=h.app.music_artwork.as_ref().unwrap().1.id();
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.brush.as_ref().is_some_and(|b|b.fill_texture_id==texture))),"專輯封面不能被島仔替代");
    let text=h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.text()=="晚風吹過了山崗"=>Some(t),_=>None}).unwrap();
    assert_eq!(text.galley.rows.len(),1,"保留單行歌詞");
    assert!((text.pos.x+text.galley.rect.center().x-WIN_W/2.0-24.0).abs()<1.5,"歌詞在封面和島仔右側居中");
    assert!(h.shapes.iter().filter(|s|matches!(&s.shape,egui::Shape::Path(p) if p.points.len()==13 && (2.79..=3.61).contains(&p.stroke.width))).count()>=2);
}

#[test]
fn singing_microphone_moves_relative_to_the_face_and_disappears_on_pause(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    for _ in 0..120 {h.frame(vec![]);}
    let head=|shapes:&[egui::epaint::ClippedShape]|shapes.iter().find_map(|s|match &s.shape {
        egui::Shape::Circle(circle) if (circle.radius-2.6).abs()<0.01=>Some(circle.center),_=>None,
    });
    let mut offsets=Vec::new();
    for _ in 0..120 {
        h.frame(vec![]);
        let head=head(&h.shapes).expect("演唱應顯示球頭麥克風");
        let eyes:Vec<_>=h.shapes.iter().filter_map(|s|match &s.shape {
            egui::Shape::Path(p) if p.points.len()==13 && (p.stroke.width-2.8).abs()<0.01=>Some(p.points.iter().map(|p|p.y).sum::<f32>()/13.0),_=>None,
        }).collect();
        assert_eq!(eyes.len(),2);offsets.push(head.y-(eyes[0]+eyes[1])/2.0);
    }
    if crate::bar_layout::animations_enabled(){
        let range=offsets.iter().copied().fold(f32::NEG_INFINITY,f32::max)-offsets.iter().copied().fold(f32::INFINITY,f32::min);
        assert!(range>0.4,"麥克風需要有獨立動態");
    }
    h.media.playing=false;for _ in 0..90 {h.frame(vec![]);}
    assert!(head(&h.shapes).is_none(),"暫停後應收起麥克風");
}

#[test]
fn music_mascot_puts_away_the_microphone_during_notices_and_their_fade_out(){
    use crate::extras::PetState;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]晚風吹過了山崗\n[00:15]下一句"));
    let microphone=|h:&H|h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c)if(c.radius-2.6).abs()<0.01));
    for _ in 0..90{h.frame(vec![]);}
    assert_eq!(h.app.pet_state(),PetState::Music);assert!(microphone(&h));
    for (title,body) in [("大寫鎖定","已開啟"),("大寫鎖定","已關閉"),("鍵盤輸入語言","中文"),("系統音量","音量 60%"),("網路連線","Wi-Fi · 已連線"),("電源已連線","當前電量 80%"),("藍芽裝置","耳機")]{
        h.app.alert(title,body,5,false);
        for _ in 0..60{h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Awake,"{title} 出現時應使用普通表情");assert!(!microphone(&h),"{title} 期間不能帶著演唱道具");}
        h.t+=2.0;h.frame(vec![]);
        assert!(h.app.notice.is_none());
        assert_eq!(h.app.pet_state(),PetState::Awake,"提示淡出期間也不能提前開始演唱");assert!(!microphone(&h));
        for _ in 0..90{h.frame(vec![]);}
        assert_eq!(h.app.pet_state(),PetState::Music);assert!(microphone(&h),"提示完全結束後應恢復聽歌動作");
        assert!(h.media.playing);assert!(h.app.island_lyric().is_some());
    }
    h.app.pet.react(PetState::Satisfied);h.app.alert("大寫鎖定","已開啟",5,false);h.frame(vec![]);
    assert_eq!(h.app.pet_state(),PetState::Satisfied,"已有的喝水反饋應保留優先順序");
}

#[test]
fn music_mascot_plays_new_expressions_and_resumes_after_interruptions(){
    use crate::extras::PetState;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:300.0,..Default::default()};
    for _ in 0..1080 {h.frame(vec![]);}
    let expression=h.app.pet.expressions.current().expect("聽歌狀態也應顯示新表情");
    assert_eq!(h.app.pet_state(),PetState::Music);
    let atlas=h.ctx.tex_manager().read().allocated().find_map(|(id,meta)|(meta.name=="daozai-expressions").then_some(*id)).unwrap();
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Mesh(mesh)if mesh.texture_id==atlas)));
    h.app.alert("系統音量","音量 60%",5,false);h.frame(vec![]);
    assert_eq!(h.app.pet_state(),PetState::Awake);assert!(h.app.pet.expressions.current().is_none());
    for _ in 0..300 {h.frame(vec![]);if h.app.pet_state()==PetState::Music {break;}}
    assert_eq!(h.app.pet.expressions.current(),Some(expression),"通知結束後繼續原來的聽歌表情");
    h.media.playing=false;h.frame(vec![]);assert!(h.app.pet.expressions.current().is_none());
    for _ in 0..60 {h.frame(vec![]);}
    h.media.playing=true;h.frame(vec![]);assert_eq!(h.app.pet.expressions.current(),Some(expression));
    h.app.pomo.running=true;h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Focused);
    assert!(h.app.pet.expressions.current().is_none());
    h.app.pomo.running=false;h.frame(vec![]);assert_eq!(h.app.pet.expressions.current(),Some(expression));
    h.app.settings.pet=false;h.frame(vec![]);assert!(h.app.pet.expressions.current().is_none());
}

#[test]
fn selecting_permanent_mascot_or_minimal_applies_the_selected_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.set_minimal(true);for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.settings.minimal && !h.app.settings.pet && h.region.height()<20.0);
    h.app.set_pet(true);for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.settings.pet && !h.app.settings.minimal && h.region.height()>30.0);
    let saved=crate::model::Settings::load();assert!(saved.pet && !saved.minimal);
}
#[test]
fn mascot_wakes_before_returning_to_idle(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.opts.pet_idle=45.0;
    h.frame(vec![]);h.t+=190.0;for _ in 0..60 {h.frame(vec![]);}
    assert_eq!(format!("{:?}",h.app.pet_state()),"Sleeping");
    h.cursor=Some(Pos2::new(550.0,100.0));h.frame(vec![egui::Event::PointerMoved(h.cursor.unwrap())]);
    assert_eq!(format!("{:?}",h.app.pet_state()),"Waking");
    for _ in 0..180 {h.frame(vec![]);}
    assert_eq!(format!("{:?}",h.app.pet_state()),"Awake");
}
#[test]
fn mascot_reacts_to_music_and_respects_focus(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};h.frame(vec![]);
    assert_eq!(format!("{:?}",h.app.pet_state()),"Music");
    h.app.pomo.running=true;h.frame(vec![]);assert_eq!(format!("{:?}",h.app.pet_state()),"Focused");
    h.app.pomo.running=false;h.media.playing=false;h.frame(vec![]);assert_eq!(format!("{:?}",h.app.pet_state()),"Awake");
}
#[test]
fn mascot_keeps_task_celebration_until_panel_closes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.todos.items.push(crate::model::TodoItem{text:"完成測試任務".into(),done_on:None,due:None,reminded:false,..Default::default()});
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(4,1.0);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(250.0,47.0));
    assert!(h.app.todos.items[0].done_on.is_some());
    h.hover(H::local(300.0,60.0),4.0);assert_eq!(format!("{:?}",h.app.pet_state()),"Happy");
    h.cursor=None;for _ in 0..240 {h.frame(vec![]);}
    assert_eq!(format!("{:?}",h.app.pet_state()),"Awake");
}
#[test]
fn mascot_drink_nods_and_focus_completion_celebrates(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);h.app.drink_water();
    assert_eq!(format!("{:?}",h.app.pet_state()),"Satisfied");
    for _ in 0..240 {h.frame(vec![]);}
    h.app.pomo.running=true;h.app.pomo.remaining=0.01;h.frame(vec![]);
    assert_eq!(format!("{:?}",h.app.pet_state()),"Happy");
}
#[test]
fn mascot_power_and_network_recover_without_stale_warning(){
    use crate::extras::PetState;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.power=(Some(15),false);h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Worried);
    h.power.1=true;h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Awake);
    h.network=Some(false);h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Worried);
    h.network=Some(true);h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Awake);
    h.power=(None,false);h.network=None;h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Awake);
}
#[test]
fn mascot_system_idle_wakes_once_then_follows_pointer(){
    use crate::extras::PetState;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.opts.pet_idle=45.0;
    h.system_idle=Some(200.0);h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Sleeping);
    h.system_idle=Some(0.0);h.frame(vec![]);assert_eq!(h.app.pet_state(),PetState::Waking);
    for _ in 0..180 {h.frame(vec![]);}assert_eq!(h.app.pet_state(),PetState::Awake);
    h.cursor=Some(Pos2::new(WIN_W/2.0,50.0));
    for _ in 0..60 {h.frame(vec![]);}assert_eq!(h.app.pet_state(),PetState::Curious);assert!(!h.app.open);
}
#[test]
fn mascot_feedback_plays_beside_notice_and_respects_switches(){
    use crate::extras::PetState;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.drink_water();h.app.alert("待辦到時間了","測試提醒",4,false);
    for _ in 0..60 {h.frame(vec![]);}assert_eq!(h.app.pet_state(),PetState::Satisfied);
    assert!(h.app.notice.is_some());
    let atlas=h.ctx.tex_manager().read().allocated().find_map(|(id,meta)|(meta.name=="daozai-expressions").then_some(*id)).expect("表情圖集已載入");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Mesh(mesh) if mesh.texture_id==atlas)),"通知期間島仔仍應可見");
    for _ in 0..240 {h.frame(vec![]);}assert_eq!(h.app.pet_state(),PetState::Awake);
    h.app.notice=None;for _ in 0..240 {h.frame(vec![]);}assert_eq!(h.app.pet_state(),PetState::Awake);
    h.app.drink_water();h.app.settings.minimal=true;h.frame(vec![]);assert!(h.app.pet.reaction.is_none());
    h.app.settings.minimal=false;h.app.settings.pet=false;h.app.drink_water();h.frame(vec![]);assert!(h.app.pet.reaction.is_none());
}

#[test]
#[ignore = "按需匯出吉祥物的實際渲染畫面"]
fn mascot_expression_gallery(){
    use crate::extras::PetState;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::env::var_os("DAOZAI_PET_PREVIEW_DIR").map(std::path::PathBuf::from)
        .unwrap_or_else(||std::env::temp_dir().join(format!("daozai-pet-preview-{}",std::process::id())));
    std::fs::create_dir_all(&dest).unwrap();
    let states=[("awake",PetState::Awake),("curious",PetState::Curious),("waking",PetState::Waking),
        ("happy",PetState::Happy),("satisfied",PetState::Satisfied),("music",PetState::Music),
        ("worried",PetState::Worried),("focused",PetState::Focused),("dozing",PetState::Dozing),("sleeping",PetState::Sleeping)];
    for (name,state) in states {
        let mut h=H::new();h.app.pet.react(state);
        let mut raster=crate::preview_raster::Raster::new(1280,380);
        let last=if state==PetState::Waking{42}else{79};
        for frame in 0..=last {
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),today:"2026-10-31".into(),date:"10月31日 週六".into(),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))),time:Some(t),
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                    let win=ui.max_rect();h.app.frame(ui,win,&env);
                });
            });
            raster.update_textures(&output.textures_delta);
            if frame==last {
                raster.buf.fill([0.09,0.11,0.18,1.0]);
                raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                assert!(rgba.chunks_exact(4).any(|p|p[0]>230&&p[1]>230&&p[2]>230),"表情畫面不能空白：{name}");
                image::save_buffer(dest.join(format!("{name}.png")),&rgba,1280,380,image::ColorType::Rgba8).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
    eprintln!("表情預覽：{}",dest.display());
}

#[test]
fn mascot_atlas_respects_visibility_and_skin_switches(){
    use crate::pet_expressions::Expression;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=true;h.app.settings.minimal=false;
    for _ in 0..90 {h.frame(vec![]);}
    for skin in [crate::model::Skin::Classic,crate::model::Skin::Cream,crate::model::Skin::GlassLight] {
        h.app.settings.skin=skin;
        for expression in Expression::ALL {
            h.app.pet.expressions.play(expression);for _ in 0..30 {h.frame(vec![]);}
            assert_eq!(h.app.pet.expressions.current(),Some(expression));
            let atlas=h.ctx.tex_manager().read().allocated().find_map(|(id,meta)|(meta.name=="daozai-expressions").then_some(*id)).unwrap();
            let drawn=h.shapes.iter().find_map(|shape|match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id==atlas=>Some(mesh),_=>None,
            }).expect("新表情應畫在常駐島上");
            assert!(h.region.contains_rect(drawn.calc_bounds()),"新表情不能撐出島體");
            // 每個表情單獨一張序列圖：橫向鋪滿整張，紋理只有一列大小。
            assert!(drawn.vertices[0].uv.x.abs()<0.001);
            assert_eq!(h.ctx.tex_manager().read().meta(atlas).unwrap().size,[144,1440],"只保留正在播放的表情");
        }
    }
    h.app.settings.pet=false;h.frame(vec![]);assert!(h.app.pet.expressions.current().is_none());
    h.app.settings.pet=true;h.app.pet.expressions.play(Expression::Heart);h.app.settings.minimal=true;
    h.frame(vec![]);assert!(h.app.pet.expressions.current().is_none());
}

#[test]
#[ignore="按需匯出接入程式後的十二組表情"]
fn mascot_integrated_expression_gallery(){
    use crate::pet_expressions::Expression;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_PET_INTEGRATED_PREVIEW").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    for (name,skin) in [("classic",crate::model::Skin::Classic),("cream",crate::model::Skin::Cream),("glass",crate::model::Skin::GlassLight)] {
        let mut h=H::new();h.app.settings.pet=true;h.app.settings.minimal=false;h.app.settings.skin=skin;
        let mut raster=crate::preview_raster::Raster::new(1280,540);
        let mut sequence=0;
        for (index,expression) in Expression::ALL.into_iter().enumerate() {
            for frame in 0..=(expression.duration()*20.0).ceil() as usize {
                if frame==1 {h.app.pet.expressions.play(expression);}
                let t=sequence as f64/20.0;sequence+=1;
                let env=Env{t,clock:"09:41:00".into(),today:"2026-11-06".into(),date:"11月6日 週五".into(),idle_seconds:Some(0.0),..Default::default()};
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))),time:Some(t),
                    viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    let win=ui.max_rect();h.app.frame(ui,win,&env);
                });});
                raster.update_textures(&output.textures_delta);
                if frame>0 {
                    raster.buf.fill([0.95,0.96,0.98,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw(1280,540,rgba).unwrap();
                    image::imageops::crop_imm(&image,440,0,400,100).to_image().save(dest.join(format!("{name}-{index:02}-{frame:03}.png"))).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
    }
}
#[test]
#[cfg(feature="cn-messengers")]
fn minimal_island_hides_messenger_notices_until_expanded(){
    // 群友截圖：極簡劉海下來電 / 新訊息後，細線變成帶微信圖示的方塊。極簡劉海照常彈橫幅，收起後只留細線，未讀不常掛。
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::app::Messenger;
    for (source,shape) in [(Messenger::Wechat,crate::model::IslandShape::Notch),(Messenger::Wechat,crate::model::IslandShape::Pill),(Messenger::Qq,crate::model::IslandShape::Pill)] {
        let mut h=H::new();
        h.app.settings.wechat_notifications=true;h.app.settings.read_notifications=true;h.app.settings.minimal=true;h.app.settings.island_shape=shape;
        for _ in 0..60 {h.frame(vec![]);}
        let idle=h.region;
        match source {
            Messenger::Wechat=>h.app.receive_wechat_reminder_from(4242,0x1234),
            Messenger::Qq=>h.app.receive_notification("QQ".into(),"QQ".into(),"QQ · 新訊息".into()),
            Messenger::WeCom|Messenger::Feishu|Messenger::DingTalk=>h.app.receive_messenger_reminder(crate::app::Unread::of(source)),
            Messenger::Discord|Messenger::Line=>unreachable!("本測試只覆蓋舊版 IM 的極簡橫幅"),
        }
        for _ in 0..30 {h.frame(vec![]);}
        assert!(h.app.notice.as_ref().and_then(|a|a.messenger())==Some(source),"{source:?} {shape:?}：極簡劉海照常彈橫幅");
        for _ in 0..(8*60) {h.frame(vec![]);}
        assert!(h.app.notice.is_none(),"{source:?}：橫幅按時收起");
        assert_eq!(h.app.pending_messengers.len(),1,"{source:?}：未讀仍要記住");
        assert_eq!(h.region,idle,"{source:?} {shape:?}：收起時仍只有細線");
        // 滑鼠移上去展開：未讀膠囊出現在面板旁，可以點。
        h.hover(Pos2::new(WIN_W/2.0,2.0),0.8);h.hover(Pos2::new(WIN_W/2.0,40.0),1.0);
        assert!(h.app.open && h.app.progress()>0.9,"{source:?}：懸停應展開");
        assert!(h.app.messenger_appear()>0.9 && h.app.pending_messengers.len()==1,"{source:?}：展開後未讀膠囊在面板旁顯示");
    }
}
/// 微信語音來電：響鈴期間工作列持續閃動，按秒渲染島體，排查樣式問題。
#[test]
#[ignore="生成來電響鈴時的島體逐幀畫面，供人工檢視"]
fn preview_wechat_call_ringing(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::env::var_os("DAOZAI_CALL_PREVIEW_DIR").map(std::path::PathBuf::from)
        .unwrap_or_else(||std::env::temp_dir().join(format!("daozai-call-preview-{}",std::process::id())));
    std::fs::create_dir_all(&dest).unwrap();
    // 單條提醒 / 響鈴期間反覆提醒 / 來電視窗在另一個程式裡（被當成第二個賬號）。
    for (scenario,minimal) in [("single",false),("single",true),("ringing",false),("ringing",true),("twoproc",false),("twoproc",true)] {
        let mode=format!("{scenario}-{}",if minimal{"minimal"}else{"normal"});
        let mut h=H::new();h.app.settings.wechat_notifications=true;h.app.settings.minimal=minimal;
        let mut raster=crate::preview_raster::Raster::new(1280,380);
        for frame in 0..=(16*60) {
            let t=frame as f64/60.0;
            match scenario {
                "single" if frame==60=>h.app.receive_wechat_reminder_from(4242,0x1234),
                "ringing" if frame>=60 && t<12.0 && frame%330==60=>h.app.receive_wechat_reminder_from(4242,0x1234),
                "twoproc" if frame==60=>h.app.receive_wechat_reminder_from(4242,0x1234),
                "twoproc" if frame==120=>h.app.receive_wechat_reminder_from(4343,0x5678),
                _=>{}
            }
            let env=Env{t,clock:"09:41:00".into(),today:"2026-10-31".into(),date:"10月31日 週六".into(),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))),time:Some(t),
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});
            });
            raster.update_textures(&output.textures_delta);
            if frame%60==0 {
                raster.buf.fill([0.09,0.11,0.18,1.0]);
                raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest.join(format!("{mode}-{:04.1}s.png",t)),&rgba,1280,380,image::ColorType::Rgba8).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
    eprintln!("來電預覽：{}",dest.display());
}
#[test]
fn todo_due_roundtrip_and_fires_only_once(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items.push(crate::model::TodoItem{text:"會議 | 準備".into(),done_on:None,due:Some("2026-10-28 09:40".into()),reminded:false,..Default::default()});h.app.todos.save();
    let loaded=crate::model::TodoList::load();assert_eq!(loaded.items[0].text,"會議 | 準備");assert_eq!(loaded.items[0].due.as_deref(),Some("2026-10-28 09:40"));
    h.frame(vec![]);assert!(h.app.todos.items[0].reminded);let n=h.cmds.len();h.frame(vec![]);assert_eq!(n,h.cmds.len());assert!(crate::model::TodoList::load().items[0].reminded);
}
#[test]
fn quiet_water_hours_cover_overnight(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.water_start=22;h.app.settings.water_end=7;
    assert!(h.app.water_hours(23));assert!(h.app.water_hours(2));assert!(!h.app.water_hours(12));
}
#[test]
fn repeated_system_changes_merge_without_losing_history(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.alert("系統音量","音量 20%",5,false);h.frame(vec![]);
    h.app.alert("系統音量","音量 30%",5,false);h.frame(vec![]);
    assert_eq!(h.app.notice.as_ref().unwrap().body,"音量 30%");assert_eq!(h.app.history.len(),2);assert!(h.app.alerts.is_empty());
}
#[test]
fn windows_notification_echo_does_not_duplicate_own_reminders(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.alert("喝水時間到了","喝點水吧",2,true);
    h.app.settings.read_notifications=true;h.app.receive_notification("Windows 通知".into(),"test".into(),"喝水時間到了 · 喝點水吧".into());assert_eq!(h.app.history.len(),1);
    assert_eq!(h.app.alerts.len(),1,"島仔發出的系統通知不能再次進入島內通知佇列");
    h.app.notification_apps.items.push(("mail".into(),"郵件".into(),true));h.app.receive_notification("郵件".into(),"mail".into(),"收到一封新郵件".into());assert_eq!(h.app.alerts.len(),2);assert!(h.app.notification_bell_until>h.app.t);
}

#[test]
fn minimal_takes_precedence_over_mascot(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=true;h.app.settings.minimal=true;
    for _ in 0..120 {h.frame(vec![]);}assert!(h.region.height()<20.0,"啟用極簡和吉祥物時必須顯示細線");
    h.hover(Pos2::new(WIN_W/2.0,2.0),1.0);assert!(h.app.progress()>0.9,"極簡線仍要能懸停展開");
}
#[test]
fn water_settings_are_in_the_island_and_editable(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.app.go(2,1.0);h.hover(H::local(300.0,60.0),0.8);h.click(H::local(145.0,110.0));assert_eq!(h.app.page,2);assert_eq!(h.app.subpage,Some(2));assert_eq!(h.app.settings_category,1);
    h.hover(H::local(300.0,60.0),0.8);h.click(H::local(357.0,69.0));assert_eq!(h.app.settings.serving_ml,300);
    // 標題旁的切換在設定子頁同樣可用：切到久坐後第一行就是久坐時長，最後一行時段兩邊共用。
    h.click(H::local(136.0,19.0));assert!(h.app.sit_mode);for _ in 0..40{h.frame(vec![]);}
    h.hover(H::local(300.0,60.0),0.3);h.click(H::local(357.0,47.0));assert_eq!(h.app.settings.sit_minutes,65);
    h.hover(H::local(300.0,60.0),0.3);h.click(H::local(357.0,113.0));assert!(h.app.health_time,"最後一行進入提醒時段二級頁");
    let end=h.app.settings.water_end;h.hover(H::local(300.0,60.0),0.3);h.click(H::local(357.0,47.0));assert_eq!(h.app.settings.water_end,(end+1)%24);
}
#[test]
fn lunch_break_silences_water_and_sit_reminders(){
    use crate::app::SIT_TITLE;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.system_idle=Some(0.0);h.frame(vec![]);
    // 測試時鐘固定在 09:41，把午休設成 09:30–10:00。
    h.app.settings.lunch_rest=true;h.app.settings.lunch_start=9*60+30;h.app.settings.lunch_end=10*60;
    h.app.water.remind=true;h.app.last_reminder=h.app.t-2.0*3600.0;h.app.sit_start=h.app.t-2.0*3600.0;
    h.frame(vec![]);assert!(h.app.notice.is_none(),"午休中不應彈喝水或久坐提醒");
    h.app.settings.lunch_rest=false;h.frame(vec![]);
    assert!(h.app.notice.is_some(),"關閉午休後應恢復提醒");
    h.app.notice=None;h.app.alerts.clear();h.app.water.remind=false;h.app.confirm_health_reminder(crate::extras::HealthReminder::Sit);h.app.sit_start=h.t-2.0*3600.0;
    h.app.settings.lunch_rest=true;h.app.settings.lunch_start=10*60;h.app.settings.lunch_end=13*60;
    h.frame(vec![]);assert!(h.app.notice.as_ref().is_some_and(|a|a.title==SIT_TITLE),"午休以外照常提醒");
}
#[test]
fn health_time_subpage_edits_lunch_and_backs_out_one_level(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.app.open_settings_category(1);for _ in 0..30{h.frame(vec![]);}
    h.hover(H::local(300.0,60.0),0.3);h.click(H::local(200.0,113.0));assert!(h.app.health_time,"點提醒時段行進入二級頁");
    let start=h.app.settings.lunch_start;h.hover(H::local(300.0,60.0),0.3);h.click(H::local(225.0,91.0));assert_eq!(h.app.settings.lunch_start,start+15);
    h.hover(H::local(300.0,60.0),0.3);h.click(H::local(353.0,69.0));assert!(!h.app.settings.lunch_rest,"午休開關可關閉");
    h.hover(H::local(300.0,60.0),0.3);h.click(H::local(20.0,18.0));assert!(!h.app.health_time);assert_eq!(h.app.subpage,Some(2),"返回先回到健康提醒設定");
}
#[test]
fn sit_reminder_waits_for_confirmation_before_restarting(){
    use crate::app::SIT_TITLE;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.system_idle=Some(0.0);h.frame(vec![]);h.app.water.remind=false;
    h.t+=59.0*60.0;h.frame(vec![]);assert!(h.app.notice.is_none(),"未滿 60 分鐘不提醒: {:?}",h.app.notice.as_ref().map(|a|(a.title.clone(),a.body.clone())));
    h.t+=61.0;h.frame(vec![]);
    assert!(h.app.notice.as_ref().is_some_and(|a|a.title==SIT_TITLE));
    assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Notify(t,_)if t==SIT_TITLE)));
    let count=h.app.history.len();
    h.screen_locked=true;h.system_idle=Some(40.0*60.0);h.t+=500.0*60.0;h.frame(vec![]);
    assert!(h.app.notice.is_none());assert_eq!(h.app.history.len(),count,"鎖屏和長時間未處理都只保留一次提醒");
    h.app.confirm_health_reminder(crate::extras::HealthReminder::Sit);assert_eq!(h.app.sat_minutes(),0);
    h.screen_locked=false;h.system_idle=Some(0.0);h.t+=59.0*60.0;h.frame(vec![]);assert!(h.app.notice.is_none());
    h.t+=61.0;h.frame(vec![]);assert!(h.app.notice.as_ref().is_some_and(|a|a.title==SIT_TITLE));
}
#[test]
fn sit_reminder_waits_for_focus_and_stops_with_module_or_switch(){
    use crate::app::{Phase,SIT_TITLE,WATER_PAGE};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.system_idle=Some(0.0);h.frame(vec![]);h.app.water.remind=false;
    h.app.pomo.phase=Phase::Focus;h.app.pomo.remaining=1e7;h.app.pomo.running=true;
    h.t+=61.0*60.0;h.frame(vec![]);assert!(h.app.notice.is_none(),"專注中不打擾: {:?}",h.app.notice.as_ref().map(|a|(a.title.clone(),a.body.clone())));
    h.app.pomo.running=false;h.frame(vec![]);assert!(h.app.notice.as_ref().is_some_and(|a|a.title==SIT_TITLE),"專注結束後補上提醒");
    h.app.notice=None;h.app.settings.sit_remind=false;h.t+=2.0*3600.0;h.frame(vec![]);assert!(h.app.notice.is_none());
    h.app.settings.sit_remind=true;h.app.set_page_visible(WATER_PAGE,false);h.t+=2.0*3600.0;h.frame(vec![]);assert!(h.app.notice.is_none(),"模組關閉即不再提醒");
}
#[test]
fn sit_notice_and_page_buttons_reset_the_timer(){
    use crate::app::SIT_TITLE;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.system_idle=Some(0.0);h.frame(vec![]);
    h.app.sit_start=h.t-70.0*60.0;h.app.alert(SIT_TITLE,"已連續坐了 70 分鐘",2,false);for _ in 0..60 {h.frame(vec![]);}
    h.click_text("知道了");assert!(h.app.notice.is_none());assert!((h.app.sit_due()-h.t-3600.0).abs()<5.0);assert_eq!(h.app.water.drunk_ml,0);
    h.app.alert(SIT_TITLE,"已連續坐了 70 分鐘",2,false);for _ in 0..60 {h.frame(vec![]);}
    h.click_text("起身了");assert!(h.app.notice.is_none());assert_eq!(h.app.sat_minutes(),0);assert_eq!(h.app.water.drunk_ml,0);
    // 健康頁：點「久坐」分段切換，同一位置的主按鈕變成「起身了」、開關管久坐；點「喝水」切回
    h.app.sit_start=h.t-30.0*60.0;
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(2,1.0);h.hover(H::local(300.0,60.0),0.8);
    h.click(H::local(124.0,22.0));assert!(h.app.sit_mode);h.hover(H::local(300.0,90.0),0.8);
    h.click(H::local(70.0,110.0));assert_eq!(h.app.sat_minutes(),0);assert_eq!(h.app.water.drunk_ml,0);
    let water_remind=h.app.water.remind;
    h.click(H::local(179.0,46.0));assert!(!h.app.settings.sit_remind);assert_eq!(h.app.water.remind,water_remind,"久坐模式的開關只管久坐");
    assert!(!crate::model::Settings::load().sit_remind,"重新載入後應保留久坐開關");
    h.click(H::local(97.0,22.0));assert!(!h.app.sit_mode);h.hover(H::local(300.0,90.0),0.8);
    h.click(H::local(70.0,110.0));assert_eq!(h.app.water.drunk_ml,250,"回到喝水後按鈕可用");
}
#[test]
fn water_notice_buttons_work_without_opening_panel(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.alert("喝水時間到了","該喝水了",2,false);for _ in 0..60 {h.frame(vec![]);}
    h.click_text("喝一口");assert_eq!(h.app.water.drunk_ml,250);assert!(h.app.notice.is_none());
}
#[test]
fn focus_completion_offers_starting_the_break(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);h.app.pomo.running=true;h.app.pomo.remaining=1.0;h.t+=2.0;h.frame(vec![]);
    assert!(h.app.pomo.phase==crate::app::Phase::Rest);assert!(!h.app.pomo.running);assert!(h.app.notice.is_some());
    for _ in 0..60 {h.frame(vec![]);}h.click_text("開始");assert!(h.app.pomo.running);
}
#[test]
fn todo_completed_items_move_down_and_undo_restores_the_original_order(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=["已完成甲","待做甲","週一重複","已完成乙","每日待辦"].into_iter().map(crate::model::TodoItem::new).collect();
    h.app.todos.items[0].done_on=Some("2026-10-28".into());h.app.todos.items[3].done_on=Some("2026-10-28".into());h.app.todos.items[2].repeat=1;h.app.todos.items[4].repeat=crate::model::REPEAT_ALL;
    let original=h.app.todos.items.iter().map(|t|t.text.clone()).collect::<Vec<_>>();assert_eq!(h.app.todos.display_order("2026-10-28"),vec![1,4,2,0,3],"今天未完成優先，其次其他日期任務，已完成沉底");
    h.app.todos.items[0].toggle("2026-10-28");assert_eq!(h.app.todos.display_order("2026-10-28"),vec![0,1,4,2,3],"取消完成應恢復原有相對順序");
    h.app.todos.items[4].toggle("2026-10-28");assert_eq!(h.app.todos.display_order("2026-10-28"),vec![0,1,2,3,4]);assert_eq!(h.app.todos.display_order("2026-10-29"),vec![0,1,4,2,3],"重複任務到下一天應回到未完成區");
    assert_eq!(h.app.todos.items.iter().map(|t|t.text.clone()).collect::<Vec<_>>(),original,"只改變顯示順序，不能移動原資料");
}

#[test]
fn todo_edit_choice_can_edit_other_rows_and_completed_items(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=["第一項","第二項","已完成任務"].into_iter().map(crate::model::TodoItem::new).collect();h.app.todos.items[2].done_on=Some("2026-10-28".into());
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click_text("改一下");
    assert!(h.app.editing.is_none(),"多條待辦應先選擇，不能預設修改第一項");h.frame(vec![]);h.click_text("第二項");
    assert_eq!(h.app.editing,Some(1));h.app.buf="第二項已修改".into();h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!(h.app.todos.items[0].text,"第一項");assert_eq!(h.app.todos.items[1].text,"第二項已修改");assert!(h.app.todos.items[2].done_on.is_some());
    h.frame(vec![]);h.click_text("改一下");h.frame(vec![]);h.click_text("已完成任務");assert_eq!(h.app.editing,Some(2));
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());assert!(h.app.todos.items[2].done_on.is_some());
}

#[test]
fn todo_edit_choice_keeps_a_single_completed_task_editable(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=vec![crate::model::TodoItem::new("已完成任務")];h.app.todos.items[0].done_on=Some("2026-10-28".into());
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click_text("改一下");assert_eq!(h.app.editing,Some(0),"只有一條時可直接編輯，完成狀態不能禁用編輯");
}

#[test]
fn todo_edit_choice_reaches_scrolled_items_and_cancels_without_changes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=(0..7).map(|i|crate::model::TodoItem::new(format!("任務 {i}"))).collect();
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click_text("改一下");assert!(h.app.editing.is_none());
    h.hover(H::local(300.0,60.0),0.2);for _ in 0..7{h.wheel(-24.0);}assert_eq!(h.app.page(),4,"選擇事項時滾到末尾也不能翻頁");h.frame(vec![]);h.click_text("任務 6");assert_eq!(h.app.editing,Some(6));
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());assert_eq!(h.app.todos.items[6].text,"任務 6");assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()));
}

#[test]
fn todo_completed_reordering_does_not_complete_the_next_item_on_a_double_click(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=["任務甲","任務乙","任務丙"].into_iter().map(crate::model::TodoItem::new).collect();h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);
    let point=H::local(212.0,46.0);h.click(point);h.frame(vec![]);h.click(point);h.frame(vec![]);
    assert!(h.app.todos.items[0].done_on.is_some());assert!(h.app.todos.items[1].done_on.is_none(),"首項下移後，快速連點不能誤完成下一項");
    h.click_text("任務甲");h.frame(vec![]);assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()));assert_eq!(h.app.todos.display_order("2026-10-28"),vec![0,1,2]);
}

#[test]
fn todo_edit_choice_can_cancel_without_toggling_or_deleting_items(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=["任務甲","任務乙"].into_iter().map(crate::model::TodoItem::new).collect();h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);
    h.click_text("改一下");h.frame(vec![]);h.click_text("取消");assert!(!h.app.is_editing());assert_eq!(h.app.todos.items.len(),2);assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()));
    h.frame(vec![]);h.click_text("改一下");h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());assert_eq!(h.app.todos.items.len(),2);
}

#[test]
fn todo_edit_choice_keeps_the_selected_item_when_an_earlier_item_is_deleted(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=["第一項","第二項","第三項"].into_iter().map(crate::model::TodoItem::new).collect();h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);
    h.click_text("改一下");h.frame(vec![]);h.click_text("第二項");h.frame(vec![]);h.click(h.ctx.read_response(egui::Id::new(("card",true,("todo-delete",0usize)))).unwrap().rect.center());
    assert_eq!(h.app.editing,Some(0),"刪除前面的條目後，編輯目標仍須是第二項");h.app.buf="第二項已修改".into();h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert_eq!(h.app.todos.items.iter().map(|t|t.text.as_str()).collect::<Vec<_>>(),["第二項已修改","第三項"]);
}

#[test]
#[ignore="按需匯出待辦完成排序和點選編輯的實際畫面"]
fn todo_edit_choice_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_TODO_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for skin in [crate::model::Skin::Classic,crate::model::Skin::Cream]{for choosing in [false,true]{
        let mut h=H::new();h.app.settings.skin=skin;h.app.todos.items=["整理會議資料","確認本週安排","已經完成的任務"].into_iter().map(crate::model::TodoItem::new).collect();h.app.todos.items[2].done_on=Some("2026-11-06".into());h.app.show_page(4);h.app.todo_choosing=choosing;
        let mut raster=feature_raster();shoot_feature(&mut h,&mut raster,&dest,&format!("todo-{}-{}",skin.key(),if choosing{"choose"}else{"list"}),90,Some(H::local(175.0,70.0)),"2026-11-06");
    }}
}

#[test]
fn task_editor_saves_and_cancels_inside_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.app.todos.items.push(crate::model::TodoItem{text:"準備會議".into(),done_on:None,due:None,reminded:false,..Default::default()});h.app.go(4,1.0);h.hover(H::local(300.0,60.0),0.8);h.app.open_task_dialog(0);
    h.hover(H::local(300.0,60.0),0.2);assert!(h.app.is_editing());h.click(H::local(326.0,110.0));assert!(!h.app.is_editing());assert_eq!(h.app.todos.items[0].text,"準備會議");
    h.app.open_task_dialog(0);h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());
}
#[test]
fn focus_timer_gets_room_in_the_collapsed_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();for _ in 0..90 {h.frame(vec![]);}
    let before=h.app.current_width();h.app.pomo.running=true;
    for _ in 0..100 {h.frame(vec![]);}assert!(h.app.current_width()>before+20.0);assert!(h.app.pomo.remaining<1500.0);
}

#[test]
fn switching_to_rest_waits_for_the_start_button(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(1,1.0);h.hover(H::local(260.0,70.0),1.0);
    h.click(H::local(296.0,106.5));
    assert!(h.app.pomo.phase==crate::app::Phase::Rest);
    assert!(!h.app.pomo.running,"切換階段應等待使用者點選開始");
}

#[test]
fn starting_timer_keeps_lyrics_album_and_mascot_with_a_compact_countdown(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.island_lyrics=true;h.app.settings.pet=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,
        artwork:Some(std::sync::Arc::new(egui::ColorImage::new([32,32],egui::Color32::from_rgb(70,120,220)))),..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]晚風吹過了山崗\n[00:15]下一句"));
    for _ in 0..120 {h.frame(vec![]);}
    for phase in [crate::app::Phase::Focus,crate::app::Phase::Rest] {
        h.app.pomo.phase=phase;h.app.pomo.remaining=h.app.phase_secs(phase);h.app.pomo.running=true;
        for _ in 0..120 {
            h.frame(vec![]);
            assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("晚風吹過了山崗"),"計時不能關閉島上歌詞");
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="晚風吹過了山崗" && t.galley.rows.len()==1)));
            assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="09:41")));
        }
        let texture=h.app.music_artwork.as_ref().unwrap().1.id();
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.brush.as_ref().is_some_and(|b|b.fill_texture_id==texture))));
        let secs=h.app.pomo.remaining.ceil() as u32;let countdown=format!("{:02}:{:02}",secs/60,secs%60);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==countdown)),"歌詞模式中也應顯示計時");
        assert!(h.app.current_width()<=PW+0.1);assert!((h.hit.height()-34.0).abs()<0.1);
    }
    h.app.pomo.running=false;for _ in 0..90 {h.frame(vec![]);}
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("晚風吹過了山崗"));
}

#[test]
fn timer_keeps_the_existing_lyric_segments_and_content_bounds(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let line="願每一個認真生活的人，都能在平凡的日子裡找到屬於自己的光";
    for pet in [true,false] {
        let mut h=H::new();h.app.settings.island_lyrics=true;h.app.settings.pet=pet;
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:6.0,duration:120.0,playing:true,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc(&format!("[00:05]{line}\n[00:15]下一句")));
        let shown=|h:&H|h.shapes.iter().find_map(|s|match &s.shape {
            egui::Shape::Text(t) if !t.galley.text().is_empty() && line.contains(t.galley.text())=>Some((t.galley.text().to_owned(),t.visual_bounding_rect(),t.galley.rows.len(),t.galley.elided)),_=>None,
        }).unwrap();
        for position in [6.0,14.0] {
            h.media.position=position;h.app.pomo.running=false;
            for _ in 0..90 {h.frame(vec![]);}let expected=shown(&h).0;
            h.app.pomo.running=true;for _ in 0..120 {h.frame(vec![]);}
            let (text,bounds,rows,elided)=shown(&h);assert_eq!(text,expected,"計時不應改變歌詞分段");
            assert_eq!(rows,1);assert!(!elided);
            let secs=h.app.pomo.remaining.ceil() as u32;let countdown=format!("{:02}:{:02}",secs/60,secs%60);
            let timer=h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.text()==countdown=>Some(t.visual_bounding_rect()),_=>None}).unwrap();
            assert!(bounds.right()+20.0<timer.left(),"歌詞與倒計時之間應留出波形和圖示的位置");
            assert!(timer.right()<h.hit.right() && bounds.top()>=h.hit.top() && bounds.bottom()<h.hit.bottom());
            assert!((h.app.current_width()-if pet{418.0}else{370.0}).abs()<0.1);
        }
    }
}

#[test]
fn focus_page_selects_stopped_phases_and_starts_only_from_play(){
    use crate::app::Phase;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.focus_minutes=30;h.app.settings.rest_minutes=7;
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(1,1.0);h.hover(H::local(260.0,70.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="休息")));
    h.click(H::local(336.0,106.5));
    assert!(h.app.pomo.phase==Phase::Rest && !h.app.pomo.running);
    assert_eq!(h.app.pomo.remaining,420.0,"休息採用設定中的時長");
    h.click(H::local(264.0,106.5));assert!(h.app.pomo.running);
    h.click(H::local(264.0,106.5));assert!(!h.app.pomo.running);
    let paused=h.app.pomo.remaining;for _ in 0..30 {h.frame(vec![]);}assert_eq!(h.app.pomo.remaining,paused);
    h.click(H::local(264.0,106.5));assert!(h.app.pomo.running && h.app.pomo.phase==Phase::Rest);
    h.click(H::local(336.0,106.5));assert!(h.app.pomo.running,"點選當前階段不重置計時");
    h.click(H::local(192.0,106.5));assert!(h.app.pomo.phase==Phase::Focus && !h.app.pomo.running);
    assert_eq!(h.app.pomo.remaining,1800.0);
    h.click(H::local(264.0,106.5));assert!(h.app.pomo.running);
    h.click(H::local(336.0,106.5));assert!(h.app.pomo.phase==Phase::Rest && !h.app.pomo.running);
    h.click(H::local(264.0,106.5));assert!(h.app.pomo.running);
    h.click(H::local(329.0,50.0));assert!(!h.app.pomo.running && h.app.pomo.phase==Phase::Rest);
    assert_eq!(h.app.pomo.remaining,420.0,"重置應保留休息階段");
}

#[test]
fn collapsed_focus_uses_tomato_and_rest_uses_a_white_cup(){
    use crate::app::Phase;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for phase in [Phase::Focus,Phase::Rest] {
        h.app.pomo.phase=phase;h.app.pomo.remaining=h.app.phase_secs(phase);h.app.pomo.running=true;
        for _ in 0..120 {h.frame(vec![]);}
        let secs=h.app.pomo.remaining.ceil() as u32;let text=format!("{:02}:{:02}",secs/60,secs%60);
        let color=if phase==Phase::Focus{crate::theme::RED_TOP}else{crate::theme::SOFT_GREEN};
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==text && t.fallback_color==color)),"常駐倒計時顏色應跟隨階段");
        let tomato=h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Mesh(m) if m.vertices.iter().any(|v|v.color==crate::theme::RED_TOP)));
        if phase==Phase::Focus {assert!(tomato,"專注時應顯示紅色番茄水位");}
        else {
            assert!(!tomato,"休息時應替換番茄");
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.fill==egui::Color32::from_rgb(245,242,234) && (7.0..13.0).contains(&r.rect.width()) && (5.0..10.0).contains(&r.rect.height()))),"休息時應顯示白瓷茶杯");
        }
        assert!((h.hit.height()-34.0).abs()<0.1,"圖示不能撐高常駐島");
    }
}

#[test]
fn focus_and_rest_completion_prepare_the_configured_next_phase_without_starting(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.settings.rest_minutes=7;h.app.settings.focus_minutes=30;
    for _ in 0..5 {
        h.app.pomo.phase=crate::app::Phase::Focus;h.app.pomo.running=true;h.app.pomo.remaining=0.01;h.frame(vec![]);
        assert!(h.app.pomo.phase==crate::app::Phase::Rest && !h.app.pomo.running);
        assert_eq!(h.app.pomo.remaining,420.0);assert!(h.app.notice.is_some());
        h.app.pomo.running=true;h.app.pomo.remaining=0.01;h.frame(vec![]);
        assert!(h.app.pomo.phase==crate::app::Phase::Focus && !h.app.pomo.running);
        assert_eq!(h.app.pomo.remaining,1800.0);
    }
}

#[test]
fn rest_settings_keep_the_existing_duration_and_save_the_new_key(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let _h=H::new();
    assert_eq!(crate::model::Settings::default().rest_minutes,5);
    let path=crate::model::config_dir().join("settings.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path,"focus_minutes=30\nshort_minutes=7\nlong_minutes=17\n").unwrap();
    let settings=crate::model::Settings::load();assert_eq!(settings.rest_minutes,7);
    settings.save();let saved=std::fs::read_to_string(&path).unwrap();
    assert!(saved.lines().any(|line|line=="rest_minutes=7"));
    assert!(!saved.contains("short_minutes") && !saved.contains("long_minutes"));
    let reloaded=crate::model::Settings::load();assert_eq!(reloaded.rest_minutes,7);assert_eq!(reloaded.focus_minutes,30);
}

#[test]
fn collapsed_focus_animates_without_music_or_mascot_and_stops_in_minimal_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=false;h.app.pomo.running=true;
    for _ in 0..120 {h.frame(vec![]);}
    if crate::bar_layout::animations_enabled(){assert!(h.animating,"關閉吉祥物後，常駐番茄的水面仍應輕微波動");}
    h.app.settings.minimal=true;for _ in 0..120 {h.frame(vec![]);}
    assert!(!h.animating,"極簡細線隱藏計時圖示後，應停止其動畫重新整理");
}
#[test]
fn app_notifications_show_temporary_bell_in_minimal_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.minimal=true;h.frame(vec![]);
    h.app.settings.read_notifications=true;h.app.receive_notification("郵件".into(),"mail.app".into(),"私人內容".into());
    assert!(h.app.notification_apps.items[0].2);assert!(h.app.notification_bell_until>h.app.t);assert_eq!(h.app.alerts.len(),1);
    assert!(!std::fs::read_to_string(crate::model::config_dir().join("notification-apps.json")).unwrap().contains("私人內容"));
    for _ in 0..90 {h.frame(vec![]);}assert!(h.region.height()>20.0,"極簡模式收到通知時應臨時展開");
    h.t+=5.0;for _ in 0..90 {h.frame(vec![]);}assert!(h.region.height()<20.0,"鈴鐺消失後應恢復極簡模式");
    h.app.notification_apps.items[0].2=false;let until=h.app.notification_bell_until;
    h.app.settings.read_notifications=true;h.app.receive_notification("郵件".into(),"mail.app".into(),"私人內容".into());assert_eq!(h.app.notification_bell_until,until);
}

#[test]
fn fluid_music_seek_draws_solid_played_segment(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for lyrics in [false,true] {
        let mut h=H::new();
        h.app.settings.skin=crate::model::Skin::Fluid;
        h.app.show_lyrics=lyrics;
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:30.0,duration:120.0,..Default::default()};
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.2);
        let (x,w,y)=if lyrics{(44.0,176.0,82.0)}else{(96.0,221.0,92.6)};
        let start=H::local(x,y);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r)
            if (r.rect.left()-start.x).abs()<1.0 && (r.rect.center().y-start.y).abs()<1.0
            && (r.rect.width()-w*0.25).abs()<1.0 && r.fill.a()==255 && r.fill.r()>160)));
    }
}

#[test]
fn music_seek_drag_previews_and_commits_on_release(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (source,lyrics) in [("",false),("",true),("cloudmusic.exe",false),("cloudmusic.exe",true),("bodian_pc.exe",false),("bodian_pc.exe",true),("kwmusic.exe",false),("kwmusic.exe",true)] {
        let mut h=H::new();h.app.show_lyrics=lyrics;
        h.media=crate::smtc::NowPlaying{source:source.into(),title:"Track".into(),artist:"Artist".into(),position:12.0,duration:120.0,..Default::default()};
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
        let (x,w,y)=if lyrics{(44.0,176.0,82.0)}else{(96.0,221.0,92.6)};
        let press=H::local(x+w*0.1,y);h.cursor=Some(press);
        h.frame(vec![egui::Event::PointerMoved(press)]);
        h.frame(vec![egui::Event::PointerButton{pos:press,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let to=H::local(x+w*0.75,y);h.cursor=Some(to);
        h.frame(vec![egui::Event::PointerMoved(to)]);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="01:30")),"{source} 拖動中應顯示目標時間");
        assert!(!h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Media(crate::smtc::Transport::Seek(_)))),"拖動中不應反覆傳送跳轉");
        h.frame(vec![egui::Event::PointerButton{pos:to,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Media(crate::smtc::Transport::Seek(t)) if (*t-90.0).abs()<0.1)),"釋放應提交目標進度");
    }
}

#[test]
fn old_and_new_qqmusic_seek_and_hold_the_target_after_release(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for source in ["QQMusic.exe","Tencent.QQMusic"] {
        let mut h=H::new();h.app.show_lyrics=false;
        h.media=crate::smtc::NowPlaying{source:source.into(),title:"Track".into(),artist:"Artist".into(),position:12.0,duration:120.0,playback_type:crate::smtc::MediaKind::Music,..Default::default()};
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
        let (x,w,y)=(96.0,221.0,92.6);
        let press=H::local(x+w*0.1,y);h.cursor=Some(press);
        h.frame(vec![egui::Event::PointerMoved(press)]);
        h.frame(vec![egui::Event::PointerButton{pos:press,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let to=H::local(x+w*0.75,y);h.cursor=Some(to);
        h.frame(vec![egui::Event::PointerMoved(to)]);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="01:30")),"{source} 拖動中顯示目標時間");
        h.frame(vec![egui::Event::PointerButton{pos:to,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Media(crate::smtc::Transport::Seek(t)) if (*t-90.0).abs()<0.1)),"{source} 鬆手提交跳轉");
        // 播放器還沒回報新位置：鬆手後仍顯示目標時間，不彈回 00:12。
        h.frame(vec![]);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="01:30")),"{source} 鬆手後保持目標時間");
        assert!(crate::smtc::is_qqmusic(source));
    }
}

#[test]
fn volume_hud_drag_maps_pointer_to_percentage(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.alert("系統音量","音量 0%",5,false);
    for _ in 0..60 {h.frame(vec![]);}
    assert!(!h.app.open);
    h.click_notice_badge();
    // 縮短後的系統音量條仍支援點選和拖動。
    let (x0,y)=if h.app.compact_notices(){(WIN_W/2.0+(h.app.current_width()-44.0)/2.0+8.0+crate::app::HUD_TRACK_X,55.0)}else{(WIN_W/2.0-h.app.current_width()/2.0+h.app.pet_space()+crate::app::HUD_TRACK_X,19.0)};
    let press=Pos2::new(x0+52.0,y);
    h.cursor=Some(press);h.frame(vec![egui::Event::PointerMoved(press)]);
    h.frame(vec![egui::Event::PointerButton{pos:press,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    let to=Pos2::new(x0+105.0,y);
    h.cursor=Some(to);h.frame(vec![egui::Event::PointerMoved(to)]);
    h.frame(vec![egui::Event::PointerButton{pos:to,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    let last=h.cmds.iter().rev().find_map(|c|match c{crate::app::Cmd::SetVolume(v)=>Some(*v),_=>None});
    assert_eq!(last,Some(70));
}

#[test]
fn bluetooth_connection_soon_after_initial_scan_is_not_suppressed(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.frame(vec![]);
    h.t=2.0;
    h.bt=vec![crate::bt::Device{id:"shokz".into(),name:"Shokz".into(),battery:Some(100),kind:crate::bt::DeviceKind::Bone}];
    h.frame(vec![]);
    assert_eq!(h.app.notice.as_ref().map(|a|a.title.as_str()),Some("藍芽裝置"));
}

#[test]
fn bluetooth_audio_connection_takes_precedence_over_volume_notice(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for volume_already_visible in [false,true] {
        let mut h=H::new();h.frame(vec![]);
        h.app.alert("系統音量","音量 80%",5,false);
        if volume_already_visible {h.frame(vec![]);}
        h.bt=vec![crate::bt::Device{id:"audio".into(),name:"Shokz".into(),battery:Some(80),kind:crate::bt::DeviceKind::Bone}];
        h.frame(vec![]);
        assert_eq!(h.app.notice.as_ref().map(|a|a.title.as_str()),Some("藍芽裝置"),"耳機連線應優先於音量提示");
        assert!(!h.app.alerts.iter().any(|a|a.title=="系統音量"),"切換耳機產生的音量提示不應延後補彈");
        h.t+=3.0;h.frame(vec![]);
        h.app.alert("系統音量","音量 60%",5,false);h.frame(vec![]);
        assert_eq!(h.app.notice.as_ref().map(|a|a.body.as_str()),Some("音量 60%"),"連線後手動調節音量應正常提示");
    }
}

#[test]
fn bluetooth_keyboard_connection_keeps_unrelated_volume_notice(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.frame(vec![]);
    h.app.alert("系統音量","音量 60%",5,false);h.frame(vec![]);
    h.bt=vec![crate::bt::Device{id:"keyboard".into(),name:"Keyboard".into(),battery:None,kind:crate::bt::DeviceKind::Keyboard}];h.frame(vec![]);
    assert_eq!(h.app.notice.as_ref().map(|a|a.title.as_str()),Some("系統音量"));
    assert!(h.app.alerts.iter().any(|a|a.title=="藍芽裝置"));
}

#[test]
fn bluetooth_notices_track_multiple_devices_independently(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    let audio=crate::bt::Device{id:"audio".into(),name:"Shokz".into(),battery:None,kind:crate::bt::DeviceKind::Bone};
    let keyboard=crate::bt::Device{id:"keyboard".into(),name:"QingLong 87".into(),battery:None,kind:crate::bt::DeviceKind::Keyboard};
    h.bt=vec![audio.clone()];h.frame(vec![]);
    h.bt.push(keyboard.clone());h.t=3.0;h.frame(vec![]);
    let notice=h.app.notice.as_ref().unwrap();
    assert_eq!(notice.body,"QingLong 87");
    assert_eq!(notice.bluetooth,Some((crate::bt::DeviceKind::Keyboard,true)));
    h.t=7.0;h.bt=vec![keyboard];h.frame(vec![]);
    let notice=h.app.notice.as_ref().unwrap();
    assert_eq!(notice.body,"Shokz · 已斷開");
    assert_eq!(notice.bluetooth,Some((crate::bt::DeviceKind::Bone,false)));
}

#[test]
fn display_choice_saves_and_keeps_disconnected_selection(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings_open=true;h.ctx.set_embed_viewports(false);
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    h.app.displays=vec![
        crate::display_layout::Display{id:"internal".into(),label:"顯示器 1 · 主屏".into(),bounds,work_top:0,primary:true},
        crate::display_layout::Display{id:r"\\?\DISPLAY#external".into(),label:"顯示器 2 · 外接屏".into(),bounds,work_top:0,primary:false},
    ];
    let mut raster=crate::preview_raster::Raster::new(690,560);
    fn frame(h:&mut H,raster:&mut crate::preview_raster::Raster,events:Vec<egui::Event>){
        h.t+=1.0/60.0;let time=h.t;
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            *capture.lock().unwrap()=Some(ctx.run(egui::RawInput{time:Some(time),viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,560.0))),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()},|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let root=h.ctx.run(egui::RawInput{time:Some(time),..Default::default()},|ctx|h.app.settings_ui(ctx,false));let child=output.lock().unwrap().take().unwrap();
        raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);h.shapes=child.shapes;
    }
    // 設定頁內容超過一屏時先向下滾動，直到目標完整出現在視窗內。
    fn reveal(h:&mut H,raster:&mut crate::preview_raster::Raster,label:&str)->Option<Rect>{
        let find=|h:&H|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None}).filter(|r|r.bottom()<550.0);
        for _ in 0..20 {
            if let Some(r)=find(h){return Some(r);}
            frame(h,raster,vec![egui::Event::PointerMoved(Pos2::new(450.0,300.0)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:Vec2::new(0.0,-60.0),modifiers:Default::default()}]);
            for _ in 0..6{frame(h,raster,vec![]);}
        }
        find(h)
    }
    fn click(h:&mut H,raster:&mut crate::preview_raster::Raster,label:&str){
        let pos=reveal(h,raster,label).map(|r|r.center()).unwrap_or_else(||panic!("未找到：{label}"));
        for pressed in [true,false]{frame(h,raster,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        frame(h,raster,vec![]);
    }
    for _ in 0..3{frame(&mut h,&mut raster,vec![]);}
    assert!(h.app.settings.display_id.is_empty());click(&mut h,&mut raster,"跟隨主屏");
    for _ in 0..20{frame(&mut h,&mut raster,vec![]);}
    if let Some(path)=std::env::var_os("DAOZAI_DISPLAY_PREVIEW"){
        raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),1.0),1.0);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(path,&rgba,690,560,image::ColorType::Rgba8).unwrap();
    }
    click(&mut h,&mut raster,"顯示器 2 · 外接屏");assert_eq!(h.app.settings.display_id,r"\\?\DISPLAY#external");
    assert_eq!(crate::model::Settings::load().display_id,h.app.settings.display_id);
    h.app.displays.pop();frame(&mut h,&mut raster,vec![]);
    reveal(&mut h,&mut raster,"所選螢幕未連線");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="所選螢幕未連線")));
    assert_eq!(crate::display_layout::selected(&h.app.displays,&h.app.settings.display_id).unwrap().id,"internal");
    click(&mut h,&mut raster,"所選螢幕未連線");click(&mut h,&mut raster,"跟隨主屏");
    assert!(crate::model::Settings::load().display_id.is_empty());
}

#[test]
fn closing_app_notification_clears_its_bell_and_keeps_queued_messages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for lyrics in [false,true]{
        let mut h=H::new();h.app.settings.island_lyrics=lyrics;h.app.settings.read_notifications=true;h.app.settings.notification_preview=true;
        if lyrics {
            h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
            h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗"));
        }
        for _ in 0..60{h.frame(vec![]);}
        h.app.receive_notification("測試訊息".into(),"notice.test".into(),"第一條訊息".into());
        for _ in 0..60{h.frame(vec![]);}
        let bell=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Path(p)if p.points.len()==17=>Some(p.visual_bounding_rect().center()),_=>None}).unwrap();
        let close=h.ctx.read_response(egui::Id::new(("card",true,"external-dismiss"))).unwrap().rect;
        assert!(bell.x>close.right(),"通知鈴鐺應位於島內最右側");
        h.app.receive_notification("測試訊息".into(),"notice.test".into(),"第二條訊息".into());
        h.click(close.center());
        assert!(h.app.notice.is_none());assert!(h.app.notification_bell_until<=h.t,"關閉正文也應結束鈴鐺提醒");
        h.frame(vec![]);assert_eq!(h.app.notice.as_ref().unwrap().body,"第二條訊息");
        for _ in 0..30{h.frame(vec![]);}
        h.click_text("關閉");h.cursor=None;
        for _ in 0..60{h.frame(vec![]);}
        assert!(h.app.notice.is_none());assert!(h.app.hud_notice.is_none());
        assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c)if c.fill==egui::Color32::from_rgb(255,72,72))),"關閉後不能殘留鈴鐺紅點");
    }
}

#[test]
fn lyric_app_notification_respects_preview_and_restores_current_lyrics(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for preview in [false,true]{
        let mut h=H::new();h.app.settings.island_lyrics=true;h.app.settings.read_notifications=true;h.app.settings.notification_preview=preview;
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]首句歌詞\n[00:20]後續歌詞"));
        for _ in 0..60{h.frame(vec![]);}
        h.app.receive_notification("測試訊息".into(),"notice.test".into(),"下午一起開會".into());
        h.frame(vec![]);let at=h.app.notice.as_ref().unwrap().at;
        let mut slopes=Vec::new();
        for elapsed in [0.35,0.60,1.0]{
            h.t=at+elapsed;h.frame(vec![]);
            let slope=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Path(p)if p.points.len()==17=>Some(p.points[13].y-p.points[16].y),_=>None}).expect("應繪製通知鈴鐺");
            slopes.push(slope);
        }
        assert!(slopes[0]*slopes[1]<0.0,"鈴鐺應向左右兩個方向擺動");assert!(slopes[2].abs()<0.01,"擺動結束後應回正");
        for _ in 0..60{h.frame(vec![]);}
        let visible=|h:&H,label:&str|h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==label&&t.opacity_factor>0.9&&s.clip_rect.contains_rect(t.visual_bounding_rect())));
        assert_eq!(visible(&h,"下午一起開會"),preview);
        assert_eq!(visible(&h,"首句歌詞"),!preview);
        assert_eq!(h.app.compact_notices(),!preview);
        assert!(h.region.height()<70.0,"通知應留在島內，不能自動彈出下方卡片");
        assert!(!h.app.open);
        if !preview {assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c)if c.fill==egui::Color32::from_rgb(255,72,72))),"通知鈴鐺應有紅點");}
        let active=h.app.notice.as_ref().unwrap();h.t=active.at+active.duration-0.1;h.media.position=25.0;
        h.frame(vec![]);assert!(h.app.notice.is_some());
        for _ in 0..90{
            h.frame(vec![]);
            if h.app.notice.is_none(){assert!(!visible(&h,"Track")&&!visible(&h,"Artist"),"恢復歌詞不能閃出播放器");}
        }
        assert!(h.app.notice.is_none());assert!(!visible(&h,"下午一起開會"));assert!(visible(&h,"後續歌詞"));
    }
}

#[test]
#[ignore="按需匯出歌詞模式下正文開關的通知畫面"]
fn lyric_app_notification_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_APP_NOTICE_PREVIEW").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for preview in [false,true]{
        let mut h=H::new();h.app.settings.island_lyrics=true;h.app.settings.read_notifications=true;h.app.settings.notification_preview=preview;
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗"));
        let mut raster=crate::preview_raster::Raster::new(1440,405);
        for frame in 0..90{
            if frame==30{h.app.receive_notification("微信".into(),"wechat.preview".into(),"下午三點開會，記得帶上電腦".into());}
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),media:h.media.clone(),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(1.5),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);});});
            raster.update_textures(&output.textures_delta);
            if frame==89{
                raster.buf.fill([0.94,0.95,0.97,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw(1440,405,rgba).unwrap();
                image::imageops::crop_imm(&image,105,0,960,150).to_image().save(dest.join(if preview{"正文開啟.png"}else{"正文關閉.png"})).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
fn full_notifications_return_when_island_lyrics_are_not_visible(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (lyrics,music) in [(false,false),(false,true),(true,false)]{
        let mut h=H::new();h.app.settings.pet=true;h.app.settings.island_lyrics=lyrics;
        if music {h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};}
        for _ in 0..90{h.frame(vec![]);}h.app.alert("大寫鎖定","已開啟",5,false);for _ in 0..60{h.frame(vec![]);}
        for label in ["大寫鎖定","已開啟"]{assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==label&&t.opacity_factor>0.9&&s.clip_rect.contains(t.visual_bounding_rect().center()))),"非島上歌詞狀態應直接顯示通知文案：{label}");}
        assert!((h.app.current_width()-crate::app::HUD_W-44.0).abs()<1.0,"完整提示保留吉祥物與文案的寬度");
    }
}

#[test]
fn notification_style_follows_the_current_lyric_mode_without_a_new_notice(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗"));
    h.app.alert("大寫鎖定","已開啟",5,false);
    for (lyrics,playing) in [(false,true),(true,true),(true,false),(true,true),(false,true)]{
        h.app.settings.island_lyrics=lyrics;h.media.playing=playing;for _ in 0..25{h.frame(vec![]);}
        assert_eq!(h.app.compact_notices(),lyrics&&playing);
        let has_title=h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="大寫鎖定"&&t.opacity_factor>0.9));
        assert_eq!(has_title,!(lyrics&&playing),"當前通知應跟隨島上歌詞的顯示狀態切換樣式");
    }
}

#[test]
#[ignore="按需匯出普通模式與島上歌詞模式的通知畫面"]
fn notice_mode_layout_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_NOTICE_MODES_PREVIEW").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for lyrics in [false,true]{
        let mut h=H::new();h.app.settings.pet=true;h.app.settings.island_lyrics=lyrics;h.app.pomo.running=lyrics;
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗"));
        let mut raster=crate::preview_raster::Raster::new(1920,405);
        for frame in 0..120{
            let t=frame as f64/60.0;if frame==30{h.app.alert("大寫鎖定","已開啟",5,false);}
            let env=Env{t,clock:"09:41:00".into(),today:"2026-11-01".into(),date:"11月1日 週日".into(),media:h.media.clone(),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(1.5),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);});});
            raster.update_textures(&output.textures_delta);
            if frame==119{
                raster.buf.fill([0.94,0.95,0.97,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw(1920,405,rgba).unwrap();
                image::imageops::crop_imm(&image,105,0,960,140).to_image().save(dest.join(if lyrics{"島上歌詞模式.png"}else{"普通模式.png"})).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
fn lyric_caps_lock_notice_keeps_one_glass_outline(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for skin in [crate::model::Skin::GlassLight,crate::model::Skin::GlassDark]{
    for shape in crate::model::IslandShape::ALL{for ppp in [1.0,1.25,1.5,2.0]{
        let mut h=H::new();h.app.settings.pet=false;h.app.settings.skin=skin;h.app.settings.island_shape=shape;h.app.settings.island_lyrics=true;h.app.settings.glass_blur=true;
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗"));
        let mut raster=crate::preview_raster::Raster::new((crate::app::CANVAS_W*ppp) as usize,(WIN_H*ppp) as usize);
        for frame in 0..360{
            h.t=frame as f64/60.0;
            if frame==60{h.app.alert("大寫鎖定","已開啟",5,false);}if frame==105{h.app.alert("大寫鎖定","已關閉",5,false);}
            let env=Env{t:h.t,clock:"09:41:00".into(),media:h.media.clone(),backdrop:Some([0.94,0.95,0.97]),glass_blur:true,..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let mut glass=None;
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{glass=h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env).glass;});});
            let contours=glass.unwrap();
            assert_eq!(contours.len(),1,"{skin:?} {shape:?} 倍率 {ppp} 第 {frame} 幀：提示與原島應共用外輪廓，避免拼接處出現內圈");
            if shape==crate::model::IslandShape::Pill{
                let rims=output.shapes.iter().filter(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.stroke.width>0.0&&r.rect.width()>50.0&&r.rect.height()>=20.0)).count();
                assert_eq!(rims,1,"膠囊連線處不能重複繪製玻璃描邊");
            }
            raster.update_textures(&output.textures_delta);let primitives=h.ctx.tessellate(output.shapes,ppp);
            if ppp==1.5{if let Some(name)=match frame{45=>Some("before"),90=>Some("on"),135=>Some("off"),359=>Some("after"),_=>None}{if let Some(dest)=std::env::var_os("DAOZAI_CAPS_LOCK_PREVIEW_DIR"){
                let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();raster.buf.fill([0.94,0.95,0.97,1.0]);raster.draw(&primitives,ppp);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw(raster.w as u32,raster.h as u32,rgba).unwrap();
                image::imageops::crop_imm(&image,(140.0*ppp) as u32,0,(440.0*ppp) as u32,(80.0*ppp) as u32).to_image().save(dest.join(format!("{}-{}-{name}.png",skin.key(),shape.key()))).unwrap();
            }}}
            raster.free_textures(&output.textures_delta);
        }
        assert!(h.app.notice.is_none());assert!((h.app.current_width()-crate::app::LYRIC_BAR_W).abs()<1.0,"提示結束後應恢復原有寬度");
    }}}
}

#[test]
fn lyric_notifications_extend_right_and_keep_the_lyrics_in_place(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let text=|h:&H,label:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label&&t.opacity_factor>0.9&&s.clip_rect.contains(t.visual_bounding_rect().center())=>Some(t.visual_bounding_rect()),_=>None}).unwrap_or_else(||panic!("應保留可見內容：{label}"));
        let mut h=H::new();h.app.settings.pet=true;h.app.settings.island_lyrics=true;h.power=(Some(75),false);
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗\n[01:00]下一句"));
        for _ in 0..90{h.frame(vec![]);}
        let label="晚風吹過了山崗";let original=text(&h,label);let right=WIN_W/2.0+h.app.current_width()/2.0;
        h.app.alert("大寫鎖定","已開啟",5,false);
        for _ in 0..60{h.frame(vec![]);assert!(text(&h,label).center().distance(original.center())<0.2,"提示不能擠動或替換原內容");}
        assert!((h.app.current_width()-(right-WIN_W/2.0)*2.0-44.0).abs()<1.0,"通知圖示佔用 44 點寬度");
        assert!(h.region.right()<right+64.0);assert!(!h.app.open);
        h.t+=3.0;for _ in 0..100{h.frame(vec![]);}
        assert!(text(&h,label).center().distance(original.center())<0.2);
}

#[test]
fn an_open_page_and_lyrics_can_coexist_with_notifications_and_the_bell(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗\n[01:00]下一句"));
    for _ in 0..90{h.frame(vec![]);}
    h.app.notification_apps.items.push(("mail.test".into(),"郵件".into(),true));h.app.settings.read_notifications=true;h.app.receive_notification("郵件".into(),"mail.test".into(),"收到新郵件".into());h.frame(vec![]);
    assert!(h.app.island_lyric().is_some(),"通知鈴鐺不能關閉島上歌詞");
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.hover(H::local(300.0,60.0),0.5);assert!(h.app.open);
    h.app.alert("鍵盤輸入語言","中文",5,false);for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.open);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="Track")),"展開頁面保留歌曲訊息");
    assert!(!h.app.compact_notices(),"展開頁面時不使用島上歌詞的小圖示提示");
}

#[test]
#[ignore="按需錄製右側通知與原島內容同時顯示的實際畫面"]
fn right_notice_preview(){
    use std::io::Write;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_RIGHT_NOTICE_PREVIEW").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    h.app.settings.pet=true;h.app.settings.island_lyrics=true;h.app.pomo.running=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗\n[01:00]下一句"));
    let mut video=std::process::Command::new(std::env::var_os("DAOZAI_FFMPEG").unwrap_or_else(||"ffmpeg".into())).args(["-y","-loglevel","error","-f","rawvideo","-pix_fmt","rgba","-s","1920x405","-r","30","-i","-","-vf","crop=1440:210:105:0","-c:v","libx264","-crf","19","-pix_fmt","yuv420p"]).arg(dest.join("右側通知演示.mp4")).stdin(std::process::Stdio::piped()).spawn().unwrap();
    let mut pipe=video.stdin.take().unwrap();let mut raster=crate::preview_raster::Raster::new(1920,405);
    for frame in 0..420{
        let t=frame as f64/30.0;
        if frame==90{h.app.alert("大寫鎖定","已開啟",5,false);}
        if frame==240{h.app.alert("藍芽裝置","OpenMove by Shokz · 電量 80%",5,false);}
        let env=Env{t,clock:"09:41:00".into(),today:"2026-11-01".into(),date:"11月1日 週日".into(),media:h.media.clone(),..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(1.5),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);});});
        raster.update_textures(&output.textures_delta);raster.buf.fill([0.94,0.95,0.97,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);pipe.write_all(&rgba).unwrap();
        if let Some(name)=match frame{60=>Some("原島"),125=>Some("大小寫提示"),270=>Some("藍芽提示"),390=>Some("恢復原島"),_=>None}{let image=image::RgbaImage::from_raw(1920,405,rgba).unwrap();image::imageops::crop_imm(&image,105,0,1440,210).to_image().save(dest.join(format!("{name}.png"))).unwrap();}
        raster.free_textures(&output.textures_delta);
    }
    drop(pipe);assert!(video.wait().unwrap().success());
}

#[test]
fn hud_expands_smoothly_from_collapsed_width(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    for _ in 0..60 { h.frame(vec![]); }
    let before=h.app.current_width();
    h.app.alert("大寫鎖定","已開啟",5,false);
    h.frame(vec![]);
    assert!(h.app.current_width()-before < 30.0, "提示條不能在第一幀突然跳到目標寬度");
}

#[test]
fn bluetooth_notification_keeps_its_full_message_outside_lyric_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();for _ in 0..90 {h.frame(vec![]);}
    h.app.alert("藍芽裝置","OpenMove by Shokz · 電量 80%",5,false);
    for _ in 0..60 {h.frame(vec![]);}
    assert!((h.app.current_width()-crate::app::HUD_W-h.app.pet_space()).abs()<1.0);
    assert!(h.region.height()>60.0,"兩行藍芽提示保持完整高度");
    h.click_notice_badge();
    for label in ["OpenMove by Shokz","已連線","80%"]{assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==label)),"點選後可檢視裝置訊息：{label}");}
}

#[test]
fn hud_repeated_volume_changes_do_not_restart_entrance(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.frame(vec![]);
    h.app.alert("系統音量","音量 20%",5,false);
    for _ in 0..45 {h.frame(vec![]);}
    let before=h.app.current_width();
    h.app.alert("系統音量","音量 30%",5,false);
    h.frame(vec![]);
    assert!((h.app.current_width()-before).abs()<5.0);
    assert_eq!(h.app.notice.as_ref().unwrap().body,"音量 30%");
}

#[test]
fn hud_shrinks_smoothly_after_expiration(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    for _ in 0..90 {h.frame(vec![]);}let collapsed=h.app.current_width();
    h.app.alert("大寫鎖定","已開啟",5,false);
    for _ in 0..60 {h.frame(vec![]);}
    let before=h.app.current_width();
    h.t+=2.0;
    h.frame(vec![]);
    assert!(h.app.notice.is_none());
    assert!(h.app.current_width()>collapsed+5.0);
    assert!(h.app.current_width()<before);
}

#[test]
fn bluetooth_badge_follows_connect_and_disconnect(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    // 首次掃描已連線的裝置，記錄但不作為新連線提醒。
    h.bt=vec![crate::bt::Device{id:"buds".into(),name:"Buds".into(),battery:Some(80),kind:crate::bt::DeviceKind::Earbuds}];h.frame(vec![]);
    assert!(h.app.notice.is_none());
    assert_eq!(h.app.bt_shown.first().map(|d|d.name.as_str()),Some("Buds"));
    // 之後斷開、再連上，各彈一條 HUD 通知。
    h.t=20.0;h.bt.clear();h.frame(vec![]);
    assert_eq!(h.app.notice.as_ref().map(|a|a.body.as_str()),Some("Buds · 已斷開"));
    assert!(h.app.bt_shown.is_empty());
    h.t=30.0;h.bt=vec![crate::bt::Device{id:"buds".into(),name:"Buds".into(),battery:Some(80),kind:crate::bt::DeviceKind::Earbuds}];h.frame(vec![]);
    assert_eq!(h.app.notice.as_ref().map(|a|a.body.as_str()),Some("Buds · 電量 80%"));
    // 沒有裝置時不留殘留狀態。
    h.t=40.0;h.bt.clear();h.frame(vec![]);
    assert!(h.app.bt_shown.is_empty()&&h.app.notice.as_ref().is_none_or(|a|a.title!="藍芽裝置"||a.body=="Buds · 已斷開"));
}

#[test]
fn focus_settings_are_inside_focus_page(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(1,1.0);h.hover(H::local(300.0,60.0),0.8);h.click(H::local(110.0,18.0));assert_eq!(h.app.page,1);assert_eq!(h.app.subpage,Some(1));
}

#[test]
fn calendar_notice_opens_the_calendar_instead_of_mail() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    assert!(h.app.settings.calendar_auto_sync);
    h.app.settings.calendar_auto_sync=false;
    h.app.settings.save();
    assert!(!crate::model::Settings::load().calendar_auto_sync,"自動同步選擇應持久儲存");
    let start=chrono::Local::now().timestamp()+600;
    h.app.calendar.config=Some(crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"demo".into(),href:None,server:None});
    h.app.calendar_event(crate::calendar::Event::Synced{config:h.app.calendar.config.clone().unwrap(),events:vec![crate::mail::Meeting{ all_day:false, end:None,title:"產品評審".into(),start,at:"10-31 15:00".into(),uid:"calendar-review".into()}],at:"10-31 14:50".into()});
    for _ in 0..60 {h.frame(vec![]);}
    h.click_notice_badge();
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="檢視")),"會議提醒按鈕使用「檢視」");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="產品評審")),"會議名稱應獨立顯示");
    h.click_text("檢視");
    assert!(!h.app.settings_open,"會議提醒不應開啟設定頁");
    assert_eq!(h.app.page,7,"檢視會議必須展開島上的日程頁");
    assert!(h.app.open);
    let date=chrono::DateTime::from_timestamp(start,0).unwrap().with_timezone(&chrono::Local).date_naive();
    assert_eq!(h.app.calendar.view.day,date);
    assert_eq!(h.app.calendar.view.focus.as_deref(),Some(crate::calendar::dedupe_key(&h.app.calendar.events[0])).as_deref());
    assert_ne!(h.app.page,5);
}

#[test]
fn mail_meeting_notice_opens_its_calendar_day() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.mail.recent.push(crate::mail::Item {received_unix: None, account:"demo@example.com".into(),from:"專案組".into(),subject:"會議邀請".into(),html: String::new().into(), summary:String::new().into(),links:vec![],code:None,time:"14:50".into(),uid:9,
        meeting:Some(crate::mail::Meeting{ all_day:false, end:None,title:"產品評審".into(),start:chrono::Local::now().timestamp()+600,at:"10-31 15:00".into(),uid:"mail-review".into()})});
    for _ in 0..60 {h.frame(vec![]);}
    h.click_text("檢視");
    assert_eq!(h.app.page,7);
    assert!(h.app.open);
    assert_ne!(h.app.page,5);
    let entries=h.app.calendar_entries(h.app.calendar.view.day);
    assert_eq!(entries.len(),1);
    assert!(matches!(entries[0].sources[0],crate::calendar_ui::Source::Mail("demo@example.com")));
    assert!(!h.app.settings_open);
}

#[test]
fn calendar_merges_matching_invites_and_preserves_each_source() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let start=chrono::Local::now().timestamp()+600;
    let event=crate::mail::Meeting{ all_day:false, end:None,title:"產品評審".into(),start,at:String::new(),uid:"shared-review".into()};
    h.app.calendar.events=vec![event.clone()];h.app.calendar.view.events=vec![event.clone()];
    for account in ["work@example.com","personal@example.com"] {
        h.app.mail.recent.push(crate::mail::Item {received_unix: None, account:account.into(),from:"專案組".into(),subject:"會議邀請".into(),html: String::new().into(), summary:String::new().into(),links:vec![],code:None,time:String::new(),uid:9,meeting:Some(event.clone())});
    }
    let day=chrono::DateTime::from_timestamp(start,0).unwrap().with_timezone(&chrono::Local).date_naive();
    for provider in crate::calendar::Provider::ALL {
        h.app.calendar.config=Some(crate::calendar::Config{provider,username:"demo".into(),href:None,server:None});
        let entries=h.app.calendar_entries(day);
        assert_eq!(entries.len(),1,"同一場會議只佔一行");
        assert_eq!(entries[0].sources.len(),3,"工作日曆及兩個信箱應保留各自來源");
        assert!(matches!(entries[0].sources[0],crate::calendar_ui::Source::Calendar(Some(p)) if p==provider));
    }
    h.app.calendar.events.clear();h.app.calendar.view.events.clear();
    for (i,item) in h.app.mail.recent.iter_mut().enumerate(){let event=item.meeting.as_mut().unwrap();event.uid.clear();event.title=format!("不同會議 {i}");}
    assert_eq!(h.app.calendar_entries(day).len(),2,"缺少會議編號時不能只按相同時間誤合併");
}

#[test]
fn calendar_dates_are_readable_and_source_labels_do_not_take_a_second_line() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::NaiveDate::from_ymd_opt(2026,11,15).unwrap();
    let event=crate::mail::Meeting{ all_day:false, end:None,title:"產品評審".into(),start:day.and_hms_opt(14,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp(),at:"11-15 14:00".into(),uid:"readable-calendar".into()};
    h.app.calendar.config=Some(crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"demo".into(),href:None,server:None});
    h.app.calendar.view.events=vec![event.clone()];
    h.app.mail.recent.push(crate::mail::Item {received_unix: None, account:"work@example.com".into(),from:"專案組".into(),subject:"會議邀請".into(),html: String::new().into(), summary:String::new().into(),links:vec![],code:None,time:String::new(),uid:1,meeting:Some(event.clone())});
    h.app.open_calendar(Some(event));h.hover(H::local(300.0,40.0),1.0);
    let date=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="15"=>Some(t),_=>None}).expect("月曆應顯示日期");
    assert!(date.galley.job.sections[0].format.font_id.size>=10.0,"日期數字需要明顯放大");
    let date_rect=|label:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None}).expect("日期應可見");
    let (ten,eleven,seventeen)=(date_rect("10"),date_rect("11"),date_rect("17"));
    assert!(eleven.left()-ten.right()>=6.0,"相鄰日期需要橫向留白：{ten:?} {eleven:?}");
    assert!(seventeen.top()-ten.bottom()>=7.0,"相鄰周需要縱向留白：{ten:?} {seventeen:?}");
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="飛書" || t.galley.text().starts_with("郵件 · "))),"來源應使用標題前的 Logo，詳細文字保留在懸停提示中");
}

#[test]
fn calendar_first_schedule_has_a_large_time_and_keeps_the_lower_list_scrollable() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::NaiveDate::from_ymd_opt(2026,10,28).unwrap();
    let start=day.and_hms_opt(10,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp();
    h.app.calendar.view.day=day;h.app.calendar.view.month=crate::calendar::month_start(day);h.app.calendar.view.loaded=Some(h.app.calendar.view.month);
    h.app.calendar.view.events=(0..7).map(|i|crate::mail::Meeting{ all_day:false,title:format!("會議 {i}"),start:start+i*3600,end:Some(start+i*3600+2700),at:String::new(),uid:format!("layout-{i}")}).collect();
    h.app.open_calendar(None);h.hover(H::local(300.0,40.0),1.0);
    let text=|label:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t),_=>None}).unwrap();
    assert!(text("10:00").galley.job.sections[0].format.font_id.size>=23.0);
    assert!(text("會議 0").galley.job.sections[0].format.font_id.size>text("會議 1").galley.job.sections[0].format.font_id.size);
    assert!(text("10:45 結束").visual_bounding_rect().bottom()<text("會議 1").visual_bounding_rect().top());
    assert!(text("會議 4").visual_bounding_rect().bottom()<H::local(0.0,205.0).y);
    h.hover(H::local(300.0,194.0),0.25);h.wheel(-40.0);
    assert_eq!(h.app.calendar.view.first,1,"列表底部也應滾動日程");assert_eq!(h.app.page,7);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="會議 0")),"滾動不能切換大卡片");
    h.click(H::local(260.0,138.0));h.hover(H::local(300.0,194.0),0.2);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="12:00" && t.galley.job.sections[0].format.font_id.size>=23.0)),"點選才切換大卡片");
    h.wheel(-40.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="12:00" && t.galley.job.sections[0].format.font_id.size>=23.0)),"選中後滾動仍保持詳情");
    h.app.open_calendar(Some(h.app.calendar.view.events[6].clone()));h.hover(H::local(300.0,40.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="會議 6")),"提醒定位的末條日程應完整可見");
}

#[test]
fn calendar_month_navigation_and_today_select_the_right_day() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_calendar(None);
    h.app.select_calendar_day(chrono::NaiveDate::from_ymd_opt(2026,12,31).unwrap());
    h.hover(H::local(300.0,60.0),1.0);
    h.click(H::local(129.0,80.0));
    assert_eq!(h.app.calendar.view.month,chrono::NaiveDate::from_ymd_opt(2027,1,1).unwrap());
    let point=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="15"=>Some(t.visual_bounding_rect().center()),_=>None}).expect("日期應顯示在島上");
    h.click(point);
    assert_eq!(h.app.calendar.view.day,chrono::NaiveDate::from_ymd_opt(2027,1,15).unwrap());
    h.click(H::local(67.0,18.0));
    assert_eq!(h.app.calendar.view.day,chrono::NaiveDate::from_ymd_opt(2026,10,28).unwrap());
    assert_eq!(h.app.page,7);assert!(!h.app.settings_open);
}

#[test]
fn calendar_month_results_preserve_reminders_and_ignore_old_requests() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.calendar.config=Some(crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"demo".into(),href:None,server:None});
    let upcoming=crate::mail::Meeting{ all_day:false, end:None,title:"明天會議".into(),start:chrono::Local::now().timestamp()+3600,at:String::new(),uid:"upcoming".into()};
    h.app.calendar.events=vec![upcoming.clone()];
    let month=chrono::NaiveDate::from_ymd_opt(2024,2,1).unwrap();
    h.app.select_calendar_day(month);
    let config=h.app.calendar.config.clone().unwrap();
    h.app.calendar_event(crate::calendar::Event::MonthLoaded{config:config.clone(),month,result:Ok(vec![])});
    assert_eq!(h.app.calendar.events,vec![upcoming]);
    assert_eq!(h.app.calendar.view.loaded,Some(month));
    let next=chrono::NaiveDate::from_ymd_opt(2024,3,1).unwrap();h.app.select_calendar_day(next);
    h.app.calendar_event(crate::calendar::Event::MonthLoaded{config:config.clone(),month,result:Ok(vec![])});
    assert!(h.app.calendar.view.loading,"舊請求不能結束新月份的載入");
    let mut other=config.clone();other.username="another-account".into();
    h.app.calendar_event(crate::calendar::Event::MonthLoaded{config:other,month:next,result:Ok(vec![])});
    assert!(h.app.calendar.view.loading,"其他帳號的舊結果應被忽略");
    h.app.calendar_event(crate::calendar::Event::MonthLoaded{config,month:next,result:Err("連線暫不可用".into())});
    assert!(!h.app.calendar.view.loading);assert!(!h.app.calendar.view.error.is_empty());
}

#[test]
fn calendar_island_collapses_and_reopens_on_the_same_page() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_calendar(None);h.hover(H::local(300.0,60.0),1.0);
    assert_eq!(h.app.page,7);assert!(h.app.open);
    assert!(h.hit.bottom()>=crate::app::CALENDAR_H,"月曆最後一行必須位於可操作區域內");
    h.hover(Pos2::new(600.0,WIN_H+30.0),5.0);assert!(!h.app.open);
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);assert!(h.app.open);
    assert_eq!(h.app.page,7);assert!(!h.app.settings_open);
}

#[test]
#[ignore="按需匯出島內日程的實際渲染畫面"]
fn calendar_layout_gallery() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_CALENDAR_PREVIEW_DIR").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    for mode in ["agenda","dingtalk","wecom","empty","loading","failed","disconnected","local-editor","local","local-all-day","mixed"] {
        let mut h=H::new();
        let day=chrono::NaiveDate::from_ymd_opt(2026,11,2).unwrap();
        h.app.calendar.view.day=day;h.app.calendar.view.month=crate::calendar::month_start(day);
        h.app.open_calendar(None);
        let provider=match mode{"dingtalk"=>crate::calendar::Provider::DingTalk,"wecom"=>crate::calendar::Provider::WeCom,_=>crate::calendar::Provider::Feishu};
        if !["disconnected","local-editor","local","local-all-day"].contains(&mode){h.app.calendar.config=Some(crate::calendar::Config{provider,username:"demo".into(),href:None,server:None});h.app.calendar.connected=true;}
        h.app.calendar.view.loaded=Some(crate::calendar::month_start(day));
        if ["agenda","dingtalk","wecom","mixed"].contains(&mode) {
            for (i,(hour,minute,title)) in [(14,30,"產品方案評審"),(16,0,"下一階段的設計方案與開發排期對齊"),(17,0,"專案進度同步")].into_iter().enumerate(){
                let at=day.and_hms_opt(hour,minute,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap();
                h.app.calendar.view.events.push(crate::mail::Meeting{ all_day:false,end:Some(at.timestamp()+2700),title:title.into(),start:at.timestamp(),at:at.format("%m-%d %H:%M").to_string(),uid:format!("sample-{i}")});
            }
            for d in [4,9,12,18,20,25] {let date=chrono::NaiveDate::from_ymd_opt(2026,11,d).unwrap();h.app.calendar.view.events.push(crate::mail::Meeting{ all_day:false, end:None,title:"專案安排".into(),start:date.and_hms_opt(10,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp(),at:String::new(),uid:format!("other-{d}")});}
            for (i,(hour,title,account)) in [(15,"客戶溝通：合同條款確認","work@example.com"),(16,"交付驗收會議","team@example.com")].into_iter().enumerate(){
                let at=day.and_hms_opt(hour,30,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap();
                h.app.mail.recent.push(crate::mail::Item {received_unix: None, account:account.into(),from:"專案組".into(),subject:"會議邀請".into(),html: String::new().into(), summary:String::new().into(),links:vec![],code:None,time:String::new(),uid:i as u32,
                    meeting:Some(crate::mail::Meeting{ all_day:false, end:None,title:title.into(),start:at.timestamp(),at:at.format("%m-%d %H:%M").to_string(),uid:format!("mail-{i}")})});
            }
            h.app.mail.recent.push(crate::mail::Item {received_unix: None, account:"work@example.com".into(),from:"專案組".into(),subject:"會議邀請".into(),html: String::new().into(), summary:String::new().into(),links:vec![],code:None,time:String::new(),uid:3,meeting:Some(h.app.calendar.view.events[1].clone())});
        }
        if ["local","local-all-day","mixed"].contains(&mode){
            let mut draft=crate::local_calendar::Draft::new(day);draft.title="散步與讀書".into();draft.start_time="15:00".into();draft.end_time="16:00".into();
            draft.all_day=mode!="local";h.app.calendar.local.push(draft.entry().unwrap());
        }
        if mode=="mixed" {
            let start=day.and_hms_opt(9,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp();
            h.app.calendar.view.events.push(crate::mail::Meeting{all_day:false,title:"早上的會議".into(),start,end:Some(start+3600),at:String::new(),uid:"past".into()});
        }
        if mode=="local-editor"{h.app.open_calendar_editor(None);}
        if mode=="loading"{h.app.calendar.view.loaded=None;h.app.calendar.view.loading=true;}
        if mode=="failed"{h.app.calendar.view.loaded=None;h.app.calendar.view.error="連不上伺服器，檢查網路".into();}
        let mut raster=crate::preview_raster::Raster::new(1280,600);
        for f in 0..90 {
            let t=f as f64/60.0;
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(WIN_W,WIN_H))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let env=Env{t,today:"2026-11-02".into(),date:"11月2日 週一".into(),clock:"14:15:00".into(),battery:Some(76),charging:true,cursor:Some(H::local(300.0,40.0)),..Default::default()};
            let out=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});});
            raster.update_textures(&out.textures_delta);
            if f==89 {
                if mode=="local-all-day" {
                    assert!(out.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="散步與讀書")),"只有全天日程時也必須顯示普通日程行");
                }
                raster.buf.fill([0.035,0.055,0.09,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw(1280,600,rgba).unwrap();
                image::imageops::crop_imm(&image,180,0,920,510).to_image().save(dest.join(format!("calendar-{mode}.png"))).unwrap();
            }
            raster.free_textures(&out.textures_delta);
        }
    }
}

#[test]
fn calendar_reminder_fires_once_and_dedupes_with_mail() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"a@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.calendar.config=Some(crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"demo".into(),href:None,server:None});
    h.app.settings.calendar_prompt = false;
    let start = chrono::Local::now().timestamp() + 600;
    let at = "10-30 10:00".to_string();
    let meeting = |title: &str| crate::mail::Meeting { all_day:false, end:None, title: title.into(), start, at: at.clone(), uid: "evt-shared-1".into() };
    // 郵件裡先來一份，日曆再來同一個 UID + DTSTART：只彈一次。
    h.app.mail_event(crate::mail::Event::Mail {
        item: crate::mail::Item {
            received_unix: None, account: "a@qq.com".into(),
            from: "騰訊企業郵".into(),
            subject: "週會".into(),
            time: "09:41".into(),
            code: None,
            html: String::new().into(), summary: String::new().into(),
            links: vec![],
            meeting: Some(meeting("週會")),
            uid: 7,
        },
        new: true,
    });
    h.app.calendar_event(crate::calendar::Event::Synced {
        config:h.app.calendar.config.clone().unwrap(),
        events: vec![meeting("週會（飛書）")],
        at: "10-30 09:00".into(),
    });
    h.frame(vec![]);
    h.frame(vec![]);
    let hits = h.app.history.iter().filter(|a| a.title == "會議提醒").count();
    assert_eq!(hits, 1, "同一場會只提醒一次");
    assert_eq!(h.app.history.iter().find(|a|a.title=="會議提醒").unwrap().page,7,"會議提醒應指向島內日程頁");
    // 再同步一次、再過幾幀，也不重複。
    h.app.calendar_event(crate::calendar::Event::Synced { config:h.app.calendar.config.clone().unwrap(),events: vec![meeting("週會（飛書）")], at: "10-30 09:05".into() });
    h.frame(vec![]);
    assert_eq!(h.app.history.iter().filter(|a| a.title == "會議提醒").count(), 1);
}

#[test]
fn calendar_setup_does_not_overlay_music() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.settings.calendar_prompt = true;
    h.app.calendar.config = None;
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 1.0);
    h.hover(H::local(300.0, 60.0), 0.6);
    h.click(H::local(40.0, 16.0));
    assert!(!h.app.settings_open, "音樂頁不應出現日曆接入懸浮入口");
    h.app.open_settings_category(3);
    assert!(h.app.settings_open, "仍可從設定訪問日程");
    assert_eq!(h.app.settings_tab, 6, "日程位於通知、郵件與日程分組");
}

#[test]
fn language_notice_text_is_vertically_centered() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.frame(vec![]);
    h.app.alert("鍵盤輸入語言","英語",5,false);
    for _ in 0..60 {h.frame(vec![]);}
    h.click_notice_badge();
    fn find(shapes:&[egui::epaint::ClippedShape])->Option<Rect>{
        shapes.iter().find_map(|s|match &s.shape {
            egui::Shape::Text(text) if text.galley.text()=="英語"=>Some(text.visual_bounding_rect()),
            _=>None,
        })
    }
    let text=find(&h.shapes).expect("輸入語言狀態應可見");
    assert!((text.center().y-19.0).abs()<1.0,"文字應與圖示垂直居中：{text:?}");
}

#[test]
fn todo_notice_view_locates_the_reminded_task() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.todos.items=(0..7).map(|i|crate::model::TodoItem{text:format!("任務 {i}"),done_on:None,due:if i==6{Some("2026-10-28 09:40".into())}else{None},reminded:false,..Default::default()}).collect();
    for _ in 0..60 {h.frame(vec![]);}
    h.click_text("檢視");
    assert_eq!(h.app.page,4);
    assert!(h.app.todo_first>=4,"開啟待辦頁時應能看到收到提醒的任務");
}

#[test]
fn mail_hyperlink_click_requests_the_default_browser() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"a@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![crate::mail::Item {received_unix: None, account:"a@qq.com".into(),from:"產品團隊".into(),subject:"評審檔案".into(),time:"09:41".into(),code:None,html: String::new().into(), summary:"請檢視以下檔案。".into(),links:vec![crate::mail::Link{label:"檢視檔案".into(),url:"https://example.com/review?a=1&b=2".into()}],meeting:None,uid:10}];
    h.app.mail.open_uid=Some(("a@qq.com".into(),10));h.app.show_page(5);
    h.hover(H::local(300.0,60.0),1.0);
    let link=h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text().contains("檢視檔案")=>Some(text.visual_bounding_rect()),_=>None}).expect("應展示郵件的超連結");
    h.click(link.center());
    assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenUrl(url)if url=="https://example.com/review?a=1&b=2")),"點選後應透過預設瀏覽器開啟完整地址");
}

#[test]
fn new_notification_sways_twice_then_stops_and_restarts_on_the_next_one() {
    for (i,expected_positive) in [true,false,true,false].into_iter().enumerate(){
        let angle=crate::paint::bell_swing(0.95*(i as f64*0.25+0.125));
        assert_eq!(angle>0.0,expected_positive);
    }
    assert_eq!(crate::paint::bell_swing(1.0),0.0);
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.settings.read_notifications=true;h.app.receive_notification("測試通知".into(),"test.notification".into(),"第一條".into());
    let first=h.app.notification_bell_at;
    for _ in 0..90{h.frame(vec![]);}
    h.app.settings.read_notifications=true;h.app.receive_notification("測試通知".into(),"test.notification".into(),"第二條".into());
    assert!(h.app.notification_bell_at>first+1.0);
    h.frame(vec![]);
    assert!(h.app.notification_bell_until>h.t);
}

#[test]
fn hud_titles_and_states_share_the_icon_center_without_overlapping() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (title,body,left,right) in [
        ("網路連線","Wi-Fi · 網路已連線","Wi-Fi","網路已連線"),
        ("網路連線","乙太網 · 網路已斷開","乙太網","網路已斷開"),
        ("大寫鎖定","已開啟","大寫鎖定","已開啟"),
        ("鍵盤輸入語言","中文","輸入語言","中文"),
        ("電源已連線","當前電量 76%","電源已連線","當前電量 76%"),
        ("已切換電池供電","當前電量 76%","電池供電","當前電量 76%"),
    ] {
        let mut h=H::new();h.frame(vec![]);h.app.alert(title,body,5,false);
        for _ in 0..60{h.frame(vec![]);}
        h.click_notice_badge();
        let find=|needle:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==needle=>Some(t.visual_bounding_rect()),_=>None}).unwrap_or_else(||panic!("未顯示狀態文字：{needle}"));
        let l=find(left);let r=find(right);
        assert!((l.center().y-19.0).abs()<1.0 && (r.center().y-19.0).abs()<1.0,"{title} 應垂直居中");
        assert!(l.right()+8.0<=r.left(),"{title} 的標題和狀態應留出間距");
    }
}

#[test]
fn low_power_notice_dismisses_without_opening_the_mail_page() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.frame(vec![]);h.app.alert("電量偏低","剩餘 18% · 請連線電源",5,false);
    for _ in 0..60{h.frame(vec![]);}
    h.click_text("知道了");
    assert!(h.app.notice.is_none());
    assert_eq!(h.app.page,0,"低電量提示只需確認關閉");
}

#[test]
fn code_copy_confirmation_stays_below_the_island_and_does_not_capture_hover() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for detail in [false,true] {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"a@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        h.app.mail.recent=vec![crate::mail::Item {received_unix: None, account:"a@qq.com".into(),from:"GitHub".into(),subject:"登入驗證碼".into(),time:"09:41".into(),code:Some("482913".into()),html: String::new().into(), summary:"請勿向他人透露驗證碼。".into(),links:vec![],meeting:None,uid:42}];
        for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}
        if detail {h.app.mail.open_uid=Some(("a@qq.com".into(),42));}
        h.app.show_page(5);h.hover(H::local(300.0,60.0),4.2);
        let copy=H::local(80.0,if detail{212.0}else{70.0});
        h.click(copy);h.hover(copy,0.35);
        let confirmation=|h:&H|h.shapes.iter().find_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if text.galley.text()=="驗證碼已複製"=>Some(text.visual_bounding_rect()),_=>None,
        });
        let bounds=confirmation(&h).expect("複製後應顯示確認提示");
        assert!(bounds.top()>PH+4.0,"確認提示應在島下方：{bounds:?}");
        assert!(h.region.contains_rect(bounds),"原生視窗區域應包含提示，避免被裁掉");
        assert!(!h.hit.intersects(bounds),"確認提示不應攔截滑鼠或維持島的展開");
        h.hover(Pos2::new(600.0,180.0),0.8);
        assert!(h.app.progress()<0.02);
        assert!(confirmation(&h).is_some(),"島收起後確認提示仍應顯示到正常結束");
        h.hover(Pos2::new(600.0,180.0),2.0);
        assert!(confirmation(&h).is_none());
        assert!(h.region.bottom()<100.0,"提示結束後應釋放額外視窗區域");
    }
}

#[test]
fn adding_a_todo_saves_content_then_offers_an_optional_reminder() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for remind in [false,true] {
        let mut h=H::new();h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);
        h.click(H::local(70.0,109.0));h.frame(vec![egui::Event::Text("提交產品方案".into())]);
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.todos.items[0].text,"提交產品方案");
        assert!(h.app.todos.items[0].due.is_none());
        assert!(h.app.task_dialog.is_some(),"儲存內容後應進入可選提醒步驟");
        assert_eq!(crate::model::TodoList::load().items[0].text,"提交產品方案");
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        if remind {
            let initial=chrono::NaiveDateTime::parse_from_str(&h.app.task_dialog.as_ref().unwrap().picker.due(),"%Y-%m-%d %H:%M").unwrap();
            h.hover(H::local(235.0,77.0),0.4);h.wheel(-40.0);h.picker_click("完成");
            let expected=(initial+chrono::Duration::days(1)).format("%Y-%m-%d %H:%M").to_string();
            assert_eq!(h.app.todos.items[0].due.as_deref(),Some(expected.as_str()));
        }else{h.picker_click("暫不提醒");assert!(h.app.todos.items[0].due.is_none());}
        assert!(h.app.task_dialog.is_none());
        assert!(h.app.todos.items[0].done_on.is_none());
        assert_eq!(crate::model::TodoList::load().items[0].due,h.app.todos.items[0].due);
    }
}

#[test]
fn todo_clock_button_sets_a_reminder_without_completing_the_task() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=vec![crate::model::TodoItem{text:"已有任務".into(),done_on:None,due:None,reminded:false,..Default::default()}];
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(343.0,46.0));
    assert!(h.app.task_dialog.is_some(),"每項待辦應有可見的提醒入口");
    assert!(h.app.todos.items[0].done_on.is_none(),"時鐘入口不能同時勾選任務");
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert!(h.app.task_dialog.is_none());assert_eq!(h.app.todos.items[0].text,"已有任務");
}

#[test]
fn mail_details_show_the_receiving_account_when_two_accounts_share_a_uid() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"first@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true},crate::mail::Account {custom: None, email:"work@icloud.com".into(),provider:crate::mail::Provider::ICloud,enabled:true}];
    h.app.mail.recent=["first@qq.com","work@icloud.com"].into_iter().map(|account|crate::mail::Item {received_unix: None, account:account.into(),from:"產品團隊".into(),subject:"評審安排".into(),time:"09:41".into(),code:None,html: String::new().into(), summary:"請檢視會議安排。".into(),links:vec![],meeting:None,uid:42}).collect();
    h.app.show_page(5);h.hover(H::local(300.0,60.0),1.0);
    for (row,account) in ["first@qq.com","work@icloud.com"].into_iter().enumerate(){
        h.hover(H::local(280.0,64.0+row as f32*21.5),0.6);h.click(H::local(280.0,64.0+row as f32*21.5));h.hover(H::local(300.0,90.0),0.1);
        assert_eq!(h.app.mail.open_uid,Some((account.into(),42)));
        assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()==format!("收件信箱：{account}"))),"詳情必須明確標出這封郵件所屬的收件信箱");
        h.click(H::local(350.0,48.0));
    }
}

#[test]
fn scrolling_mail_details_keeps_the_current_message_open() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![crate::mail::Item {received_unix: None, account:"work@qq.com".into(),from:"產品團隊".into(),subject:"專案方案".into(),time:"09:41".into(),code:None,html: String::new().into(), summary:"需要滾動閱讀的郵件正文。".repeat(20).into(),links:vec![],meeting:None,uid:42}];
    h.app.mail.open_uid=Some(("work@qq.com".into(),42));h.app.show_page(5);h.hover(H::local(270.0,150.0),1.0);
    let body_clip=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text().starts_with("需要滾動閱讀")=>Some(s.clip_rect),_=>None}).expect("正文應顯示");
    assert!(body_clip.height()>=80.0,"詳情正文應有足夠閱讀空間，而不是僅能容納一行");
    h.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-120.0),modifiers:Default::default()}]);
    assert_eq!(h.app.page(),5,"郵件正文滾動不能切換功能頁");
    assert_eq!(h.app.mail.open_uid,Some(("work@qq.com".into(),42)));
}

#[test]
fn todo_reminder_picker_is_embedded_in_the_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=vec![crate::model::TodoItem{text:"準備會議".into(),done_on:None,due:None,reminded:false,..Default::default()}];
    h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);
    h.ctx.set_embed_viewports(false);
    egui::Context::set_immediate_viewport_renderer(|_,_|panic!("待辦提醒必須在島內選擇，不能建立獨立視窗"));
    h.click(H::local(343.0,46.0));
    for _ in 0..90 {h.frame(vec![]);}
    assert_eq!(h.app.expanded_height(),PH,"右側滾輪應保持待辦頁原來的高度");
    assert!(h.hit.height()<=PH+2.0,"選擇時間不能撐大島體");
    for label in ["準備會議","已完成","取消","完成","暫不提醒"] {
        let (rect,clip)=h.shapes.iter().find_map(|s|match &s.shape {
            egui::Shape::Text(t) if t.galley.text()==label=>Some((t.visual_bounding_rect(),s.clip_rect)),_=>None,
        }).unwrap_or_else(||panic!("島內缺少選項：{label}"));
        assert!(h.hit.contains_rect(rect),"選項 {label} 應在島體命中區域內");
        assert!(h.region.contains_rect(rect) && clip.contains_rect(rect),"選項 {label} 不能被視窗或島體裁剪");
    }
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert!(h.app.task_dialog.is_none());assert!(h.app.todos.items[0].due.is_none());
}

#[test]
fn reminder_wheels_select_date_hour_and_minute_without_typing(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::Local::now().date_naive()+chrono::Duration::days(2);let original=format!("{day} 09:00");
    h.app.todos.items=vec![crate::model::TodoItem{text:"準備方案".into(),due:Some(original.clone()),done_on:None,reminded:false,..Default::default()}];
    h.app.open_task_reminder(0);
    for x in [235.0,290.0,346.0] {h.hover(H::local(x,77.0),0.4);h.wheel(-40.0);}
    assert_eq!(h.app.todos.items[0].due,Some(original),"調整時不能提前寫入提醒");
    h.picker_click("完成");
    let next=day+chrono::Duration::days(1);
    assert_eq!(h.app.todos.items[0].due,Some(format!("{next} 10:01")));
    assert!(h.app.task_dialog.is_none());
}

#[test]
fn reminder_wheels_support_clicks_and_dragging(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::Local::now().date_naive()+chrono::Duration::days(2);
    h.app.todos.items=vec![crate::model::TodoItem{text:"確認方案".into(),due:Some(format!("{day} 10:30")),done_on:None,reminded:false,..Default::default()}];
    h.app.open_task_reminder(0);h.hover(H::local(300.0,60.0),1.0);
    h.click(H::local(346.0,96.0));h.click(H::local(290.0,58.0));
    let start=H::local(235.0,77.0);h.cursor=Some(start);
    h.frame(vec![egui::Event::PointerMoved(start),egui::Event::PointerButton{pos:start,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    for y in [59.0,41.0]{let point=H::local(235.0,y);h.cursor=Some(point);h.frame(vec![egui::Event::PointerMoved(point)]);}
    let end=H::local(235.0,41.0);
    h.frame(vec![egui::Event::PointerButton{pos:end,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    h.picker_click("完成");
    assert_eq!(h.app.todos.items[0].due,Some(format!("{} 09:31",day+chrono::Duration::days(2))));
    assert!(h.app.todos.items[0].done_on.is_none());
}

#[test]
fn reminder_wheels_accumulate_small_trackpad_movements(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::Local::now().date_naive()+chrono::Duration::days(2);
    h.app.todos.items=vec![crate::model::TodoItem{text:"檢查進度".into(),due:Some(format!("{day} 10:30")),done_on:None,reminded:false,..Default::default()}];
    h.app.open_task_reminder(0);h.hover(H::local(346.0,77.0),1.0);
    for _ in 0..80{h.wheel(-0.5);}
    h.picker_click("完成");
    assert_eq!(h.app.todos.items[0].due,Some(format!("{day} 10:31")));
    assert_eq!(h.app.page(),4,"操作時間滾輪不能觸發翻頁");
}

#[test]
fn reminder_picker_rejects_past_time_and_cancel_preserves_the_task(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let old=format!("{} 09:00",chrono::Local::now().date_naive()-chrono::Duration::days(1));
    h.app.todos.items=vec![crate::model::TodoItem{text:"保留任務".into(),due:Some(old.clone()),done_on:None,reminded:true,..Default::default()}];
    h.app.open_task_reminder(0);h.picker_click("完成");h.picker_frame(vec![]);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="請選擇晚於當前的時間")));
    assert_eq!(h.app.todos.items[0].due,Some(old.clone()));
    h.picker_click("取消");assert!(h.app.task_dialog.is_none());assert_eq!(h.app.todos.items[0].due,Some(old));
}

#[test]
fn full_mail_reader_keeps_the_entire_body_and_scrolls_independently(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let body=format!("開頭：這是第一段。\n\n{}\n最後一段：全文結束", "這是一段需要閱讀的完整郵件正文。\n\n".repeat(200));
    let html=body.split('\n').map(|line|format!("<p>{line}</p>")).collect::<String>();
    let item=crate::mail::Item {received_unix: None, account:"work@qq.com".into(),from:"產品團隊".into(),subject:"完整方案".into(),time:"09:41".into(),code:None,html:html.into(), summary:body.clone().into(),links:vec![],meeting:None,uid:42};
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:item.account.clone(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![item];h.app.mail.open_uid=Some(("work@qq.com".into(),42));h.app.show_page(5);h.hover(H::local(300.0,90.0),1.0);
    h.click(H::local(293.0,48.0));assert_eq!(h.app.mail.reading.as_ref().unwrap().summary.as_ref(),body);
    h.app.mail.recent.clear();
    let render=|h:&mut H,events|{
        h.ctx.set_embed_viewports(true);h.t+=1.0/60.0;
        let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(700.0,620.0))),time:Some(h.t),events,focused:true,..Default::default()},|ctx|h.app.mail_reader_ui(ctx));h.shapes=out.shapes;
    };
    // 正文按塊渲染：視口外的塊不生成文字形狀，所以「末尾可見」只能滾動到底再斷言。
    let text_top=|h:&H,needle:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==needle=>Some(t.visual_bounding_rect().top()),_=>None});
    let wheel=|dy:f32|vec![egui::Event::PointerMoved(egui::pos2(350.0,300.0)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,dy),modifiers:Default::default()}];
    render(&mut h,vec![]);render(&mut h,vec![]);
    let first=text_top(&h,"開頭：這是第一段。").expect("首屏應顯示正文開頭");
    assert!(text_top(&h,"最後一段：全文結束").is_none(),"長郵件末尾應在首屏之外");
    for _ in 0..5{render(&mut h,wheel(-400.0));}
    let moved=text_top(&h,"開頭：這是第一段。");
    assert!(moved.is_none_or(|top|top<first-100.0),"閱讀視窗的滾輪應向下滾動正文");
    for _ in 0..60{render(&mut h,wheel(-400.0));}
    assert!(text_top(&h,"最後一段：全文結束").is_some(),"滾動到底應能看到郵件末尾");
    assert!(h.app.mail.reading.is_some(),"新郵件列表變化不能打斷已經開啟的正文");
}

#[test]
#[ignore="按需匯出多格式郵件正文的實際畫面"]
fn mail_reader_rich_html_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_MAIL_GALLERY_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let mut h=H::new();h.ctx.set_embed_viewports(true);
    let html=r#"<html><head><style>body{font-family:Arial}</style></head><body>
        <div style="display:none;max-height:0;overflow:hidden">&#8203;&#8203; 這是一段只為收件箱預覽存在的填充文字 &#8203;</div>
        <table role="presentation" cellpadding="0" cellspacing="0" width="100%"><tr><td><img src="https://example.com/logo.png" alt="logo"> 通知中心</td><td>在瀏覽器中檢視</td></tr></table>
        <h1>訂單已確認</h1>
        <p>您好，<strong>訂單 A20261107</strong> 已支付成功，我們會在 24 小時內發貨。</p>
        <p><img src="https://example.com/order.png" alt="訂單物流狀態截圖" width="600"></p>
        <h3>訂單明細</h3>
        <table><tr><th>商品</th><th>數量</th><th>金額</th></tr><tr><td>機械鍵盤</td><td>1</td><td>¥499</td></tr><tr><td>腕託</td><td>2</td><td>¥78</td></tr></table>
        <h3>接下來會發生什麼</h3>
        <ol><li>倉庫揀貨並出庫</li><li>快遞攬收後簡訊通知</li><li>簽收後 7 天內可申請退換</li></ol>
        <ul><li>發票為電子發票，發貨後發到本信箱</li><li>如需修改地址，請在發貨前回複本郵件</li></ul>
        <blockquote>注意：本郵件由系統自動傳送，請勿直接回復。</blockquote>
        <hr>
        <p>感謝支援 · <a href="https://example.com/orders">檢視訂單詳情</a></p>
    </body></html>"#;
    h.app.mail.reading=Some(crate::mail::Item{received_unix:None,account:"reader@example.com".into(),from:"通知中心".into(),
        subject:"訂單 A20261107 已確認".into(),time:"11-07 09:41".into(),code:None,
        html:html.into(),
        summary:"訂單已確認 您好，訂單 A20261107 已支付成功，我們會在 24 小時內發貨。訂單明細 商品 數量 金額 機械鍵盤 1 ¥499 腕託 2 ¥78".into(),
        links:vec![crate::mail::Link{label:"檢視訂單詳情".into(),url:"https://example.com/orders".into()}],meeting:None,uid:7});
    let (w,height)=(700,620);let ppp=1.5;
    let mut raster=crate::preview_raster::Raster::new((w as f32*ppp)as usize,(height as f32*ppp)as usize);
    for f in 0..=8{
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(w as f32,height as f32))),time:Some(f as f64/60.0),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let out=h.ctx.run(raw,|ctx|h.app.mail_reader_ui(ctx));raster.update_textures(&out.textures_delta);
        if f==8{raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join("mail-reader-rich.png"),&rgba,(w as f32*ppp)as u32,(height as f32*ppp)as u32,image::ColorType::Rgba8).unwrap();}
        raster.free_textures(&out.textures_delta);
    }
}

#[test]
fn mail_plain_text_keeps_literal_stars_in_the_reader(){
    let h=H::new();h.ctx.set_embed_viewports(true);
    let lines=crate::mail_format::lines("","保留 ** 原始字元");
    let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(700.0,620.0))),..Default::default()},|ctx|{
        egui::CentralPanel::default().show(ctx,|ui|crate::mail_reader::body_ui(ui,&lines,14.0));
    });
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="保留 ** 原始字元")));
}

#[test]
fn mail_detail_and_reader_keep_their_own_cached_body(){
    let mut h=H::new();
    let item=|uid,body:&str|crate::mail::Item{html:format!("<p>{body}</p>").into(),summary:body.to_string().into(),uid,
        account:"test@example.com".into(),from:String::new(),subject:String::new(),time:String::new(),code:None,received_unix:None,links:vec![],meeting:None};
    let first=item(1,"第一封郵件");let second=item(2,"第二封郵件");
    let before=h.app.mail_body_lines(&first);h.app.mail_body_lines(&second);
    let after=h.app.mail_body_lines(&first);
    assert!(std::sync::Arc::ptr_eq(&before,&after),"詳情與獨立閱讀視窗不能讓另一封郵件的排版快取每幀失效");
}

#[test]
fn mail_html_headings_scale_with_the_body_font(){
    let h=H::new();let lines=crate::mail_format::lines("<h1>縮放標題</h1>","");
    for (base,expected) in [(14.0,18.0),(28.0,36.0)]{
        let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(700.0,620.0))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|crate::mail_reader::body_ui(ui,&lines,base));
        });
        let size=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="縮放標題"=>Some(t.galley.job.sections[0].format.font_id.size),_=>None}).unwrap();
        assert_eq!(size,expected,"正文縮放時標題應保持相同的字號比例");
    }
}

#[test]
fn mail_microsoft_settings_only_explains_unsupported_access(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_mail_settings();h.ctx.set_embed_viewports(false);
    h.app.mail.accounts.clear();h.app.mail.form.provider=crate::mail::Provider::Microsoft;
    let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1300.0))),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    for _ in 0..3{let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
    let child=output.lock().unwrap().take().unwrap();
    let texts=child.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t.galley.text()),_=>None}).collect::<Vec<_>>();
    assert!(texts.iter().any(|t|t.contains("OAuth")&&t.contains("暫不支援")),"應明確說明微軟信箱的支援狀態");
    for absent in ["使用 Microsoft 登入","可留空，在瀏覽器選擇帳號","輸入信箱授權碼","測試連線","儲存並開始監控","應用 ID"]{
        assert!(!texts.iter().any(|t|t.contains(absent)),"暫不支援時不應出現誤匯入口：{absent}");
    }
    assert!(h.app.mail.accounts.is_empty());
    assert!(!h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::MailTest(..)|crate::app::Cmd::OpenUrl(_))));
}

#[test]
fn mail_microsoft_custom_host_and_address_disable_password_actions(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_mail_settings();h.ctx.set_embed_viewports(false);h.app.mail.accounts.clear();
    let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
    let events=std::sync::Arc::new(Mutex::new(Vec::new()));let input=events.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,focused:true,events:std::mem::take(&mut *input.lock().unwrap()),
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1300.0))),
            viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    for (provider,email,host) in [(crate::mail::Provider::Custom,"student@example.edu","outlook.office365.com"),(crate::mail::Provider::Qq,"user@hotmail.com","")]{
        h.app.mail.form.provider=provider;h.app.mail.form.email=email.into();h.app.mail.form.custom.host=host.into();
        h.app.mail.form.password.clear();h.app.mail.note.clear();
        for _ in 0..3{let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
        let child=output.lock().unwrap().take().unwrap();
        assert!(child.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==crate::mail::MICROSOFT_UNSUPPORTED)));
        for label in ["測試連線","儲存並開始監控"]{
            let point=child.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap();
            for pressed in [true,false]{
                *events.lock().unwrap()=vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}];
                let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
            }
            assert!(h.app.mail.note.is_empty(),"微軟雲連線的按鈕應停用，點選不應再提示填寫授權碼");
        }
        assert!(h.app.mail.accounts.is_empty());
        assert!(!h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::MailTest(..)|crate::app::Cmd::MailSync)));
    }
}

#[test]
#[ignore="按需匯出微軟信箱暫不支援提示的實際畫面"]
fn mail_microsoft_settings_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_MAIL_GALLERY_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for custom in [false,true]{
        let mut h=H::new();h.app.open_mail_settings();h.ctx.set_embed_viewports(false);
        h.app.mail.accounts.clear();
        h.app.mail.form.provider=if custom{crate::mail::Provider::Custom}else{crate::mail::Provider::Microsoft};
        h.app.mail.form.email="student@example.edu".into();h.app.mail.form.custom.host="outlook.office365.com".into();
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1300.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.5),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let mut raster=crate::preview_raster::Raster::new(1035,1950);
        for frame in 0..4{
            let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);
            let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
            if frame==3{
                raster.buf.fill([0.08,0.08,0.09,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(if custom{"microsoft-custom-unsupported.png"}else{"microsoft-unsupported.png"}),&rgba,1035,1950,image::ColorType::Rgba8).unwrap();
            }raster.free_textures(&child.textures_delta);
        }
    }
}

#[test]
#[ignore="按需匯出提醒選擇器與郵件全文的實際畫面"]
fn reminder_and_reader_layout_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_READER_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for picker in [true,false]{
        let mut h=H::new();h.ctx.set_embed_viewports(true);
        h.app.todos.items=vec![crate::model::TodoItem{text:"提交產品方案".into(),due:None,done_on:None,reminded:false,..Default::default()}];h.app.open_task_reminder(0);
        let body=(1..=12).map(|n|format!("第 {n} 項 · 工作安排\n請確認需求與交付範圍，整理設計方案和本週待辦，並在評審前同步進度。相關意見請彙總後反饋給產品團隊。\n\n")).collect::<String>();
        h.app.mail.reading=Some(crate::mail::Item {received_unix: None, account:"work@icloud.com".into(),from:"產品團隊".into(),subject:"專案方案與下階段工作安排".into(),time:"10-31 09:41".into(),code:None,html: String::new().into(), summary:format!("各位好，\n\n以下是專案安排與交付要求，請逐項確認。\n\n{body}最後一段\n請在週五前確認，感謝配合。").into(),links:vec![crate::mail::Link{label:"檢視專案資料".into(),url:"https://example.com/project".into()}],meeting:None,uid:42});
        let (w,height)=if picker{(WIN_W as u32,WIN_H as u32)}else{(700,620)};let ppp=1.5;
        let mut raster=crate::preview_raster::Raster::new((w as f32*ppp)as usize,(height as f32*ppp)as usize);
        let last=if picker{89}else{7};
        for f in 0..=last{
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(w as f32,height as f32))),time:Some(f as f64/60.0),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let out=h.ctx.run(raw,|ctx|if picker{
                let env=Env{t:f as f64/60.0,clock:"09:41:00".into(),today:chrono::Local::now().date_naive().to_string(),date:"週日".into(),..Default::default()};
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});
            }else{h.app.mail_reader_ui(ctx)});raster.update_textures(&out.textures_delta);
            if f==last{raster.buf.fill(if picker{[0.07,0.09,0.13,1.0]}else{[0.0,0.0,0.0,1.0]});raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(if picker{"reminder-picker.png"}else{"mail-reader.png"}),&rgba,(w as f32*ppp)as u32,(height as f32*ppp)as u32,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&out.textures_delta);
        }
    }
}

#[test]
fn remove_icons_are_centered_in_their_buttons_at_each_scale(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for scale in [1.0,1.25,1.5,2.0]{
        let mut h=H::new();h.ctx.set_pixels_per_point(scale);
        h.app.todos.items=["第一項","第二項","第三項"].into_iter().map(|text|crate::model::TodoItem{text:text.into(),due:None,done_on:None,reminded:false,..Default::default()}).collect();
        h.app.show_page(4);h.hover(H::local(300.0,60.0),1.0);
        for i in 0..3usize{
            let button=h.ctx.read_response(egui::Id::new(("card",true,("todo-delete",i)))).unwrap().rect;
            let ink=h.shapes.iter().filter_map(|s|match &s.shape{
                egui::Shape::Text(t) if t.galley.text()=="×" && button.contains(t.visual_bounding_rect().center())=>Some(t.visual_bounding_rect()),
                egui::Shape::LineSegment{points,..} if points.iter().all(|p|button.shrink(2.0).contains(*p)) && (points[0].x-points[1].x).abs()>2.0 && (points[0].y-points[1].y).abs()>2.0=>Some(Rect::from_two_pos(points[0],points[1])),
                _=>None,
            }).fold(Rect::NOTHING,|a,b|a.union(b));
            assert!(ink.is_positive(),"刪除按鈕應有可見圖案");
            assert!(ink.center().distance(button.center())<0.4,"縮放 {scale}：刪除圖案中心 {:?} 偏離按鈕中心 {:?}",ink.center(),button.center());
        }
        h.click(H::local(361.5,68.0));
        assert_eq!(h.app.todos.items.iter().map(|t|t.text.as_str()).collect::<Vec<_>>(),["第一項","第三項"]);
        assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()),"刪除入口不能同時勾選其他待辦");
    }
}

#[test]
#[ignore="按需檢查全部頁面與設定的實際排版"]
fn ui_alignment_layout_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_UI_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    fn fixture()->H{
        let mut h=H::new();h.app.settings.pet=false;h.app.settings.island_lyrics=false;h.power=(Some(76),false);
        h.media=crate::smtc::NowPlaying{title:"願每一個認真生活的人，都能在平凡的日子裡找到屬於自己的光".into(),artist:"The Lanterns · Live Session".into(),duration:240.0,position:65.0,playing:true,..Default::default()};
        h.app.todos.items=["整理會議資料","Review design details","確認專案需求與下一階段開發排期"].into_iter().map(|text|crate::model::TodoItem{text:text.into(),due:None,done_on:None,reminded:false,..Default::default()}).collect();
        h.app.shelf.items=["專案設計方案與交付說明.pdf","design-preview.png","本週工作安排.docx","project-files.zip","notes.md"].into_iter().map(std::path::PathBuf::from).collect();
        h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"product-team@example.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        h.app.mail.recent=(0..3).map(|uid|crate::mail::Item {received_unix: None, account:"product-team@example.com".into(),from:"產品與設計協作團隊".into(),subject:"請確認下階段的設計方案和開發排期，評審前同步最新進度".into(),time:"11-01 09:41".into(),code:if uid==0{Some("123456".into())}else{None},html: String::new().into(), summary:"這是一段郵件正文，需要確認當前需求與後續的產品設計安排。".repeat(20).into(),links:vec![],meeting:None,uid}).collect();
        h.quota_accounts(3);h
    }
    let collapsed=["collapsed","volume","caps","language","power","network","meeting","meeting-dingtalk","meeting-wecom","bt-headphones","bt-keyboard","bt-mouse","bt-gamepad","bt-phone","bt-other","bt-speaker"];
    let scenes=["page-0","page-1","page-2","page-3","page-4","page-5","page-6","page-7","lyrics","todo-empty","shelf-empty","mail-empty","mail-detail","reminder","water-chart","focus-running"];
    for ppp in [1.25,2.0]{for name in scenes.into_iter().chain(collapsed){
        let mut h=fixture();let page=name.strip_prefix("page-").and_then(|p|p.parse::<usize>().ok()).unwrap_or(match name{"lyrics"=>0,"shelf-empty"=>3,"mail-empty"|"mail-detail"=>5,_=>4});
        if name=="todo-empty"{h.app.todos.items.clear();}if name=="shelf-empty"{h.app.shelf.items.clear();}if name=="mail-empty"{h.app.mail.accounts.clear();}
        if !collapsed.contains(&name){h.app.show_page(page);}
        if name=="water-chart"{h.app.show_page(2);h.app.water_view=1;h.app.water.drunk_ml=1250;h.app.water.hours[9]=250;h.app.water.hours[8]=500;h.app.water.hours[7]=500;}
        if name=="focus-running"{h.app.show_page(1);h.app.pomo.running=true;h.app.pomo.remaining=1283.0;}
        if let Some(kind)=name.strip_prefix("bt-"){
            let kind=match kind{"headphones"=>crate::bt::DeviceKind::Headphones,"keyboard"=>crate::bt::DeviceKind::Keyboard,"mouse"=>crate::bt::DeviceKind::Mouse,"gamepad"=>crate::bt::DeviceKind::Gamepad,"phone"=>crate::bt::DeviceKind::Phone,"speaker"=>crate::bt::DeviceKind::Speaker,_=>crate::bt::DeviceKind::Other};
            h.app.bluetooth_alert(&crate::bt::Device{id:"preview".into(),name:"我的藍芽裝置 · 超長裝置名稱與連線狀態".into(),battery:Some(100),kind},true);
        }
        match name{"volume"=>{h.app.volume_ui=Some((64,false));h.app.alert("系統音量","64%",0,false);},"caps"=>h.app.alert("大寫鎖定","已開啟",0,false),"language"=>h.app.alert("鍵盤輸入語言","中文（簡體，中國）",0,false),"network"=>h.app.alert("網路連線","Wi-Fi · 網路已連線",0,false),"power"=>h.app.alert("電源已連線","當前電量 76%",0,false),"meeting"=>h.app.meeting_alert(crate::mail::Meeting{ all_day:false,title:"產品方案評審與下階段開發排期確認".into(),start:chrono::Local::now().timestamp()+600,end:None,at:"11-01 10:00".into(),uid:"preview".into()},"郵件 · product-and-design-team@company-example.com".into(),5),_=>{}}
        if name=="meeting-dingtalk" || name=="meeting-wecom" {
            h.app.meeting_alert(crate::mail::Meeting{all_day:false,title:"專案評審與下階段安排".into(),start:chrono::Local::now().timestamp()+600,end:None,at:String::new(),uid:name.into()},if name=="meeting-dingtalk"{"釘釘"}else{"企業微信"}.into(),7);
        }
        if name=="lyrics"{h.app.show_lyrics=true;h.app.set_lyrics(&h.media.title,&h.media.artist,240.0,crate::lyrics::parse_lrc("[00:00]慢慢走過這段時光\n[01:00]願每個認真生活的人都能找到屬於自己的光\n[01:10]願風帶你去遠方"));}
        if name=="mail-detail"{h.app.mail.open_uid=Some(("product-team@example.com".into(),0));}
        if name=="reminder"{h.app.open_task_reminder(0);}
        let (width,height)=((WIN_W*ppp)as u32,(WIN_H*ppp)as u32);let mut raster=crate::preview_raster::Raster::new(width as usize,height as usize);
        for f in 0..90{
            let t=f as f64/60.0;let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(WIN_W,WIN_H))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let env=Env{t,clock:"09:41:00".into(),today:"2026-11-01".into(),date:"11月1日 週日".into(),cursor:if collapsed.contains(&name){None}else{Some(H::local(150.0,40.0))},media:h.media.clone(),battery:h.power.0,bt_ready:true,..Default::default()};
            let out=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});});raster.update_textures(&out.textures_delta);
            if f==89{raster.buf.fill([0.07,0.09,0.13,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{name}-{}.png",(ppp*100.0)as u32)),&rgba,width,height,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&out.textures_delta);
        }
    }}
    for width in [600.0,690.0]{for tab in 0..12{
        let mut h=fixture();h.app.settings_tab=tab;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let size=egui::vec2(width,if width==600.0{450.0}else{560.0});let ppp=1.25;
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(ppp),focused:Some(true),..Default::default()})).collect(),focused:true,..Default::default()};*capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));});
        let (w,height)=((size.x*ppp)as u32,(size.y*ppp)as u32);let mut raster=crate::preview_raster::Raster::new(w as usize,height as usize);
        for f in 0..5{let root=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
            if f==4{raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("settings-{tab}-{}.png",width as u32)),&rgba,w,height,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
        }
    }}
}

#[test]
fn meeting_notice_prioritizes_the_start_time_and_keeps_actions_aligned(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for ppp in [1.0,1.25,1.5,2.0]{
        let mut h=H::new();h.ctx.set_pixels_per_point(ppp);h.app.settings.pet=false;
        let start=chrono::Local::now()+chrono::Duration::minutes(10);
        h.app.meeting_alert(crate::mail::Meeting{ all_day:false,title:"產品評審".into(),start:start.timestamp(),end:None,at:start.format("%m-%d %H:%M").to_string(),uid:"meeting-layout".into()},"郵件 · product-and-design-team@company-example.com".into(),7);
        for _ in 0..90{h.frame(vec![]);}
        h.click_notice_badge();
        assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().starts_with("郵件 · "))),"軟體名稱僅用於識別圖示，不佔用提醒正文");
        let clock=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==start.format("%H:%M").to_string()=>Some((t.visual_bounding_rect(),s.clip_rect,t.pos.x,t.galley.job.sections[0].format.font_id.size)),_=>None}).unwrap();
        let countdown=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text().ends_with("分鐘後")=>Some((t.visual_bounding_rect(),t.galley.job.sections[0].format.color)),_=>None}).unwrap();
        assert!(clock.0.right()+12.0<=countdown.0.left(),"縮放 {ppp}：開始時刻與倒計時應保持間距");
        assert!(clock.1.contains_rect(clock.0),"大號時刻不能被提醒條裁切");
        assert_eq!(countdown.1,egui::Color32::WHITE,"倒計時使用白色，強調色保留給來源 Logo");
        let title=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="產品評審"=>Some((t.pos.x,t.galley.job.sections[0].format.font_id.size)),_=>None}).unwrap();
        assert!(clock.3>=28.0 && clock.3>title.1,"開始時刻應比事項名稱更醒目");
        assert!((title.0-clock.2).abs()<0.5,"開始時刻與標題應沿同一列對齊");
        let date=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text().contains('月') && t.galley.text().contains('日')=>Some((t.visual_bounding_rect(),t.pos.x)),_=>None}).unwrap();
        let button=h.ctx.read_response(egui::Id::new(("card",true,"meeting-action"))).unwrap().rect;
        assert!((date.0.center().y-button.center().y).abs()<0.5,"日期與檢視按鈕應垂直居中對齊");
        assert!(((date.1-clock.1.left())-(clock.1.right()-button.right())).abs()<0.5,"卡片左右應保留相同邊距");
    }
}

#[test]
fn meeting_long_titles_scroll_to_the_end_while_short_titles_stay_still(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for title in ["簡短評審","產品設計方案評審及下一階段開發排期與交付驗收安排確認"]{
        let mut h=H::new();h.app.settings.pet=false;
        h.app.meeting_alert(crate::mail::Meeting{ all_day:false,title:title.into(),start:chrono::Local::now().timestamp()+600,end:None,at:String::new(),uid:title.into()},"飛書".into(),7);
        for _ in 0..45{h.frame(vec![]);}
        let title_shape=|h:&H|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==title=>Some((t.pos.x,t.visual_bounding_rect(),s.clip_rect)),_=>None}).expect("標題應保留完整內容，不應提前省略");
        let initial=title_shape(&h);let height=h.hit.height();
        h.t+=0.2;h.frame(vec![]);assert!((title_shape(&h).0-initial.0).abs()<0.5,"開始滾動前應停留片刻");
        h.t+=5.0;h.frame(vec![]);let middle=title_shape(&h);
        h.t+=4.0;h.frame(vec![]);let end=title_shape(&h);
        if title=="簡短評審" {assert!((end.0-initial.0).abs()<0.5,"短標題保持靜止");}
        else {
            assert!(middle.0<initial.0-20.0,"長標題應向左滾動顯示後半段");
            assert_eq!(initial.2,end.2,"滾動不應改變標題的顯示範圍");
            assert!(end.1.right()<=end.2.right()+1.0 && end.1.right()>=end.2.right()-5.0,"應能看到標題末尾");
            h.t+=0.6;h.frame(vec![]);assert!((title_shape(&h).0-end.0).abs()<0.5,"滾到末尾後應停留，不能繼續滑出");
        }
        assert!((h.hit.height()-height).abs()<0.5,"滾動不應改變提醒高度");
    }
}

#[test]
fn calendar_notice_overrides_island_lyrics_and_restores_the_current_line(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=false;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,position:7.0,duration:180.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",180.0,crate::lyrics::parse_lrc("[00:00]首句歌詞\n[00:20]後續歌詞"));
    for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.compact_notices());
    h.app.meeting_alert(crate::mail::Meeting{ all_day:false,title:"優先會議".into(),start:chrono::Local::now().timestamp()+600,end:None,at:String::new(),uid:"priority".into()},"飛書".into(),7);
    for _ in 0..90{h.frame(vec![]);}
    assert!(!h.app.compact_notices(),"日程提醒不應縮成歌詞旁的小圖示");
    assert!(h.hit.height()>130.0,"應直接展示完整 A 版提醒");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="優先會議")));
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="檢視")));
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="首句歌詞")));
    let active=h.app.notice.as_ref().unwrap();h.t=active.at+active.duration-1.0/30.0;h.media.position=25.0;
    for _ in 0..90{
        h.frame(vec![]);
        if h.app.notice.is_none(){assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Track" || t.galley.text()=="Artist")),"恢復歌詞時不能閃出展開的播放器");}
    }
    assert!(h.app.notice.is_none() && h.app.settings.island_lyrics && h.app.compact_notices());
    assert!(h.hit.height()<35.0,"提醒結束後應恢復歌詞條高度");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="後續歌詞")),"應恢復當前進度的歌詞");
}

#[test]
fn shelf_remove_icon_stays_centered_on_hover_and_preserves_the_file(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for ppp in [1.0,1.25,1.5,2.0]{
        let mut h=H::new();h.ctx.set_pixels_per_point(ppp);
        let path=crate::model::config_dir().join("設計說明.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,"保留原檔案").unwrap();
        h.app.shelf.items=vec![path.clone()];h.app.show_page(3);
        for pointer in [H::local(300.0,60.0),H::local(233.0,39.0)]{
            h.hover(pointer,1.0);
            let button=h.ctx.read_response(egui::Id::new(("card",true,("shelf-remove",&path)))).unwrap().rect;
            let ink=h.shapes.iter().filter_map(|s|match &s.shape{
                egui::Shape::LineSegment{points,..} if points.iter().all(|p|button.shrink(2.0).contains(*p)) && (points[0].x-points[1].x).abs()>2.0 && (points[0].y-points[1].y).abs()>2.0=>Some(Rect::from_two_pos(points[0],points[1])),_=>None,
            }).fold(Rect::NOTHING,|a,b|a.union(b));
            assert!(ink.is_positive() && ink.center().distance(button.center())<0.4,"置物架移除圖案應與按鈕同心");
        }
        h.click(H::local(233.0,39.0));assert!(h.app.shelf.items.is_empty());assert!(path.exists());
    }
}

#[test]
fn resident_components_can_be_dragged_saved_and_reset(){
    use crate::bar_layout::{CLOCK,BATTERY,WEATHER};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(h:&mut H,events:Vec<egui::Event>,slot:usize)->[Pos2;2]{
        let mut points=[Pos2::ZERO;2];
        h.t+=0.02;
        let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(600.0,300.0))),time:Some(h.t),events,..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.resident_order_settings(ui);for (index,slot) in [slot,BATTERY].into_iter().enumerate(){points[index]=ui.ctx().read_response(ui.id().with(("resident-icon",slot))).expect("暫未顯示的元件也應可以獨立拖動").rect.center();}});});
        h.shapes=out.shapes;points
    }
    fn point(h:&H,label:&str)->Pos2{h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap()}
    for slot in [CLOCK,WEATHER] {
    let mut h=H::new();
    draw(&mut h,vec![],slot);let [from,to]=draw(&mut h,vec![],slot);
    let button=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()};
    draw(&mut h,vec![egui::Event::PointerMoved(from),button(from,true)],slot);
    draw(&mut h,vec![egui::Event::PointerMoved(from+egui::vec2(0.0,10.0))],slot);
    draw(&mut h,vec![egui::Event::PointerMoved(to)],slot);
    for _ in 0..3{draw(&mut h,vec![],slot);}
    draw(&mut h,vec![button(to,false)],slot);
    assert_eq!(h.app.settings.resident_order.first(),Some(&slot));
    assert!(h.app.settings.resident_order.contains(&BATTERY));
    h.app.settings.save();assert_eq!(crate::model::Settings::load().resident_order,h.app.settings.resident_order);
    let reset=point(&h,"恢復預設");draw(&mut h,vec![egui::Event::PointerMoved(reset),button(reset,true)],slot);draw(&mut h,vec![button(reset,false)],slot);
    assert!(h.app.settings.resident_order.is_empty());
    }
}

#[test]
fn weather_resident_settings_preserve_old_orders_and_the_status_switch(){
    use crate::bar_layout::{BATTERY,CLOCK,FOCUS,MUSIC,PET,QUOTA,WEATHER};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let path=crate::model::config_dir().join("settings.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path,"resident_order=7,6,2,0,9,1\nshow_weather=0\n").unwrap();
    h.app.settings=crate::model::Settings::load();
    assert_eq!(h.app.settings.resident_order,[FOCUS,MUSIC,QUOTA,CLOCK,WEATHER,PET,BATTERY],"舊排列應在時鐘後插入天氣並保留其餘順序");
    h.app.weather.snapshot=Some(crate::weather::Snapshot{temp:9.0,..Default::default()});
    assert!(h.app.weather_pill().is_none(),"狀態開關關閉後天氣不佔常駐位置");
    assert!(h.app.module_on(8),"關閉常駐天氣應保留天氣頁面");
    h.app.settings.save();h.app.settings=crate::model::Settings::load();
    assert!(h.app.weather_pill().is_none(),"重啟後應保留常駐天氣開關");
    for invalid in ["0,0,2,5,6,7,9","0,1,2,5,6,7,99","0,1,2","invalid",""] {
        std::fs::write(&path,format!("resident_order={invalid}\n")).unwrap();
        assert!(crate::model::Settings::load().resident_order.is_empty(),"無效排列應使用預設位置");
    }
}

#[test]
fn weather_status_switch_controls_the_badge_and_keeps_the_page_available(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(h:&mut H,events:Vec<egui::Event>){
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();h.t+=0.02;let time=h.t;
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1000.0))),time:Some(time),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
        h.shapes=output.lock().unwrap().take().unwrap().shapes;
    }
    for theme in [crate::settings_theme::SettingsTheme::Dark,crate::settings_theme::SettingsTheme::Light] {
        let mut h=H::new();h.app.settings.settings_theme=theme;h.app.settings_tab=10;h.app.open_settings();h.ctx.set_embed_viewports(false);
        h.app.weather.snapshot=Some(crate::weather::Snapshot{temp:9.0,..Default::default()});
        for _ in 0..3 {draw(&mut h,vec![]);}
        let text_rect=|h:&H,label:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None}).unwrap();
        let heading=text_rect(&h,"狀態元件");let bottom=text_rect(&h,"功能頁面").top();
        let title=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="天氣"&&t.pos.x>=heading.left()&&t.pos.y>heading.bottom()&&t.pos.y<bottom=>Some(t.visual_bounding_rect()),_=>None}).expect("狀態元件應有獨立的天氣開關");
        let point=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Rect(r) if (r.rect.width()-34.0).abs()<0.01&&(r.rect.height()-20.0).abs()<0.01&&r.rect.center().y>title.top()&&r.rect.center().y<title.bottom()+24.0=>Some(r.rect.center()),_=>None}).unwrap();
        for expected in [false,true] {
            for pressed in [true,false] {draw(&mut h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
            assert_eq!(h.app.weather_pill().is_some(),expected);
            assert_eq!(h.app.side_items().contains(&crate::side_dock::SideItem::Weather),expected);
            assert!(h.app.module_on(8),"關閉狀態元件後天氣頁仍可使用");
            h.app.settings=crate::model::Settings::load();assert_eq!(h.app.weather_pill().is_some(),expected,"天氣開關應自動儲存");
        }
    }
}

#[test]
fn mailbox_scroll_reaches_older_mail_without_changing_pages() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=(1..=8).map(|uid|crate::mail::Item {received_unix: None, account:"test@qq.com".into(),from:format!("來源 {uid}"),subject:format!("郵件 {uid}"),time:"11-03 09:41".into(),code:Some(format!("{uid:06}")),html: String::new().into(), summary:String::new().into(),links:vec![],meeting:None,uid}).collect();
    for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}
    h.app.show_page(5);h.hover(H::local(200.0,90.0),1.0);
    h.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-900.0),modifiers:Default::default()}]);
    h.hover(H::local(200.0,90.0),0.6);
    assert_eq!(h.app.page(),5,"滾動信箱列表不能切換頁面");
    let row=h.ctx.read_response(egui::Id::new(("card",true,("mail-row",8u32,"test@qq.com")))).expect("第八封郵件應可訪問");
    h.click(row.rect.center());
    assert_eq!(h.app.mail.open_uid,Some(("test@qq.com".into(),8)));
}

#[test]
#[ignore="按需檢查信箱列表和驗證碼來源的實際畫面"]
fn mailbox_readability_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_MAIL_PREVIEW").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for variant in 0..5 {
        let multiple=variant==1;let detail=variant==2;
        let mut h=H::new();h.app.settings.pet=false;h.app.settings.mail_sync_operations=true;
        h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        if multiple {h.app.mail.accounts.push(crate::mail::Account {custom: None, email:"personal@gmail.com".into(),provider:crate::mail::Provider::Gmail,enabled:true});}
        h.app.mail.recent=[("GitHub","登入驗證碼 · 本次登入有效期 10 分鐘",Some("274630")),("Figma","產品設計評審 · 檢視本輪修改意見",None),("Notion","確認你的信箱地址",Some("815926")),("產品團隊","週五專案安排與交付清單",None),("Microsoft","你的賬戶安全通知",None)].into_iter().enumerate().map(|(i,(from,subject,code))|crate::mail::Item {html:String::new().into(),received_unix: None, account:if multiple&&i%2==1{"personal@gmail.com"}else{"work@qq.com"}.into(),from:from.into(),subject:subject.into(),time:format!("11-03 09:{:02}",41-i),code:code.map(str::to_string),summary:String::new().into(),links:vec![],meeting:None,uid:i as u32+1}).collect();
        if detail {
            h.app.mail.recent[0].summary="你好，\n\n你的登入驗證碼是 274630，有效期為 10 分鐘。\n\n如果這不是你本人的操作，請忽略這封郵件。請勿將驗證碼透露給他人。\n\n這封郵件由系統自動傳送，無需回覆。".into();
            h.app.mail.open_uid=Some(("work@qq.com".into(),1));
        }
        if variant==4{for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,0.0,chrono::Utc::now().timestamp());}}
        for account in h.app.mail.accounts.clone(){
            h.app.mail.manage.identities.insert(account.email.clone(),account.clone());
            let entries=h.app.mail.recent.iter().filter(|m|m.account==account.email).map(|m|(m.uid,m.uid%2==0)).collect();
            h.app.mail.manage.data.insert((account.email,crate::mail_manage::Folder::Inbox),crate::mail_manage::Snapshot{validity:7,entries,items:vec![]});
        }
        if variant==3{
            h.app.mail.manage.trash=h.app.mail.recent.iter().take(2).cloned().map(|mut m|{m.code=None;m}).collect();
            h.app.mail.manage.trash_open=true;
            h.app.mail.manage.data.insert(("work@qq.com".into(),crate::mail_manage::Folder::Trash),crate::mail_manage::Snapshot{validity:9,entries:vec![(1,true),(2,true)],items:vec![]});
        }
        h.app.show_page(5);
        let ppp=1.5;let mut raster=crate::preview_raster::Raster::new((crate::app::CANVAS_W*ppp)as usize,(WIN_H*ppp)as usize);
        for frame in 0..100 {
            let t=frame as f64/60.0;let env=Env{t,clock:"09:41:00".into(),cursor:Some(H::local(if variant==1{350.0}else{200.0},64.0)),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(crate::app::CANVAS_W,WIN_H))),time:Some(t),events:vec![egui::Event::PointerMoved(H::local(if variant==1{350.0}else{200.0},64.0))],viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let out=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,egui::vec2(WIN_W,WIN_H)),&env);});});
            raster.update_textures(&out.textures_delta);
            if frame==99 {raster.buf.fill([0.05,0.07,0.10,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw((crate::app::CANVAS_W*ppp)as u32,(WIN_H*ppp)as u32,rgba).unwrap();image::imageops::crop_imm(&image,((WIN_W-PW)/2.0*ppp-10.0)as u32,0,(PW*ppp+20.0)as u32,if detail{370}else{225}).to_image().save(dest.join(if detail{"郵件詳情.png"}else if variant==3{"回收站.png"}else if variant==4{"驗證碼分欄.png"}else if multiple{"懸停操作.png"}else{"信箱列表.png"})).unwrap();}
            raster.free_textures(&out.textures_delta);
        }
    }
}

#[test]
fn mailbox_large_code_card_preserves_copy_and_identifies_source(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=["work@qq.com","personal@gmail.com"].into_iter().map(|email|crate::mail::Account {custom: None, email:email.into(),provider:crate::mail::Provider::Qq,enabled:true}).collect();
    h.app.mail.recent=[("work@qq.com","GitHub","274630"),("personal@gmail.com","Notion","815926")].into_iter().map(|(account,from,code)|crate::mail::Item {received_unix: None, account:account.into(),from:from.into(),subject:"登入驗證碼".into(),time:"09:41".into(),code:Some(code.into()),html: String::new().into(), summary:String::new().into(),links:vec![],meeting:None,uid:42}).collect();
    for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}
    h.app.show_page(5);h.hover(H::local(200.0,90.0),1.0);
    assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="GitHub")));
    h.cmds.clear();h.click(H::local(60.0,70.0));
    assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Copy(value) if value=="274630")));
    assert!(h.app.mail.open_uid.is_none(),"複製驗證碼不能誤開郵件詳情");
    assert_eq!(h.app.expanded_height(),PH,"信箱沿用錄屏裡的面板尺寸");
}

#[test]
fn hidden_components_do_not_keep_static_pages_animating(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=false;
    h.app.show_page(4);h.hover(H::local(300.0,60.0),2.0);
    assert!(!h.animating);
    let mut moving=crate::anim::Spring::new(0.0,230.0,0.9);moving.target=200.0;
    h.app.shelf_x.insert("hidden.txt".into(),moving);
    h.frame(vec![]);
    assert!(!h.animating,"離開置物頁後，未完成的檔案動畫不能讓其他頁面持續重新整理");
    h.app.shelf_x.clear();h.app.pomo.running=true;
    h.hover(H::local(300.0,60.0),1.5);
    assert!(!h.animating,"待辦頁沒有顯示專注動畫，計時繼續但不應強制連續重新整理");
}

#[test]
fn minimal_music_stops_hidden_playback_animations(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.set_minimal(true);
    h.media=crate::smtc::NowPlaying{title:"歌曲".into(),artist:"歌手".into(),playing:true,..Default::default()};
    for _ in 0..180{h.frame(vec![]);}
    assert!(!h.app.open);
    assert!(!h.animating,"極簡模式隱藏媒體元件後，不應繼續為播放動畫重新整理");
}

#[test]
fn native_app_icon_uses_bounded_resolution(){
    let icon=crate::theme::app_icon();
    assert!(icon.width<=128 && icon.height<=128,"原生視窗圖示不需要常駐完整的 1024 畫素品牌原圖");
    assert_eq!(icon.rgba.len(),(icon.width*icon.height*4)as usize);
    assert!(std::sync::Arc::ptr_eq(&icon,&crate::theme::app_icon()));
}

#[test]
fn long_mail_lists_only_layout_the_visible_rows(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=false;
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=(0..60).map(|i|crate::mail::Item {received_unix: None, account:"work@qq.com".into(),from:format!("發件人 {i}"),subject:format!("第 {i} 封郵件標題"),time:"09:41".into(),code:None,html: String::new().into(), summary:String::new().into(),links:vec![],meeting:None,uid:i}).collect();
    h.app.show_page(5);h.hover(H::local(300.0,60.0),1.0);
    let count=h.ctx.fonts(|fonts|fonts.num_galleys_in_cache());
    assert!(count<100,"60 封郵件不應全部排版，當前快取了 {count} 個文字佈局");
    for _ in 0..15{h.wheel(-40.0);h.hover(H::local(300.0,60.0),0.1);}
    assert_eq!(h.app.page,5,"列表虛擬化後仍應在信箱頁滾動");
}

#[test]
fn calendar_overnight_appears_on_both_days_but_not_after_end() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let first=chrono::NaiveDate::from_ymd_opt(2026,11,3).unwrap();
    h.app.calendar.view.loaded=Some(crate::calendar::month_start(first));
    let start=first.and_hms_opt(23,30,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp();
    h.app.calendar.view.events.push(crate::mail::Meeting{ all_day:false,title:"跨夜".into(),start,end:Some(start+3600),at:String::new(),uid:"overnight".into()});
    assert_eq!(h.app.calendar_entries(first).len(),1);
    assert_eq!(h.app.calendar_entries(first.succ_opt().unwrap()).len(),1);
    assert_eq!(h.app.calendar_entries(first+chrono::Duration::days(2)).len(),0);
}

#[test]
fn calendar_month_refresh_removes_cancelled_cached_event() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::NaiveDate::from_ymd_opt(2026,11,3).unwrap();
    let start=day.and_hms_opt(16,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp();
    h.app.calendar.events.push(crate::mail::Meeting{all_day:false,title:"已取消".into(),start,end:None,at:String::new(),uid:"removed".into()});
    h.app.calendar.view.loaded=Some(crate::calendar::month_start(day));
    h.app.calendar.view.events.clear();
    assert!(h.app.calendar_entries(day).is_empty(),"月份同步後不能讓提醒快取恢復已取消日程");
}

#[test]
fn ignored_mail_event_hides_stops_reminders_persists_and_restores() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let now=chrono::Local::now();let day=now.date_naive();
    let meeting=crate::mail::Meeting{all_day:false,title:"測試邀請".into(),start:now.timestamp()+600,end:None,at:String::new(),uid:"ignore-me".into()};
    h.app.mail.recent.push(crate::mail::Item {received_unix: None, account:"test@qq.com".into(),from:"測試".into(),subject:"邀請".into(),time:String::new(),code:None,html: String::new().into(), summary:String::new().into(),links:vec![],meeting:Some(meeting.clone()),uid:99});
    h.app.toggle_mail_event(&meeting);
    assert!(h.app.calendar_entries(day).is_empty());
    assert!(crate::model::Settings::load().ignored_mail_events.contains(&crate::calendar::dedupe_key(&meeting)));
    h.frame(vec![]);assert!(!h.app.history.iter().any(|a|a.title=="會議提醒"));
    assert_eq!(h.app.mail.recent.len(),1,"忽略不刪除郵件");
    h.app.calendar.events.push(meeting.clone());
    assert_eq!(h.app.calendar_entries(day)[0].sources.len(),1,"飛書來源不受郵件忽略影響");
    h.app.calendar.events.clear();h.app.toggle_mail_event(&meeting);
    assert_eq!(h.app.calendar_entries(day).len(),1);
    h.frame(vec![]);assert!(h.app.history.iter().any(|a|a.title=="會議提醒"));
}

#[test]
fn calendar_sync_result_does_not_enqueue_another_sync() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let config=crate::calendar::Config{provider:crate::calendar::Provider::Feishu,username:"test".into(),href:None,server:None};
    h.app.calendar.config=Some(config.clone());h.app.open_calendar(None);h.app.pending.clear();
    h.app.calendar.view.loading=false;
    h.app.calendar_event(crate::calendar::Event::Synced{config,events:vec![],at:"12:00".into()});
    assert!(!h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::CalendarMonth(_))),"同步結果不能觸發下一輪同步，否則後臺會持續請求");
}

#[test]
fn page_order_persists_and_controls_navigation() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.page_order=[5,2,0,1,3,4,6,7,8];
    h.app.settings.save();
    let settings=crate::model::Settings::load();
    assert_eq!(settings.page_order,[5,2,0,1,3,4,6,7,8]);
    let restarted=App::new(&h.ctx,Options{fetch_lyrics:false,pet_idle:1e9});
    assert_eq!(restarted.page(),5);
    for page in [1,3,4,6,7,8] {h.app.set_page_visible(page,false);}
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    for expected in [5,2,0] {
        h.click(H::local(398.0,68.0));
        assert_eq!(h.app.page,expected);
        h.hover(H::local(300.0,60.0),0.7);
    }
    h.click(H::local(21.0,68.0));
    assert_eq!(h.app.page,2);
    h.app.set_page_visible(2,false);
    assert_eq!(h.app.page,5);
    for invalid in ["0,0,1,2,3,4,5,6","8,1,2,3,4,5,6,7","0,1","invalid"] {
        std::fs::write(crate::model::config_dir().join("settings.txt"),format!("page_order={invalid}\n")).unwrap();
        assert_eq!(crate::model::Settings::load().page_order,[0,1,2,3,4,5,6,7,8]);
    }
}

#[test]
fn page_order_drag_applies_on_release_and_saves() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings_tab=10;h.app.open_settings();h.ctx.set_embed_viewports(false);
    fn draw(h:&mut H,events:Vec<egui::Event>){
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        h.t+=0.02;let time=h.t;
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1000.0))),time:Some(time),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let _=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));
        let child=output.lock().unwrap().take().unwrap();h.shapes=child.shapes;
    }
    for _ in 0..3 {draw(&mut h,vec![]);}
    let point=|h:&H,label:&str|h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().left_center()),_=>None}).unwrap();
    let from=point(&h,"播放控制與同步歌詞")+egui::vec2(-18.0,-6.0);
    let to=point(&h,"喝水與久坐提醒");
    let button=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()};
    draw(&mut h,vec![egui::Event::PointerMoved(from),button(from,true)]);
    draw(&mut h,vec![egui::Event::PointerMoved(from+egui::vec2(0.0,10.0))]);
    draw(&mut h,vec![egui::Event::PointerMoved(to)]);
    assert_eq!(h.app.settings.page_order,[0,1,2,3,4,5,6,7,8]);
    draw(&mut h,vec![button(to,false)]);
    assert_eq!(h.app.settings.page_order,[1,2,0,3,4,5,6,7,8]);
    assert_eq!(crate::model::Settings::load().page_order,h.app.settings.page_order);
}

#[test]
fn settings_theme_follows_system_and_preserves_island_style() {
    use crate::settings_theme::SettingsTheme;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
    assert_eq!(h.app.settings.settings_theme,SettingsTheme::System);
    assert_eq!(SettingsTheme::parse("unknown"),SettingsTheme::System);
    let mut raster=crate::preview_raster::Raster::new(690,1240);
    for (mode,system,dark) in [(SettingsTheme::System,false,false),(SettingsTheme::System,true,true),
        (SettingsTheme::System,false,false),(SettingsTheme::Dark,false,true),(SettingsTheme::Light,true,false)] {
        h.app.settings.settings_theme=mode;
        h.app.settings.save();assert_eq!(crate::model::Settings::load().settings_theme,mode);
        h.ctx.data_mut(|d|d.insert_temp(egui::Id::new("settings-system-theme"),(std::time::Instant::now(),system)));
        let output=std::sync::Arc::new(std::sync::Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1240.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        for tab in 0..12 {
            h.app.settings_tab=tab;
            // AI 額度的新增賬號表單預設收起，點「＋ 新增賬號」才展開；展開後再檢查表單行對齊。
            h.app.quota.connect_open=tab==11;
            for frame in 0..3 {
                h.ctx.data_mut(|d|d.insert_temp(egui::Id::new("settings-system-theme"),(std::time::Instant::now(),system)));
                let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
                assert!(h.ctx.style().visuals.dark_mode,"設定頁主題不能洩漏到島體");
                let commands=&root.viewport_output[&egui::ViewportId::from_hash_of("daozai-settings")].commands;
                assert!(commands.iter().any(|c|matches!(c,egui::ViewportCommand::SetTheme(t) if *t==if dark{egui::SystemTheme::Dark}else{egui::SystemTheme::Light})),"原生邊框必須與頁面一致");
                let child=output.lock().unwrap().take().unwrap();
                if frame==2 {
                    let pairs:&[(&str,&str)]=match tab {
                        1=>&[("時間偏移","0.35 秒")],
                        2=>&[("專注","25 分鐘"),("休息","5 分鐘")],
                        3=>&[("每日目標","2000 ml"),("每次記錄","250 ml"),("提醒間隔","45 分鐘"),("提醒時段","9 時")],
                        7=>&[("驗證碼隱藏","60 秒"),("檢查間隔","120 秒"),("會議提前","15 分鐘"),("信箱","輸入信箱地址"),("授權碼","輸入信箱授權碼")],
                        8=>&[("使用者名稱","輸入使用者名稱或貼上帳號訊息"),("密碼","輸入平臺生成的密碼")],
                        11=>&[("服務","Codex"),("備註","例如：個人、工作、備用"),("選擇該帳號的登入檔案","選擇…")],
                        _=>&[],
                    };
                    for (label,value) in pairs {
                        let centers=|needle:&str|child.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==needle=>Some(t.visual_bounding_rect().center().y),_=>None}).collect::<Vec<_>>();
                        let distance=centers(label).iter().flat_map(|a|centers(value).into_iter().map(move|b|(a-b).abs())).fold(f32::INFINITY,f32::min);
                        // 純中文（微軟正黑體 msjh.ttc）與純拉丁/數字（Segoe UI）在同字號下的可見字形邊界垂直中心差約 1.5px。
                        assert!(distance<=2.0,"設定頁 {tab} 的 {label} 與 {value} 未居中：{distance}");
                    }
                }
                if tab==0 && frame==2 {
                    let center=|label:&str|child.shapes.iter().find_map(|s|match &s.shape {
                        egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().center().y),_=>None,
                    }).unwrap();
                    for (label,choices) in [("顯示屏",vec!["跟隨主屏"]),("時間制式",vec!["24 小時","12 小時"]),("時段文字",vec!["不顯示","上午 / 下午","詳細時段"])] {
                        // 微軟正黑體 UI（msjh.ttc）混排 Segoe UI 的 `/` 時，可見字形邊界垂直中心差約 1.125px。
                        for choice in choices {assert!((center(label)-center(choice)).abs()<1.5,"{label}與{choice}應垂直居中：{} / {}",center(label),center(choice));}
                    }
                }
                let background=if dark{egui::Color32::from_rgb(25,25,27)}else{egui::Color32::from_rgb(246,247,249)};
                assert!(child.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.fill==background && r.rect.width()>400.0)));
                raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);
                if frame==2 {
                    if let Some(dest)=std::env::var_os("DAOZAI_THEME_PREVIEW_DIR") {
                        let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
                        raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                        image::save_buffer(dest.join(format!("settings-{}-{tab}.png",if dark{"dark"}else{"light"})),&rgba,690,1240,image::ColorType::Rgba8).unwrap();
                    }
                }
                raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
        }
    }
}


#[test]
fn mail_report_open_settings_matrix() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.ctx.set_embed_viewports(false);
    for scale in [1.0,1.25,1.5,2.0] {
        for size in [egui::vec2(600.0,450.0),egui::vec2(690.0,560.0)] {
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
                let id=viewport.ids.this;
                let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
                let _=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));
            });
            for count in [0,1,6] {
                h.app.mail.accounts=crate::mail::Provider::ALL.into_iter().take(count).enumerate().map(|(i,provider)|crate::mail::Account {custom: None, email:format!("probe{i}@example.invalid"),provider,enabled:false}).collect();
                for provider in crate::mail::Provider::ALL {
                    h.app.mail.form.provider=provider;h.app.open_mail_settings();
                    for _ in 0..3 {let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
                    assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,7);
                    h.app.settings_open=false;let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
                }
            }
        }
    }
}

#[test]
fn mail_report_initial_fetch_unicode_message() {
    for space in ['\u{00a0}','\u{2002}','\u{2003}','\u{202f}'] {
        for kind in ["text/plain","text/html"] {
            let raw=format!("From: probe@example.invalid\r\nSubject: verification\r\nMIME-Version: 1.0\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n驗證碼：{space}834921");
            let parsed=crate::mail::parse_message(raw.as_bytes());
            assert_eq!(crate::mail::code_of(&parsed),Some("834921".into()));
        }
    }
}

#[test]
fn mail_report_save_and_failure_paths() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.open_mail_settings();h.ctx.set_embed_viewports(false);
    fn frame(h:&mut H,events:Vec<egui::Event>){
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(900.0,1800.0))),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
        h.shapes=output.lock().unwrap().take().unwrap().shapes;
    }
    fn click(h:&mut H,label:&str){
        frame(h,vec![]);frame(h,vec![]);
        let point=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap();
        frame(h,vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false]{frame(h,vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    }
    for button in ["測試連線","儲存並開始監控"] {
        h.app.mail.form.email.clear();h.app.mail.form.password.clear();click(&mut h,button);
        assert_eq!(h.app.mail.note,"請先填信箱地址");
        h.app.mail.form.email="probe@example.invalid".into();click(&mut h,button);
        assert!(h.app.mail.note.contains("請先填授權碼"));assert!(h.app.mail.accounts.is_empty());
    }
    let email=format!("daozai-crash-probe-{}@example.invalid",std::process::id());
    struct Cleanup(String);impl Drop for Cleanup{fn drop(&mut self){crate::mail::secrets::delete(&self.0);}}
    let _cleanup=Cleanup(email.clone());
    h.app.mail.form.email=email.clone();h.app.mail.form.password="synthetic-test-secret".into();
    click(&mut h,"儲存並開始監控");
    assert_eq!(h.app.mail.accounts.len(),1);assert!(h.app.mail.form.password.is_empty());
    assert_eq!(crate::mail::load_accounts()[0].email,email);
    assert_eq!(crate::mail::secrets::load(&email).unwrap(),"synthetic-test-secret");
    h.app.mail_event(crate::mail::Event::Status{account:email.clone(),status:crate::mail::Status::Failed("模擬認證失敗".into())});frame(&mut h,vec![]);
    h.app.mail.form.password="synthetic-test-secret".into();click(&mut h,"測試連線");
    assert_eq!(h.app.mail.testing.as_deref(),Some(email.as_str()));
    h.app.mail_event(crate::mail::Event::Test{account:email.clone(),ok:false,note:"模擬網路超時".into()});frame(&mut h,vec![]);
    assert!(h.app.mail.testing.is_none());assert!(h.app.mail.note.contains("模擬網路超時"));
    // 從表單測試連線成功＝連線並儲存，不必再按一次「儲存並開始監控」；測試失敗則什麼都不存。
    h.app.mail.accounts.clear();crate::mail::secrets::delete(&email);
    h.app.mail.form.email=email.clone();h.app.mail.form.password="synthetic-test-secret".into();click(&mut h,"測試連線");
    h.app.mail_event(crate::mail::Event::Test{account:email.clone(),ok:false,note:"模擬認證失敗".into()});frame(&mut h,vec![]);
    assert!(h.app.mail.accounts.is_empty()&&h.app.mail.form_test.is_none()&&crate::mail::secrets::load(&email).is_err(),"測試失敗不能存帳號");
    click(&mut h,"測試連線");
    h.app.mail.form.password="改過的授權碼".into();
    h.app.mail_event(crate::mail::Event::Test{account:email.clone(),ok:true,note:String::new()});frame(&mut h,vec![]);
    assert_eq!(h.app.mail.accounts.len(),1,"測試成功要直接加入信箱清單");assert!(h.app.mail.note.contains("已儲存"),"{}",h.app.mail.note);
    assert_eq!(crate::mail::secrets::load(&email).unwrap(),"synthetic-test-secret","要存測試當下的授權碼，不是測試期間改過的欄位");
    assert_eq!(h.app.mail.form.password,"改過的授權碼","表單已被改過就不清掉");
    h.app.mail_event(crate::mail::Event::Test{account:email,ok:true,note:String::new()});
    assert!(h.app.mail.note.contains("連線成功")&&!h.app.mail.note.contains("已儲存"),"沒有從表單發起的測試不重複儲存");
}

#[test]
fn hover_tooltips_fit_native_region_and_release_it_after_leaving(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for shelf in [false,true] {
        let mut h=H::new();
        let label;
        let pointer;
        if shelf {
            let path=crate::model::config_dir().join("開發專案原始碼與當前維護的專案資料夾");
            std::fs::create_dir_all(&path).unwrap();
            label=path.display().to_string();h.app.shelf.add(&path).unwrap();
            h.app.show_page(3);h.hover(H::local(300.0,60.0),1.0);
            pointer=H::local(210.0,66.0);
        } else {
            h.app.settings.island_lyrics=true;
            h.media.title="測試歌曲".into();h.media.artist="歌手".into();h.media.duration=240.0;h.media.playing=true;
            h.app.set_lyrics("測試歌曲","歌手",240.0,crate::lyrics::parse_lrc("[00:00]測試歌詞"));
            h.app.alert("郵件驗證碼","收到郵件，驗證碼 123456，請在十分鐘內使用",5,false);
            h.hover(Pos2::new(600.0,220.0),1.0);
            label="郵件驗證碼\n收到郵件，驗證碼 123456，請在十分鐘內使用".to_string();
            pointer=Pos2::new(WIN_W/2.0+(h.app.current_width()-44.0)/2.0+26.0,15.0);
        }
        h.hover(pointer,0.4);
        let hit_before_tooltip=h.hit;
        h.hover(pointer,1.1);
        let bounds=h.shapes.iter().find_map(|s|match &s.shape {
            egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None,
        }).expect("懸停後應顯示完整提示");
        assert!(h.region.contains_rect(bounds),"懸停提示不能被原生視窗裁掉：{bounds:?} / {:?}",h.region);
        assert_eq!(h.hit,hit_before_tooltip,"提示不應擴大島體互動區域");
        h.hover(Pos2::new(900.0,250.0),3.0);
        assert!(h.region.bottom()<100.0,"提示消失後應恢復收起區域");
    }
}

#[test]
fn clock_seconds_formats_and_persists(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();assert!(!h.app.settings.clock_seconds);
    assert_eq!(h.app.settings.clock_text("23:59:58"),"23:59");
    h.app.settings.clock_seconds=true;
    for (input,expected) in [("00:00:00","00:00:00"),("23:59:59","23:59:59"),("13:05","13:05:00"),("invalid","--:--:--")] {
        assert_eq!(h.app.settings.clock_text(input),expected);
    }
    h.app.settings.clock_12h=true;h.app.settings.clock_period=1;
    for (input,expected) in [("00:00:01","上午 12:00:01"),("12:00:00","下午 12:00:00"),("13:05:08","下午 1:05:08")] {
        assert_eq!(h.app.settings.clock_text(input),expected);
    }
    h.app.settings.clock_period=2;
    assert_eq!(h.app.settings.clock_text("23:59:59"),"晚上 11:59:59");
    for enabled in [true,false] {
        h.app.settings.clock_seconds=enabled;h.app.settings.save();
        assert_eq!(crate::model::Settings::load().clock_seconds,enabled);
    }
}

#[test]
fn clock_residency_only_hides_the_collapsed_clock(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (seconds,twelve,period) in [(false,false,0),(true,false,0),(true,true,2)] {
        let mut h=H::new();h.app.settings.pet=false;h.power=(Some(76),false);h.quota_accounts(2);
        h.app.settings.clock_seconds=seconds;h.app.settings.clock_12h=twelve;h.app.settings.clock_period=period;
        h.app.weather.snapshot=Some(crate::weather::Snapshot{temp:19.0,..Default::default()});
        let digits=if seconds {if twelve{"9:41:00"}else{"09:41:00"}}else{"09:41"};
        let visible=|h:&H|h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==digits));
        for _ in 0..90{h.frame(vec![]);}assert!(visible(&h));let width=h.app.resident.target.width;
        h.app.settings.show_clock=false;
        for _ in 0..90{h.frame(vec![]);}
        assert!(!visible(&h));assert!(h.app.resident.target.x[crate::bar_layout::CLOCK].is_none());
        assert!(h.app.resident.target.width<width,"關閉時間應收回常駐佔位");
        h.hover(Pos2::new(WIN_W/2.0,15.0),1.0);assert!(visible(&h),"展開頂欄仍應顯示時間");
        h.cursor=None;for _ in 0..120{h.frame(vec![]);}
        for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
            h.app.settings.dock=dock;
            for _ in 0..90{side_frame(&mut h,Some(Pos2::new(250.0,580.0)));}
            assert!(!h.app.side_items().contains(&crate::side_dock::SideItem::Clock));
            assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="41")),"側邊收起態也應隱藏時間");
        }
        h.app.settings.show_clock=true;h.app.settings.dock=crate::model::Dock::Top;
        for _ in 0..90{h.frame(vec![]);}
        assert!(visible(&h));assert_eq!(h.app.resident.target.width,width,"重新開啟時間應恢復佈局");
    }
}

#[test]
fn clock_residency_switch_in_page_management_saves(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(h:&mut H,raster:&mut crate::preview_raster::Raster,events:Vec<egui::Event>){
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();h.t+=0.02;let time=h.t;
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            *capture.lock().unwrap()=Some(ctx.run(egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,1400.0))),time:Some(time),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()},|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));let child=output.lock().unwrap().take().unwrap();
        raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);h.shapes=child.shapes;
    }
    for dark in [true,false] {
        let mut h=H::new();h.app.settings.settings_theme=if dark{crate::settings_theme::SettingsTheme::Dark}else{crate::settings_theme::SettingsTheme::Light};
        h.app.settings_tab=10;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let mut raster=crate::preview_raster::Raster::new(780,1400);
        assert!(h.app.settings.show_clock,"已有配置預設保持時間常駐");
        for _ in 0..3{draw(&mut h,&mut raster,vec![]);}
        let title=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="時間"=>Some(t.visual_bounding_rect()),_=>None}).expect("狀態元件應提供時間開關");
        let point=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Rect(r)if (r.rect.width()-34.0).abs()<0.01&&(r.rect.height()-20.0).abs()<0.01&&r.rect.center().y>title.top()&&r.rect.center().y<title.bottom()+24.0=>Some(r.rect.center()),_=>None}).unwrap();
        for expected in [false,true] {
            for pressed in [true,false]{draw(&mut h,&mut raster,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
            draw(&mut h,&mut raster,vec![]);
            assert_eq!(h.app.settings.show_clock,expected);assert_eq!(crate::model::Settings::load().show_clock,expected,"常駐時間開關應自動儲存");
            let preview=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text().contains(':')&&t.galley.job.sections.first().is_some_and(|s|(s.format.font_id.size-crate::theme::size::PILL_CLOCK).abs()<0.01)=>Some(t.fallback_color.a()),_=>None}).expect("常駐排列應保留時間的位置");
            assert_eq!(preview==255,expected,"關閉時間後排列預覽應置灰");
            if let Some(dest)=std::env::var_os("DAOZAI_CLOCK_RESIDENCY_PREVIEW") {
                let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
                raster.buf.fill([0.08,0.08,0.09,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),1.0),1.0);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest.join(format!("settings-{}-{}.png",if dark{"dark"}else{"light"},if expected{"on"}else{"off"})),&rgba,780,1400,image::ColorType::Rgba8).unwrap();
            }
        }
    }
}

#[test]
fn clock_seconds_keep_resident_spacing_and_digit_width(){
    use crate::bar_layout::{CLOCK,WEATHER};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for period in 0..=2 {for twelve in [false,true] {
        let mut h=H::new();h.app.settings.pet=false;h.app.settings.clock_seconds=true;
        h.app.settings.clock_period=period;h.app.settings.clock_12h=twelve;
        h.quota_accounts(3);h.power=(Some(76),true);
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,..Default::default()});
        for reversed in [false,true] {
            h.app.settings.resident_order=crate::bar_layout::DEFAULT_ORDER.to_vec();
            if reversed {h.app.settings.resident_order.reverse();}
            for _ in 0..90 {h.frame(vec![]);}
            let digits=if twelve {"9:41:00"}else{"09:41:00"};
            let text=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==digits=>Some(t.visual_bounding_rect()),_=>None}).expect("收起時應顯示秒數");
            let temp=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="19°"=>Some(t.visual_bounding_rect()),_=>None}).unwrap();
            assert!(!text.expand(4.0).intersects(temp),"時鐘和天氣應保持間距");
            let layout=h.app.resident.current();
            assert!((layout.x[CLOCK].unwrap()-layout.x[WEATHER].unwrap()).abs()>(layout.widths[CLOCK]+layout.widths[WEATHER])/2.0,"排列反轉後仍需預留秒數寬度");
        }
        let _=h.ctx.run(Default::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{
            let c=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect());
            let widths:Vec<_>=(0..60).map(|second|crate::paint::clock_width(&c,&h.app.settings.clock_text(&format!("09:41:{second:02}")),crate::theme::size::PILL_CLOCK)).collect();
            assert!(widths.iter().all(|w|(w-widths[0]).abs()<0.01),"秒數變化不應改變時鐘佔位");
        });});
        h.app.show_page(0);h.hover(H::local(200.0,40.0),1.0);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==if twelve{"9:41:00"}else{"09:41:00"})),"展開後頂欄也應顯示秒數");
        h.t+=5.0;
        for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
            h.app.settings.dock=dock;
            for _ in 0..90 {side_frame(&mut h,Some(Pos2::new(250.0,580.0)));}
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="00")),"側邊收起時應顯示秒數");
        }
    }}
}

#[test]
fn clock_seconds_setting_toggles_and_saves(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
    let mut raster=crate::preview_raster::Raster::new(780,1800);
    fn frame(h:&mut H,raster:&mut crate::preview_raster::Raster,events:Vec<egui::Event>){
        h.t+=1.0/60.0;let time=h.t;
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            *capture.lock().unwrap()=Some(ctx.run(egui::RawInput{time:Some(time),viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,1800.0))),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()},|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let root=h.ctx.run(egui::RawInput{time:Some(time),..Default::default()},|ctx|h.app.settings_ui(ctx,false));let child=output.lock().unwrap().take().unwrap();
        raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);h.shapes=child.shapes;
    }
    for _ in 0..3{frame(&mut h,&mut raster,vec![]);}
    let label=|h:&H|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="顯示秒數"&&s.clip_rect.contains_rect(t.visual_bounding_rect())=>Some(t.visual_bounding_rect()),_=>None});
    assert!(!h.app.settings.clock_seconds);
    for enabled in [true,false] {
        let title=label(&h).expect("常規設定應提供顯示秒數開關");
        let toggle=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Rect(r)if (r.rect.width()-34.0).abs()<0.1&&(r.rect.height()-20.0).abs()<0.1&&(r.rect.top()-title.top()).abs()<18.0=>Some(r.rect.center()),_=>None}).expect("秒數開關應與文字對齊");
        for pressed in [true,false]{frame(&mut h,&mut raster,vec![egui::Event::PointerMoved(toggle),egui::Event::PointerButton{pos:toggle,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        frame(&mut h,&mut raster,vec![]);
        assert_eq!(h.app.settings.clock_seconds,enabled);assert_eq!(crate::model::Settings::load().clock_seconds,enabled);
        if let Some(dest)=std::env::var_os("DAOZAI_CLOCK_PREVIEW_DIR") {
            let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
            raster.buf.fill([0.08,0.08,0.09,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),1.0),1.0);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dest.join(if enabled{"settings-on.png"}else{"settings-off.png"}),&rgba,780,1800,image::ColorType::Rgba8).unwrap();
        }
    }
}

#[test]
fn clock_12h_formats_boundaries_and_persists() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    assert!(!h.app.settings.clock_12h);
    assert_eq!(h.app.settings.clock_text("23:59:00"),"23:59");
    assert_eq!(h.app.settings.clock_period,0);
    h.app.settings.clock_12h=true;
    assert_eq!(h.app.settings.clock_text("00:00"),"12:00");
    assert_eq!(h.app.settings.clock_text("23:59"),"11:59");
    h.app.settings.clock_period=1;
    for (input,expected) in [("00:00:00","上午 12:00"),("11:59:00","上午 11:59"),("12:00:00","下午 12:00"),("13:05:00","下午 1:05"),("23:59:00","下午 11:59")] {
        assert_eq!(h.app.settings.clock_text(input),expected);
    }
    assert_eq!(h.app.settings.clock_text("13:05"),"下午 1:05");
    h.app.settings.save();
    assert!(crate::model::Settings::load().clock_12h);
    h.app.settings.pet=false;
    h.quota_accounts(5);
    for _ in 0..90 { h.frame(vec![]); }
    let text_rect=|label:&str|h.shapes.iter().find_map(|s|match &s.shape {
        egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None,
    }).unwrap();
    assert!(h.app.resident.current().x[crate::bar_layout::quota_slot(4)].unwrap()+WIN_W/2.0+13.5<text_rect("上午").left());
    h.hover(H::local(200.0,40.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="9:41")));
    h.app.settings.clock_period=0;
    h.app.settings.clock_12h=false;
    h.app.settings.save();
    assert!(!crate::model::Settings::load().clock_12h);
    h.frame(vec![]);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="09:41")));
}

#[test]
fn clock_period_boundaries_and_both_formats() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    for twelve in [false,true] {
        h.app.settings.clock_12h=twelve;
        h.app.settings.clock_period=2;
        for (time,label) in [("00:00","凌晨"),("05:59","凌晨"),("06:00","上午"),("10:59","上午"),("11:00","中午"),("12:59","中午"),("13:00","下午"),("16:59","下午"),("17:00","傍晚"),("18:59","傍晚"),("19:00","晚上"),("23:59","晚上")] {
            assert!(h.app.settings.clock_text(time).starts_with(&format!("{label} ")));
        }
        assert_eq!(h.app.settings.clock_text("19:30"),if twelve {"晚上 7:30"}else{"晚上 19:30"});
        for mode in 0..=2 {
            h.app.settings.clock_period=mode;
            h.app.settings.save();
            let loaded=crate::model::Settings::load();
            assert_eq!(loaded.clock_period,mode);
            assert_eq!(loaded.clock_12h,twelve);
        }
    }
}

#[test]
fn clock_period_visually_centers_with_digits() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let h=H::new();
    let mut raster=crate::preview_raster::Raster::new(420,230);
    for scale in [1.0,1.25,1.5,2.0] {
        let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(420.0,230.0))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                let mut c=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect());c.s=scale;
                for (i,period) in ["凌晨","上午","中午","下午","傍晚","晚上"].iter().enumerate() {
                    for (j,size) in [12.5,15.5].iter().enumerate() {
                        crate::paint::clock(&c,10.0+j as f32*100.0,18.0+i as f32*18.0,&format!("{period} 12:59"),*size);
                    }
                }
            });
        });
        let texts:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t),_=>None}).collect();
        for pair in texts.chunks_exact(2) {
            assert!((pair[0].visual_bounding_rect().center().y-pair[1].visual_bounding_rect().center().y).abs()<0.1,"時段文字與數字應按可見字形垂直居中，縮放 {scale}");
        }
        raster.update_textures(&output.textures_delta);
        if scale==2.0 {
            if let Some(dest)=std::env::var_os("DAOZAI_CLOCK_ALIGNMENT_PREVIEW") {
                raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest,&rgba,420,230,image::ColorType::Rgba8).unwrap();
            }
        }
    }
}

#[test]
fn avoidance_picker_add_remove_and_save() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.ctx.set_visuals(egui::Visuals::dark());
    assert!(h.app.settings.avoid_browsers);
    assert!(h.app.settings.avoid_apps.is_empty());
    h.app.settings.browser_minimal=true;
    h.app.avoidance.entries=vec![
        crate::avoidance::Candidate{path:r"D:\Quark\quark.exe".into(),name:"夸克瀏覽器".into(),title:"新標籤頁".into(),browser:true},
        crate::avoidance::Candidate{path:r"C:\Windows\System32\notepad.exe".into(),name:"記事本".into(),title:"未命名 - 記事本".into(),browser:false},
    ];
    fn draw(h:&mut H,events:Vec<egui::Event>)->egui::FullOutput {
        h.t+=1.0/60.0;
        h.ctx.run(egui::RawInput{time:Some(h.t),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(600.0,560.0))),events,..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                ui.heading("常規");
                h.app.avoidance.settings(ui,&mut h.app.settings);
            });
            h.app.avoidance.dialog(ctx,&mut h.app.settings);
        })
    }
    fn click(h:&mut H,point:Pos2){
        draw(h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        draw(h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    }
    fn point(out:&egui::FullOutput,label:&str)->Pos2 {
        out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("未找到 {label}"))
    }
    h.app.avoidance.open=true;
    let mut raster=crate::preview_raster::Raster::new(600,560);
    let mut out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);
    for _ in 0..40 {std::thread::sleep(std::time::Duration::from_millis(10));out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);}
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="瀏覽器規則已覆蓋")));
    if let Some(dest)=std::env::var_os("DAOZAI_AVOID_PREVIEW_DIR") {
        std::fs::create_dir_all(&dest).unwrap();raster.buf.fill([0.0,0.0,0.0,1.0]);
        raster.draw(&h.ctx.tessellate(out.shapes.clone(),out.pixels_per_point),out.pixels_per_point);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(std::path::Path::new(&dest).join("新增應用.png"),&rgba,600,560,image::ColorType::Rgba8).unwrap();
    }
    click(&mut h,point(&out,"記事本"));
    let out=draw(&mut h,vec![]);
    click(&mut h,point(&out,"新增"));
    assert!(!h.app.avoidance.open);
    assert_eq!(h.app.settings.avoid_apps.len(),1);
    h.app.settings.avoid_browsers=false;h.app.settings.save();
    let loaded=crate::model::Settings::load();
    assert!(!loaded.avoid_browsers);assert!(loaded.browser_minimal);
    assert_eq!(loaded.avoid_apps,h.app.settings.avoid_apps);
    let mut out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);
    for _ in 0..40{std::thread::sleep(std::time::Duration::from_millis(20));out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);}
    assert!(!out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="記事本")),"圖示區不應常駐顯示應用名稱");
    if let Some(dest)=std::env::var_os("DAOZAI_AVOID_PREVIEW_DIR") {
        raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(out.shapes.clone(),out.pixels_per_point),out.pixels_per_point);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(std::path::Path::new(&dest).join("圖示區域.png"),&rgba,600,560,image::ColorType::Rgba8).unwrap();
    }
    click(&mut h,point(&out,"×"));assert!(h.app.settings.avoid_apps.is_empty());
    h.app.settings.save();assert!(crate::model::Settings::load().avoid_apps.is_empty());
}

#[test]
fn six_minutes_without_interaction_do_not_restart_static_animation_or_grow_textures(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=false;h.app.water.remind=false;
    for _ in 0..120{h.frame(vec![]);}
    let textures=h.ctx.tex_manager().read().num_allocated();
    for second in 3..=363 {
        h.t=second as f64;h.system_idle=Some(second as f64);h.frame(vec![]);
        assert!(!h.animating,"空閒到第 {second} 秒不應重新進入連續動畫");
        assert_eq!(h.ctx.tex_manager().read().num_allocated(),textures,"空閒紋理數不能持續增長");
    }
}

#[test]
fn avoidance_icon_area_stays_one_row_and_scrolls() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.ctx.set_visuals(egui::Visuals::dark());h.app.settings.browser_minimal=true;
    fn draw(h:&mut H,events:Vec<egui::Event>)->egui::FullOutput {
        h.t+=1.0/60.0;
        h.ctx.run(egui::RawInput{time:Some(h.t),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(420.0,560.0))),events,..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{h.app.avoidance.settings(ui,&mut h.app.settings);});
        })
    }
    fn text(out:&egui::FullOutput,label:&str)->Rect {
        out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None}).unwrap()
    }
    let footer="無需全屏或最大化；視窗移開或切換應用後恢復。";
    h.app.settings.avoid_apps=vec![("D:/sample/A.exe".into(),"A應用".into())];
    draw(&mut h,vec![]);let one=draw(&mut h,vec![]);let baseline=text(&one,footer).top();
    let cross=text(&one,"×").center();
    assert!(one.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c) if c.radius<=7.1 && c.center.distance(cross)<3.0)),"移除角標應為小圓點，不能被按鈕最小尺寸撐大");
    h.app.settings.avoid_apps=(0..30).map(|i|(format!("D:/sample/{i}.exe"),format!("{}應用",char::from_u32(65+i).unwrap()))).collect();
    draw(&mut h,vec![]);let many=draw(&mut h,vec![]);
    assert!((text(&many,footer).top()-baseline).abs()<1.0,"應用增多時始終保持單行高度");
    let first=text(&many,"A");let pointer=first.center();
    draw(&mut h,vec![egui::Event::PointerMoved(pointer),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-150.0),modifiers:Default::default()}]);
    let mut scrolled=draw(&mut h,vec![]);for _ in 0..30{scrolled=draw(&mut h,vec![]);}
    assert!((text(&scrolled,footer).top()-text(&many,footer).top()).abs()<1.0,"滾動圖示不能移動區域下方內容");
    assert!(scrolled.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="A" && t.visual_bounding_rect().left()<first.left()-10.0)) || !scrolled.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="A")),"應能滾動檢視後續應用");
}

#[test]
fn compact_canvas_preserves_panel_and_notice_pixels(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(width:f32,page:Option<usize>,notice:bool)->Vec<u8>{
        let mut h=H::new();h.app.settings.island_lyrics=true;
        let media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗\n[01:00]下一句"));
        if let Some(page)=page{h.app.show_page(page);}
        let mut raster=crate::preview_raster::Raster::new(1280,270);
        let mut rgba=Vec::new();
        for frame in 0..180 {
            if notice && frame==60{h.app.alert("收到新郵件","會議通知及驗證碼 834921",5,true);}
            let click=(notice && (frame==90 || frame==91)).then(||Pos2::new(WIN_W/2.0+(h.app.current_width()-44.0)/2.0+26.0,15.0));
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),today:"2026-10-28".into(),date:"10月28日 週三".into(),media:media.clone(),cursor:click.or_else(||page.map(|_|H::local(300.0,40.0))),..Default::default()};
            let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(width,WIN_H))),time:Some(t),events:click.map(|pos|vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:frame==90,modifiers:Default::default()}]).unwrap_or_default(),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);});
            });
            raster.update_textures(&output.textures_delta);
            if [30,65,95,179].contains(&frame){
                raster.buf.fill([0.0;4]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut pixels=Vec::new();raster.to_rgba8(&mut pixels);
                if page.is_none() && notice && frame==95 {
                    if let Some(dir)=std::env::var_os("DAOZAI_GRAPHICS_PREVIEW") {let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();image::save_buffer(dir.join(format!("canvas-{width}.png")),&pixels,1280,270,image::ColorType::Rgba8).unwrap();}
                }
                rgba.extend(pixels);
            }
        }
        rgba
    }
    for page in std::iter::once(None).chain((0..8).map(Some)) {
        for notice in [false,true] {
            assert_eq!(draw(1280.0,page,notice),draw(960.0,page,notice),"縮減透明餘量不能改變頁面 {page:?} 和通知 {notice} 的畫素");
        }
    }
}

#[test]
fn external_notice_respects_permissions_masks_preview_and_never_reemits(){
 let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
 h.app.receive_notification("系統音量".into(),"external.test".into(),"驗證碼 123456".into());assert!(h.app.alerts.is_empty());
 h.app.settings.read_notifications=true;
 h.app.receive_notification("系統音量".into(),"external.test".into(),"驗證碼 123456".into());
 let a=h.app.alerts.back().unwrap();assert!(!a.is_hud());assert!(!a.body.contains("123456"));assert_eq!(a.page,usize::MAX);
 assert!(h.app.history.is_empty());assert!(!h.app.pending.iter().any(|c|matches!(c,crate::app::Cmd::Notify(..))));
 h.app.tick_alerts();assert!(h.app.notice.is_some());h.app.settings.read_notifications=false;h.app.tick_alerts();assert!(h.app.notice.is_none());
 h.app.settings.read_notifications=true;h.app.settings.notification_preview=true;
 h.app.receive_notification("測試".into(),"external.test".into(),"驗證碼 123456，請勿洩露".into());assert!(!h.app.alerts.back().unwrap().body.contains("123456"));
 h.app.tick_alerts();h.app.t+=4.6;h.app.tick_alerts();assert!(h.app.notice.is_none());
}

#[test]
fn discord_notice_waits_while_the_island_is_hidden_by_fullscreen_instead_of_expiring_unseen(){
 let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
 h.app.settings.read_notifications=true;h.app.settings.notification_preview=true;
 // 全螢幕影片擋住島時來的 Discord 通知：隱藏期間不倒數，島和鈴鐺都留著。
 h.app.fullscreen_hidden=true;
 h.app.receive_notification("Discord".into(),"com.squirrel.Discord.Discord".into(),"在嗎".into());
 h.app.tick_alerts();assert!(h.app.notice.is_some(),"通知先成為目前顯示的那一則");
 for _ in 0..6 {h.app.t+=10.0;h.app.tick_alerts();}
 assert!(h.app.notice.is_some(),"隱藏了一分鐘，通知不能過期");
 assert!(h.app.t<h.app.notification_bell_until,"鈴鐺也要等到島出現");
 // 離開全螢幕後才開始計時，使用者有完整的 4.5 秒。
 h.app.fullscreen_hidden=false;h.app.tick_alerts();
 h.app.t+=4.4;h.app.tick_alerts();assert!(h.app.notice.is_some(),"離開全螢幕後還在");
 h.app.t+=0.2;h.app.tick_alerts();assert!(h.app.notice.is_none(),"顯示滿 4.5 秒後照常收起");
 // 沒被全螢幕擋住時行為不變：一般外部通知 4.5 秒後過期。
 h.app.receive_notification("系統音量".into(),"external.test".into(),"內容".into());
 h.app.tick_alerts();assert!(h.app.notice.is_some());h.app.t+=4.6;h.app.tick_alerts();assert!(h.app.notice.is_none());
 // 內部提醒（例如行事曆）維持原本的計時，不因為這次修改被留住。
 h.app.fullscreen_hidden=true;
 h.app.alert("會議提醒","五分鐘後開始",7,false);h.app.tick_alerts();assert!(h.app.notice.is_some());
 h.app.t+=7.9;h.app.tick_alerts();assert!(h.app.notice.is_some(),"內部提醒維持自己的 8 秒");
 h.app.t+=0.2;h.app.tick_alerts();assert!(h.app.notice.is_none(),"內部提醒不受影響，隱藏期間照常過期");
}

#[test]
#[cfg(feature="cn-messengers")]
fn qq_only_notifications_filter_by_app_id_before_display_and_respect_preview(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.settings.read_notifications=true;h.app.settings.qq_only=true;h.app.settings.save();
    assert!(crate::model::Settings::load().qq_only);
    h.app.receive_notification("QQ".into(),"QQMusic".into(),"不屬於 QQ 聊天".into());assert!(h.app.alerts.is_empty());
    h.app.receive_notification("QQ".into(),"QQ".into(),"測試聯絡人 · 測試訊息".into());
    assert!(!h.app.alerts.back().unwrap().body.contains("測試聯絡人"));
    h.app.settings.notification_preview=true;
    h.app.receive_notification("QQ".into(),"QQ".into(),"測試聯絡人 · 測試訊息".into());
    assert_eq!(h.app.alerts.back().unwrap().body,"測試聯絡人 · 測試訊息");assert!(h.app.history.is_empty());
}

#[test]
fn wechat_notifications_work_without_toast_permission_and_stop_when_disabled(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.receive_wechat_reminder();assert!(h.app.alerts.is_empty());
    h.app.settings.wechat_notifications=true;
    h.app.settings.save();assert!(crate::model::Settings::load().wechat_notifications);
    assert!(!h.app.settings.read_notifications);
    h.app.receive_wechat_reminder();h.app.tick_alerts();
    let notice=h.app.notice.as_ref().unwrap();
    assert_eq!(notice.title,"微信");assert_eq!(notice.body,"有新訊息 · 請到微信檢視");
    assert!(h.app.history.is_empty());
    assert!(!h.app.pending.iter().any(|c|matches!(c,crate::app::Cmd::Notify(..))));
    h.app.settings.wechat_notifications=false;h.app.tick_alerts();assert!(h.app.notice.is_none());
    h.app.settings.wechat_notifications=true;h.app.settings.notifications=false;
    h.app.receive_wechat_reminder();assert!(h.app.alerts.is_empty());
}

#[test]
fn hidden_mail_masks_every_display_field_and_reveal_is_temporary(){
 let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
 let i=crate::mail::Item{received_unix:None,account:"a@example.com".into(),from:"123456".into(),subject:"驗證碼 123456".into(),time:"09:41".into(),code:Some("123456".into()),html:"<h1>驗證碼</h1><p><strong>123456</strong></p>".into(), summary:"驗證碼 1 2 3-4 5 6".into(),links:vec![],meeting:None,uid:42};
 h.app.mail.privacy.observe(&i,true,0.0,100);h.app.t=61.0;
 let shown=h.app.display_mail(&i);assert!(shown.code.is_none());for text in [shown.from.as_str(),shown.subject.as_str(),shown.summary.as_ref()]{assert!(!text.contains('1'));}
 assert!(shown.html.is_empty(),"隱藏驗證碼時 HTML 原文也必須隱藏");
 assert!(!h.app.mail_body_lines(&shown).iter().any(|line|line.text.contains("123456")));
 h.app.reveal_mail_code(&i);let revealed=h.app.display_mail(&i);assert!(revealed.code.is_some());
 assert_eq!(revealed.html,i.html);assert!(h.app.mail_body_lines(&revealed).iter().any(|line|line.text.contains("123456")));
 h.app.t+=15.0;let hidden=h.app.display_mail(&i);assert!(hidden.code.is_none()&&hidden.html.is_empty());
}

#[test]
fn compact_font_atlas_preserves_panel_and_notice_pixels(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(texture_side:usize,page:Option<usize>,notice:bool)->Vec<u8>{
        let mut h=H::new();h.app.settings.island_lyrics=true;
        let media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚風吹過了山崗\n[01:00]下一句"));
        if let Some(page)=page{h.app.show_page(page);}
        let mut raster=crate::preview_raster::Raster::new(1280,270);
        let mut rgba=Vec::new();
        for frame in 0..180 {
            if notice && frame==60{h.app.alert("收到新郵件","會議通知及驗證碼 834921",5,true);}
            let click=(notice && (frame==90 || frame==91)).then(||Pos2::new(WIN_W/2.0+(h.app.current_width()-44.0)/2.0+26.0,15.0));
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),today:"2026-10-28".into(),date:"10月28日 週三".into(),media:media.clone(),cursor:click.or_else(||page.map(|_|H::local(300.0,40.0))),..Default::default()};
            let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(960.0,WIN_H))),max_texture_side:Some(texture_side),time:Some(t),events:click.map(|pos|vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:frame==90,modifiers:Default::default()}]).unwrap_or_default(),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);});
            });
            raster.update_textures(&output.textures_delta);
            if [30,65,95,179].contains(&frame){
                raster.buf.fill([0.0;4]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut pixels=Vec::new();raster.to_rgba8(&mut pixels);
                if page.is_none() && notice && frame==95 {
                    if let Some(dir)=std::env::var_os("DAOZAI_GRAPHICS_PREVIEW") {let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();image::save_buffer(dir.join(format!("atlas-{texture_side}.png")),&pixels,1280,270,image::ColorType::Rgba8).unwrap();}
                }
                rgba.extend(pixels);
            }
        }
        rgba
    }
    for page in std::iter::once(None).chain((0..8).map(Some)) {
        for notice in [false,true] {
            let reference=draw(16384,page,notice);
            for limit in [4096,2048] {let other=draw(limit,page,notice);
                let max=reference.iter().zip(&other).map(|(a,b)|a.abs_diff(*b)).max().unwrap_or(0);let count=reference.iter().zip(&other).filter(|(a,b)|a!=b).count();
                // 圖集座標改變只允許少量通道有 1/255 的浮點取樣舍入差異（實測最多約 0.005% 的通道），肉眼不可見。
                assert!(max<=1 && count<reference.len()/1000,"圖集上限 {limit} 改變了頁面 {page:?}、通知 {notice} 的畫素：{max}/{count}");}
        }
    }
}
#[test]
fn compact_font_atlas_preserves_settings_pixels_at_multiple_scales(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(limit:usize,scale:f32)->Vec<Vec<u8>>{
        let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
        let output=std::sync::Arc::new(std::sync::Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,max_texture_side:Some(limit),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,560.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let mut raster=crate::preview_raster::Raster::new((690.0*scale)as usize,(560.0*scale)as usize);let mut result=Vec::new();
        for tab in 0..12 {h.app.settings_tab=tab;
            for frame in 0..3 {
                let root=h.ctx.run(egui::RawInput{max_texture_side:Some(limit),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
                let mut child=output.lock().unwrap().take().unwrap();
                raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);
                if frame==2 {
                    // 常駐預覽讀取真實時鐘，排除跨分鐘變化；固定時鐘已由主介面對照覆蓋。
                    if tab==10{child.shapes.retain(|s|!matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().len()==5 && t.galley.text().as_bytes()[2]==b':'));}
                    raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);result.push(rgba);
                }
                raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
        }result
    }
    for scale in [1.0,1.5,2.0] {
        let before=draw(16384,scale);
        for limit in [4096,2048] {let after=draw(limit,scale);
        for (tab,(before,after)) in before.iter().zip(&after).enumerate(){
            // 圖集座標改變只允許少量通道有 1/255 的浮點取樣舍入差異。
            let max=before.iter().zip(after).map(|(a,b)|a.abs_diff(*b)).max().unwrap_or(0);
            let count=before.iter().zip(after).filter(|(a,b)|a!=b).count();assert!(max<=1 && count<before.len()/1000,"設定頁 {tab} 在圖集上限 {limit}、縮放 {scale} 下出現可見差異：{max}/{count}");
        }}
    }
}

#[test]
fn mailbox_wheel_stays_in_list_with_hidden_cards_and_short_lists() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for mode in 0..4 {for count in [0,1,3,8] {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        h.app.mail.recent=(1..=count).map(|uid|crate::mail::Item{received_unix:None,account:"test@qq.com".into(),from:format!("來源 {uid}"),subject:format!("郵件 {uid}"),time:"11-04 09:41".into(),code:if mode==0{None}else{Some(format!("{uid:06}"))},html: String::new().into(), summary:String::new().into(),links:vec![],meeting:None,uid}).collect();
        if mode>=2 {for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}}
        h.app.settings.mail_code=mode!=2;
        let cursor=H::local(if mode==3 && count>0{220.0}else{65.0},90.0);
        h.app.show_page(5);h.hover(cursor,1.0);
        for delta in [300.0,-900.0,-300.0,900.0,300.0] {
            h.wheel(delta);h.hover(cursor,0.3);
            assert_eq!(h.app.page(),5,"模式 {mode}、{count} 封郵件：列表範圍內和滾動邊界不能觸發翻頁");
        }
    }}
}

#[test]
fn mailbox_compact_third_row_stays_visible_below_the_toolbar(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=(1..=6).map(|uid|crate::mail::Item {received_unix:None,account:"test@qq.com".into(),from:format!("來源 {uid}"),subject:format!("郵件 {uid}"),time:"09:41".into(),code:None,html: String::new().into(), summary:String::new().into(),links:vec![],meeting:None,uid}).collect();
    h.app.show_page(5);h.hover(H::local(200.0,90.0),1.0);
    let third=h.ctx.read_response(egui::Id::new(("card",true,("mail-row",3u32,"test@qq.com")))).expect("第三封郵件應完整可見");
    assert!(third.rect.bottom()<=H::local(380.0,122.0).y+0.5);
    h.click(third.rect.center());
    assert_eq!(h.app.mail.open_uid,Some(("test@qq.com".into(),3)));
}

#[test]
fn bluetooth_switch_clears_notices_and_restarts_with_fresh_baseline(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    let device=crate::bt::Device{id:"keyboard".into(),name:"Keyboard".into(),battery:None,kind:crate::bt::DeviceKind::Keyboard};
    h.app.bluetooth_alert(&device,true);h.frame(vec![]);
    h.app.bluetooth_alert(&device,false);
    h.app.settings.bluetooth_notifications=false;h.frame(vec![]);
    assert!(h.app.notice.as_ref().is_none_or(|a|a.title!="藍芽裝置"));
    assert!(!h.app.alerts.iter().any(|a|a.title=="藍芽裝置"));
    h.app.bluetooth_alert(&device,true);
    assert!(!h.app.alerts.iter().any(|a|a.title=="藍芽裝置"));
    h.app.settings.save();assert!(!crate::model::Settings::load().bluetooth_notifications);
    h.bt=vec![device];h.app.settings.bluetooth_notifications=true;h.frame(vec![]);
    assert!(h.app.notice.as_ref().is_none_or(|a|a.title!="藍芽裝置"));
    h.bt.clear();h.frame(vec![]);
    assert!(h.app.notice.as_ref().is_some_and(|a|a.title=="藍芽裝置") || h.app.alerts.iter().any(|a|a.title=="藍芽裝置"));
}

#[test]
#[ignore = "按需匯出五套皮膚的預覽圖：DAOZAI_SKIN_PREVIEW_DIR=目錄 cargo test skin_gallery -- --ignored"]
fn skin_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_SKIN_PREVIEW_DIR").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    let (w,hgt)=((crate::app::CANVAS_W*2.0) as usize,(WIN_H*2.0) as usize);
    // 日落漸變 + 幾條城市剪影，用來觀察半透明皮膚的通透感。
    let backdrop=|raster:&mut crate::preview_raster::Raster|{
        for y in 0..hgt {for x in 0..w {
            let t=y as f32/hgt as f32;let s=x as f32/w as f32;
            let mut c=[0.98-0.25*t+0.05*s,0.62+0.12*t-0.1*s,0.45+0.35*t,1.0];
            if (y>hgt*7/10) && ((x/37)%3!=0) && y>hgt*7/10+((x*7919)%97) {c=[0.30,0.33,0.45,1.0];}
            let cloud=((x as f32/90.0).sin()*(y as f32/40.0).cos()).max(0.0);if y<hgt/2 {for k in 0..3{c[k]=c[k]+(1.0-c[k])*cloud*0.6;}}
            if x%160<12 && y<hgt*6/10 {c=[0.18,0.20,0.30,1.0];}
            raster.buf[y*w+x]=c;
        }}
    };
    for (skin,shape) in crate::model::Skin::ALL.into_iter().flat_map(|s|crate::model::IslandShape::ALL.map(|p|(s,p))) {
        for open in [false,true] {
            let mut h=H::new();h.app.settings.skin=skin;h.app.settings.island_shape=shape;h.app.settings.pet=true;h.app.opts.pet_idle=0.0;
            h.media=crate::smtc::NowPlaying{title:"Paper Boats".into(),artist:"The Lanterns".into(),position:64.0,duration:201.0,playing:true,..Default::default()};
            let mut raster=crate::preview_raster::Raster::new(w,hgt);
            for frame in 0..120 {
                let t=frame as f64/60.0;
                let cursor=open.then_some(Pos2::new(WIN_W/2.0,14.0));
                let env=Env{t,clock:"11:13:00".into(),today:"2026-10-26".into(),date:"10月26日 週一".into(),media:h.media.clone(),cursor,backdrop:Some([0.85,0.62,0.6]),glass_blur:true,..Default::default()};
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),
                    viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                let mut glass=None;
                let output=h.ctx.run(raw,|ctx|{
                    egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));glass=h.app.frame(ui,win,&env).glass;});
                });
                raster.update_textures(&output.textures_delta);
                if open && frame==100 {h.app.show_page(1);}
                if open && frame==104 && shape==crate::model::IslandShape::Notch {
                    backdrop(&mut raster);
                    raster.draw(&h.ctx.tessellate(output.shapes.clone(),output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw(w as u32,hgt as u32,rgba).unwrap();
                    image::imageops::crop_imm(&image,120,0,1040,400).to_image().save(dest.join(format!("{}-switch.png",skin.key()))).unwrap();
                }
                if frame==119 {
                    backdrop(&mut raster);
                    // 模擬系統實時模糊：只有島體輪廓內的背景被模糊（與實機的合成層一致）。
                    if let Some(shapes)=glass {
                        let pieces=crate::app::blur_pieces(&shapes,2.0);
                        let mut blurred=raster.buf.clone();
                        for _ in 0..3 {
                            let src=blurred.clone();let r=18i32;
                            for y in 0..hgt as i32 {for x in 0..w as i32 {let mut s=[0.0f32;4];let mut n=0.0;for dx in (-r..=r).step_by(3){let xx=(x+dx).clamp(0,w as i32-1);let c=src[(y as usize)*w+xx as usize];for k in 0..4{s[k]+=c[k];}n+=1.0;}blurred[(y as usize)*w+x as usize]=s.map(|v|v/n);}}
                            let src=blurred.clone();
                            for y in 0..hgt as i32 {for x in 0..w as i32 {let mut s=[0.0f32;4];let mut n=0.0;for dy in (-r..=r).step_by(3){let yy=(y+dy).clamp(0,hgt as i32-1);let c=src[(yy as usize)*w+x as usize];for k in 0..4{s[k]+=c[k];}n+=1.0;}blurred[(y as usize)*w+x as usize]=s.map(|v|v/n);}}
                        }
                        for y in 0..hgt {for x in 0..w {
                            let (px,py)=(x as f32+0.5,y as f32+0.5);
                            let inside=pieces.iter().any(|p|{px>=p.x&&px<p.x+p.w&&py>=p.y&&py<p.y+p.h&&{let cx=px.clamp(p.x+p.radius,p.x+p.w-p.radius);let cy=py.clamp(p.y+p.radius,p.y+p.h-p.radius);(px-cx).powi(2)+(py-cy).powi(2)<=p.radius*p.radius}});
                            if inside {raster.buf[y*w+x]=blurred[y*w+x];}
                        }}
                    }
                    raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw(w as u32,hgt as u32,rgba).unwrap();
                    let crop=if open{image::imageops::crop_imm(&image,120,0,1040,400)}else{image::imageops::crop_imm(&image,300,0,680,110)};
                    crop.to_image().save(dest.join(format!("{}-{}-{}.png",skin.key(),shape.key(),if open{"open"}else{"closed"}))).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
    }
}

#[test]
#[ignore = "按需匯出：帶 AI Usage 與吉祥物的收起態，以及設定頁常駐排列預覽"]
fn skin_gallery_resident(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_SKIN_PREVIEW_DIR").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    for skin in crate::model::Skin::ALL {
        for shape in crate::model::IslandShape::ALL {
            let mut h=H::new();h.app.settings.skin=skin;h.app.settings.island_shape=shape;h.app.opts.pet_idle=0.0;
            h.quota_accounts(2);h.app.quota.config.colors.insert("preview-0".into(),crate::quota::Accent::Blue);
            // 島體收起態
            let (w,hgt)=((crate::app::CANVAS_W*2.0) as usize,(WIN_H*2.0) as usize);
            let mut raster=crate::preview_raster::Raster::new(w,hgt);
            for frame in 0..90 {
                let t=frame as f64/60.0;
                let env=Env{t,clock:"21:54:00".into(),today:"2026-11-04".into(),date:"11月4日 週三".into(),backdrop:Some([0.55,0.68,0.75]),glass_blur:true,..Default::default()};
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),
                    viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));h.app.frame(ui,win,&env);});});
                raster.update_textures(&output.textures_delta);
                if frame==89 {
                    raster.buf.fill([0.62,0.72,0.78,1.0]);
                    raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw(w as u32,hgt as u32,rgba).unwrap();
                    image::imageops::crop_imm(&image,300,0,680,90).to_image().save(dest.join(format!("bar-{}-{}.png",skin.key(),shape.key()))).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
            // 設定頁常駐排列預覽
            let mut raster=crate::preview_raster::Raster::new(1200,300);
            for frame in 0..3 {
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(600.0,150.0))),time:Some(frame as f64),
                    viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.resident_order_settings(ui);});});
                raster.update_textures(&output.textures_delta);
                if frame==2 {
                    raster.buf.fill([0.13,0.13,0.15,1.0]);
                    raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    image::RgbaImage::from_raw(1200,300,rgba).unwrap().save(dest.join(format!("resident-{}-{}.png",skin.key(),shape.key()))).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
    }
}

#[test]
#[ignore = "按需匯出：淺色皮膚下每個頁面的展開態，用於排查配色"]
fn skin_gallery_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_SKIN_PREVIEW_DIR").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    let skins=std::env::var("DAOZAI_SKINS").unwrap_or_else(|_|"glass_light,cream,classic".into());
    for skin in skins.split(',').map(crate::model::Skin::parse) {
        for page in 0..9usize {
            let mut h=H::new();h.app.settings.skin=skin;h.app.settings.island_shape=crate::model::IslandShape::Pill;
            h.quota_accounts(2);
            h.app.todos.items.push(crate::model::TodoItem{text:"整理島仔皮膚反饋".into(),done_on:None,due:Some("2026-11-05 10:00".into()),reminded:false,..Default::default()});
            h.app.todos.items.push(crate::model::TodoItem{text:"已完成的事項".into(),done_on:Some("2026-11-04".into()),due:None,reminded:false,..Default::default()});
            h.media=crate::smtc::NowPlaying{title:"Paper Boats".into(),artist:"The Lanterns".into(),position:64.0,duration:201.0,playing:true,..Default::default()};
            let (w,hgt)=((crate::app::CANVAS_W*2.0) as usize,(WIN_H*2.0) as usize);
            let mut raster=crate::preview_raster::Raster::new(w,hgt);
            for frame in 0..140 {
                let t=frame as f64/60.0;
                if frame==40 { if page==8 {h.app.open_calendar_editor(None);} else {h.app.show_page(page);} }
                let env=Env{t,clock:"22:46:00".into(),today:"2026-11-04".into(),date:"11月4日 週三".into(),media:h.media.clone(),cursor:Some(Pos2::new(WIN_W/2.0,20.0)),backdrop:Some([0.8,0.84,0.88]),glass_blur:true,..Default::default()};
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),
                    viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));h.app.frame(ui,win,&env);});});
                raster.update_textures(&output.textures_delta);
                if frame==139 {
                    raster.buf.fill([0.80,0.84,0.88,1.0]);
                    raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw(w as u32,hgt as u32,rgba).unwrap();
                    image::imageops::crop_imm(&image,160,0,960,500).to_image().save(dest.join(format!("page-{}-{page}.png",skin.key()))).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
    }
}

#[test]
#[ignore = "按需匯出 DeepSeek 餘額介面"]
fn deepseek_balance_gallery(){
    use crate::quota::{Provider,Event};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_DEEPSEEK_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for skin in [crate::model::Skin::Classic,crate::model::Skin::Cream,crate::model::Skin::GlassLight,crate::model::Skin::GlassDark]{
    for (name,amount,stale,count) in [("normal","4.18",false,2),("large","12800.00",false,2),("overflow","128000.00",false,5),("stale","12800.00",true,2),("loading","4.18",false,2),("error","4.18",false,2)] {for open in [false,true]{
        let mut h=H::new();h.app.settings.pet=false;h.app.settings.skin=skin;h.app.settings.show_battery=true;h.quota_accounts(count);
        h.app.quota.config.accounts[1].provider=Provider::Claude;h.app.quota.config.accounts[1].alias="Claude".into();h.app.quota.config.accounts[0].alias="Codex".into();
        let id=h.app.quota.config.connect_deepseek("preview".into(),"DeepSeek".into());
        let snapshot=crate::quota::parse(Provider::DeepSeek,&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":amount,"topped_up_balance":amount,"granted_balance":"0.00"}]})).unwrap();
        h.app.quota_event(Event{id:id.clone(),result:Ok(snapshot)});if stale{h.app.quota_event(Event{id:id.clone(),result:Err("無法連線".into())});}
        if name=="loading"{h.app.quota.rows.remove(&id);h.app.quota.busy.insert(id.clone());}
        if name=="error"{h.app.quota.rows.remove(&id);h.app.quota_event(Event{id,result:Err("暫時無法查詢餘額，請稍後重試".into())});}
        if open{h.app.show_page(6);}
        let mut raster=crate::preview_raster::Raster::new(1280,380);
        for frame in 0..90{
            let t=frame as f64/60.0;let env=Env{t,clock:"09:41:00".into(),battery:Some(80),charging:true,cursor:if open{Some(Pos2::new(320.0,60.0))}else{None},..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,190.0))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});
            raster.update_textures(&output.textures_delta);
            if frame==89{
                raster.buf.fill([0.09,0.11,0.18,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest.join(format!("{}-{name}-{}.png",skin.key(),if open{"open"}else{"closed"})),&rgba,1280,380,image::ColorType::Rgba8).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }}}
}

#[test]
fn deepseek_compact_amount_fits_text_and_opens_account(){
    use crate::quota::{Provider,Event};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.quota_accounts(5);h.app.settings.pet=false;h.app.settings.resident_order=vec![1,9,2,0,5,6,7];
    let id=h.app.quota.config.connect_deepseek("preview".into(),"DeepSeek".into());let mut width:Option<f32>=None;let mut amount_rect=Rect::NOTHING;
    for (amount,label) in [("4.18","¥4.1"),("12800.00","¥1.28萬"),("128000.00","¥12.8萬")]{
        let snapshot=crate::quota::parse(Provider::DeepSeek,&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":amount,"topped_up_balance":amount,"granted_balance":"0.00"}]})).unwrap();h.app.quota_event(Event{id:id.clone(),result:Ok(snapshot)});
        for _ in 0..90{h.frame(vec![]);}
        let text_rect=|label:&str|h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text) if text.galley.text()==label=>Some(text.visual_bounding_rect()),_=>None}).expect("常駐金額應可見");
        let fonts:Vec<_>=h.shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(text) if text.galley.text()==label||text.galley.text()=="09:41"=>Some(text.galley.job.sections[0].format.font_id.clone()),_=>None}).collect();assert_eq!(fonts.len(),2);assert_eq!(fonts[0],fonts[1],"餘額與時鐘使用相同字型和字號");
        amount_rect=text_rect(label);assert!(amount_rect.right()<h.region.right());
        let gap=text_rect("09:41").left()-amount_rect.right();assert!((10.0..20.0).contains(&gap),"金額與時鐘之間不應預留大片空白：{gap}");
        if let Some(short_width)=width{assert!(h.app.current_width()>short_width+10.0,"短金額必須釋放多餘佔位");}else{width=Some(h.app.current_width());}
    }
    h.click(amount_rect.center());assert_eq!(h.app.page(),6);assert_eq!(h.app.quota.config.selected,id);
}

#[test]
#[ignore = "按需匯出 DeepSeek 連線狀態提示"]
fn deepseek_connection_preview(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.quota.provider=crate::quota::Provider::DeepSeek;h.app.quota.note="DeepSeek 已連線".into();
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_DEEPSEEK_PREVIEW_DIR").expect("需指定預覽目錄"));
    let mut raster=crate::preview_raster::Raster::new(1200,1400);
    for frame in 0..8{
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(600.0,700.0))),time:Some(frame as f64/60.0),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.quota_settings(ui);});});raster.update_textures(&output.textures_delta);
        if frame==7{raster.buf.fill([0.1,0.1,0.1,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join("connection-status.png"),&rgba,1200,1400,image::ColorType::Rgba8).unwrap();}
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
fn deepseek_resident_preview_does_not_overlap_icons(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.quota_accounts(2);h.app.settings.skin=crate::model::Skin::Classic;
    let id=h.app.quota.config.connect_deepseek("preview".into(),"DeepSeek".into());h.app.quota_event(crate::quota::Event{id,result:crate::quota::parse(crate::quota::Provider::DeepSeek,&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"12800","topped_up_balance":"12800","granted_balance":"0"}]}))});
    let mut raster=crate::preview_raster::Raster::new(1200,380);
    for frame in 0..8{
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(600.0,190.0))),time:Some(frame as f64/60.0),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.resident_order_settings(ui);});});raster.update_textures(&output.textures_delta);
        if frame==7{
            let circles:Vec<_>=output.shapes.iter().filter_map(|shape|if let egui::Shape::Circle(c)=&shape.shape{Some(c)}else{None}).collect();
            let whale=output.shapes.iter().find_map(|shape|if let egui::Shape::Mesh(mesh)=&shape.shape{let rect=mesh.calc_bounds();((rect.width()-24.0).abs()<0.1).then_some(rect)}else{None}).expect("DeepSeek 圖示");
            let claude=circles.iter().find(|c|c.stroke.color==egui::Color32::from_rgb(255,116,26)&&c.radius>9.0).expect("Claude 圓環");
            assert!(whale.left()>claude.center.x+claude.radius+4.0,"DeepSeek 不應覆蓋前一枚圖示");
            if let Some(dest)=std::env::var_os("DAOZAI_DEEPSEEK_PREVIEW_DIR"){
                raster.buf.fill([0.1,0.1,0.1,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(std::path::PathBuf::from(dest).join("resident-preview.png"),&rgba,1200,380,image::ColorType::Rgba8).unwrap();
            }
        }
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
fn quota_cards_expose_names_colors_settings_and_connection(){
    fn frame(h:&mut H,events:Vec<egui::Event>){
        h.t+=1.0/60.0;let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(600.0,800.0))),time:Some(h.t),events,..Default::default()};
        let out=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.quota_settings(ui);});});h.shapes=out.shapes;
    }
    fn click(h:&mut H,label:&str){
        let pos=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).expect("操作入口應可見");
        frame(h,vec![egui::Event::PointerMoved(pos)]);
        frame(h,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        frame(h,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        frame(h,vec![]);
    }
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.quota_accounts(2);let id=h.app.quota.config.connect_deepseek("fixture".into(),"工作".into());h.app.quota_event(crate::quota::Event{id:id.clone(),result:Err("API Key 驗證失敗".into())});
    for _ in 0..8{frame(&mut h,vec![]);}
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="備註"||t.galley.text()=="服務")));
    for label in ["顏色","監控","工作","移除","當前"]{assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==label)),"應直接顯示 {label}");}
    click(&mut h,"工作");
    frame(&mut h,vec![egui::Event::Key{key:egui::Key::A,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers{ctrl:true,command:true,..Default::default()}},egui::Event::Text("備用帳號".into())]);
    assert_eq!(h.app.quota.config.accounts.iter().find(|a|a.id==id).unwrap().alias,"備用帳號","帳號名稱應可在行內修改");
    click(&mut h,"監控");assert!(!h.app.quota.config.accounts[0].enabled);click(&mut h,"監控");
    let handles:Vec<_>=h.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="::"=>Some(t.visual_bounding_rect().center()),_=>None}).collect();
    let from=handles[0];let to=handles[1];
    frame(&mut h,vec![egui::Event::PointerMoved(from),egui::Event::PointerButton{pos:from,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    frame(&mut h,vec![egui::Event::PointerMoved(from+egui::vec2(12.0,0.0))]);
    frame(&mut h,vec![egui::Event::PointerMoved(to)]);
    frame(&mut h,vec![egui::Event::PointerButton{pos:to,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    assert_eq!(h.app.quota.config.accounts[1].id,"preview-0","拖放應改變卡片順序");
    h.app.quota.config.move_account("preview-0","preview-1");for _ in 0..3{frame(&mut h,vec![]);}
    click(&mut h,"常駐島上");assert!(!h.app.quota.config.accounts[0].pinned);
    click(&mut h,"重試");assert!(h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::QuotaRefresh)));assert!(!h.app.quota.connect_open);
    click(&mut h,"重新連線");assert!(h.app.quota.connect_open);assert_eq!(h.app.quota.connect_target.as_deref(),Some("preview-0"));
    h.app.quota.connect_open=false;for _ in 0..8{frame(&mut h,vec![]);}click(&mut h,"＋ 新增帳號");assert!(h.app.quota.connect_open);assert!(h.app.quota.connect_target.is_none());assert!(h.app.quota.alias.is_empty());
    h.app.quota.connect_open=false;for _ in 0..8{frame(&mut h,vec![]);}assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="···"||t.galley.text()=="設定")));
}

#[test]
#[ignore = "按需匯出卡片式 AI Usage 設定頁"]
fn quota_cards_settings_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_DEEPSEEK_PREVIEW_DIR").expect("需指定預覽目錄"));
    for (name,width,light,dialog) in [("dark",690.0,false,false),("light",690.0,true,false),("wide",900.0,false,false),("connect",690.0,false,true),("narrow",600.0,false,false)]{
        let mut h=H::new();h.quota_accounts(2);h.app.quota.config.accounts[0].alias="本機帳號".into();h.app.quota.config.accounts[1].alias="桌面版 1".into();
        for (i,remaining) in [54.0,72.0].into_iter().enumerate(){let account_id=h.app.quota.config.accounts[i].id.clone();h.app.quota_event(crate::quota::Event{id:account_id,result:Ok(crate::quota::Snapshot{windows:vec![crate::quota::Window{label:"5 小時".into(),seconds:Some(18000),remaining:remaining+20.0,reset:None,amount:None},crate::quota::Window{label:"本週".into(),seconds:Some(604800),remaining,reset:None,amount:None}],at:chrono::Utc::now().timestamp(),..Default::default()})});}
        let id=h.app.quota.config.connect_deepseek("preview".into(),"DeepSeek".into());h.app.quota_event(crate::quota::Event{id,result:Err("API Key 驗證失敗，請重新連線帳號".into())});
        h.app.quota.connect_open=dialog;h.app.quota.provider=crate::quota::Provider::DeepSeek;
        h.app.settings.settings_theme=if light{crate::settings_theme::SettingsTheme::Light}else{crate::settings_theme::SettingsTheme::Dark};h.app.settings_tab=11;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let size=egui::vec2(width,650.0);let ppp=1.5;let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(ppp),focused:Some(true),..Default::default()})).collect(),focused:true,..Default::default()};*capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));});
        let (w,height)=((size.x*ppp)as u32,(size.y*ppp)as u32);let mut raster=crate::preview_raster::Raster::new(w as usize,height as usize);
        for frame in 0..8{let root=h.ctx.run(egui::RawInput{viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()},|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
            if frame==7{raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("quota-cards-{name}.png")),&rgba,w,height,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
        }
    }
}

/// 新功能畫面：按需匯出，檢查排版。
fn feature_raster()->crate::preview_raster::Raster{crate::preview_raster::Raster::new((crate::app::CANVAS_W*2.0) as usize,(WIN_H*2.0) as usize)}
fn shoot_feature(h:&mut H,raster:&mut crate::preview_raster::Raster,dest:&std::path::Path,name:&str,frames:usize,cursor:Option<Pos2>,today:&str){
    let ppp=2.0;let (w,hgt)=(crate::app::CANVAS_W,WIN_H);
    for f in 0..frames {
        h.t+=1.0/60.0;
        let env=Env{t:h.t,clock:"09:41:00".into(),today:today.into(),date:"11月4日 週三".into(),cursor,bt_ready:true,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
            events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let app=&mut h.app;
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));app.frame(ui,win,&env);});});
        raster.update_textures(&output.textures_delta);
        let primitives=h.ctx.tessellate(output.shapes,output.pixels_per_point);
        if f+1==frames {
            raster.buf.fill([0.20,0.24,0.32,1.0]);
            raster.draw(&primitives,output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let image=image::RgbaImage::from_raw((w*ppp) as u32,(hgt*ppp) as u32,rgba).unwrap();
            let crop_h=(h.app.expanded_height()*ppp+24.0).max(330.0).min(hgt*ppp) as u32;
            image::imageops::crop_imm(&image,200,0,880,crop_h).to_image().save(dest.join(format!("{name}.png"))).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
#[ignore="按需匯出每週待辦與剪貼簿的實際畫面"]
fn feature_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_FEATURE_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let today="2026-11-04";// 週三
    let todos=||{
        let mut a=crate::model::TodoItem::new("寫週報");a.repeat=0b001_0000;
        let mut b=crate::model::TodoItem::new("站會");b.repeat=crate::model::REPEAT_WORKDAYS;b.due=Some("2026-11-04 09:30".into());b.done_on=Some("2026-11-04".into());
        let mut c=crate::model::TodoItem::new("健身");c.repeat=0b000_0101;
        let d=crate::model::TodoItem::new("回覆設計評審意見");
        vec![a,b,c,d]
    };
    let mut h=H::new();let mut r=feature_raster();h.app.todos.items=todos();h.app.show_page(4);
    shoot_feature(&mut h,&mut r,&dest,"todo-page",90,Some(H::local(120.0,60.0)),today);
    let mut h=H::new();let mut r=feature_raster();h.app.todos.items=todos();h.app.show_page(4);
    shoot_feature(&mut h,&mut r,&dest,"todo-editor-0",30,Some(H::local(120.0,60.0)),today);
    h.app.open_task_dialog(2);
    shoot_feature(&mut h,&mut r,&dest,"todo-editor",60,Some(H::local(120.0,60.0)),today);
    let mut h=H::new();let mut r=feature_raster();h.app.todos.items=todos();h.app.show_page(4);
    shoot_feature(&mut h,&mut r,&dest,"todo-picker-0",30,Some(H::local(120.0,60.0)),today);
    h.app.open_task_reminder(3);
    shoot_feature(&mut h,&mut r,&dest,"todo-picker",60,Some(H::local(120.0,60.0)),today);
    // 剪貼簿
    let mut h=H::new();let mut r=feature_raster();h.app.show_page(3);h.app.shelf_tab=1;
    shoot_feature(&mut h,&mut r,&dest,"clip-empty",60,Some(H::local(120.0,60.0)),today);
    {let mut hist=h.app.clipboard.lock();hist.items.clear();
        for (i,t) in ["https://github.com/binghao9312/daozai-tw/releases","會議紀要：\n1. 週三前確認島仔豎版方案\n2. 剪貼簿記錄上線","13800138000","SELECT * FROM todo WHERE repeat > 0 ORDER BY due;","驗證碼 482913"].iter().enumerate(){hist.push(t,&format!("09:{:02}",30+i));}
        hist.items[3].pinned=true;}
    shoot_feature(&mut h,&mut r,&dest,"clip-list",30,Some(H::local(200.0,66.0)),today);
    h.app.shelf_tab=0;
    let file=std::env::temp_dir().join("島仔週報.docx");std::fs::write(&file,b"x").unwrap();h.app.shelf.items=vec![file];
    shoot_feature(&mut h,&mut r,&dest,"shelf-files",30,Some(H::local(120.0,60.0)),today);
    h.app.confirm_clear=Some(h.app.t);
    shoot_feature(&mut h,&mut r,&dest,"shelf-confirm",5,Some(H::local(120.0,60.0)),today);
    h.app.settings.clipboard_history=false;h.app.shelf_tab=1;
    shoot_feature(&mut h,&mut r,&dest,"clip-off",30,Some(H::local(120.0,60.0)),today);
}

static SIDE_MUSIC:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
/// 側邊停靠的畫面：畫布為 SIDE_W × SIDE_H。
fn shoot_side(h:&mut H,raster:&mut crate::preview_raster::Raster,dest:&std::path::Path,name:&str,frames:usize,cursor:Option<Pos2>){
    let ppp=2.0;let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    for f in 0..frames {
        h.t+=1.0/60.0;
        let mut env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-04".into(),date:"11月4日 週三".into(),cursor,bt_ready:true,battery:Some(76),..Default::default()};
        if SIDE_MUSIC.load(std::sync::atomic::Ordering::Relaxed) {env.media.title="晴天".into();env.media.artist="周杰倫".into();env.media.playing=true;env.media.source="cloudmusic.exe".into();env.media.duration=269.0;}
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
            events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let app=&mut h.app;let mut out=None;
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt));out=Some(app.frame(ui,win,&env));});});
        raster.update_textures(&output.textures_delta);
        if f+1==frames {
            raster.buf.fill([0.20,0.24,0.32,1.0]);
            raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let mut image=image::RgbaImage::from_raw((w*ppp) as u32,(hgt*ppp) as u32,rgba).unwrap();
            // 畫出視窗區域（紅）與命中區域（綠）的邊框，方便檢查。
            if let Some(out)=out {
                for (r,color) in [(out.region,image::Rgba([255,80,80,255])),(out.hit,image::Rgba([80,255,120,255]))] {
                    let (x0,y0,x1,y1)=((r.left()*ppp).max(0.0) as u32,(r.top()*ppp).max(0.0) as u32,((r.right()*ppp) as u32).min(image.width()-1),((r.bottom()*ppp) as u32).min(image.height()-1));
                    for x in x0..=x1 {image.put_pixel(x,y0,color);image.put_pixel(x,y1,color);}
                    for y in y0..=y1 {image.put_pixel(x0,y,color);image.put_pixel(x1,y,color);}
                }
            }
            image.save(dest.join(format!("{name}.png"))).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
#[ignore="按需匯出左右停靠的實際畫面"]
fn side_dock_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_FEATURE_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    for (dock,shape,skin) in [(crate::model::Dock::Left,crate::model::IslandShape::Notch,crate::model::Skin::Classic),(crate::model::Dock::Right,crate::model::IslandShape::Pill,crate::model::Skin::Cream)] {
        let tag=format!("{}-{}",dock.key(),shape.key());
        let mut h=H::new();let mut r=feature_raster_sized(w,hgt);
        h.app.settings.dock=dock;h.app.settings.island_shape=shape;h.app.settings.skin=skin;h.app.settings.pet=false;
        h.app.media.title="晴天".into();h.app.media.artist="周杰倫".into();h.app.media.playing=true;h.app.media.source="cloudmusic.exe".into();
        let away=Some(Pos2::new(w/2.0,hgt-10.0));
        shoot_side(&mut h,&mut r,&dest,&format!("side-{tag}-collapsed"),60,away);
        let edge_x=if dock==crate::model::Dock::Left{8.0}else{w-8.0};
        shoot_side(&mut h,&mut r,&dest,&format!("side-{tag}-hover"),8,Some(Pos2::new(edge_x,hgt/2.0)));
        shoot_side(&mut h,&mut r,&dest,&format!("side-{tag}-opening"),12,Some(Pos2::new(edge_x,hgt/2.0)));
        shoot_side(&mut h,&mut r,&dest,&format!("side-{tag}-open"),70,Some(Pos2::new(edge_x,hgt/2.0)));
        h.app.show_page(4);
        shoot_side(&mut h,&mut r,&dest,&format!("side-{tag}-todo"),50,Some(Pos2::new(edge_x,hgt/2.0)));
        shoot_side(&mut h,&mut r,&dest,&format!("side-{tag}-closed"),320,away);
        h.app.alert("待辦到時間了","回覆設計評審意見",4,false);
        shoot_side(&mut h,&mut r,&dest,&format!("side-{tag}-notice"),40,away);
    }
    // 播放音樂 + 專注中 + 毛玻璃；極簡模式；側邊提示。
    SIDE_MUSIC.store(true,std::sync::atomic::Ordering::Relaxed);
    let mut h=H::new();let mut r=feature_raster_sized(w,hgt);
    h.app.settings.dock=crate::model::Dock::Left;h.app.settings.skin=crate::model::Skin::GlassDark;
    h.app.pomo.running=true;h.app.pomo.remaining=1430.0;
    side_quota_fixture(&mut h);
    let away=Some(Pos2::new(w/2.0,hgt-10.0));
    shoot_side(&mut h,&mut r,&dest,"side-left-glass-busy",60,away);
    shoot_side(&mut h,&mut r,&dest,"side-left-glass-open",70,Some(Pos2::new(8.0,hgt/2.0)));
    shoot_side(&mut h,&mut r,&dest,"side-left-glass-closed",120,away);
    h.app.code_copied("1");
    shoot_side(&mut h,&mut r,&dest,"side-left-toast",20,away);
    SIDE_MUSIC.store(false,std::sync::atomic::Ordering::Relaxed);
    let mut h=H::new();let mut r=feature_raster_sized(w,hgt);
    h.app.settings.dock=crate::model::Dock::Right;h.app.settings.minimal=true;
    shoot_side(&mut h,&mut r,&dest,"side-right-minimal",60,away);
}
fn feature_raster_sized(w:f32,h:f32)->crate::preview_raster::Raster{crate::preview_raster::Raster::new((w*2.0) as usize,(h*2.0) as usize)}

/// 三個環形額度賬號，隨後顯示 WorkBuddy 與 DeepSeek 餘額。
fn side_quota_fixture(h:&mut H)->(String,String){
    use crate::quota::{Provider,Event};
    h.quota_accounts(3);
    let wb=h.app.quota.config.connect_workbuddy("fixture".into(),"WorkBuddy".into());
    let ds=h.app.quota.config.connect_deepseek("fixture".into(),"DeepSeek".into());
    h.app.quota_event(Event{id:wb.clone(),result:crate::quota::parse(Provider::WorkBuddy,&serde_json::json!({"code":0,"data":{"Packages":[{"CycleRemainCapacity":"28560"}]}}))});
    h.app.quota_event(Event{id:ds.clone(),result:crate::quota::parse(Provider::DeepSeek,&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"4.18","granted_balance":"0","topped_up_balance":"4.18"}]}))});
    (wb,ds)
}

#[test]
fn side_dock_stacks_ai_quota_and_opens_the_account(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    let mut h=H::new();h.app.settings.dock=crate::model::Dock::Left;
    let (wb,_)=side_quota_fixture(&mut h);
    let away=Some(Pos2::new(w/2.0,hgt-5.0));
    for _ in 0..60 {side_frame(&mut h,away);}
    let items=h.app.quota_side_items();
    assert_eq!(items.len(),h.app.quota.config.pinned_indices().len());
    let order:Vec<_>=items.iter().map(|(i,_)|h.app.quota.config.accounts[*i].provider).collect();
    assert!(items.iter().any(|(i,_)|h.app.quota.config.accounts[*i].id==wb),"WorkBuddy 應在豎向用量裡");
    // 膠囊變長以容納額度，仍完整留在畫布內並豎直居中。
    let out=side_frame(&mut h,away);
    assert!(out.hit.height()>32.0+38.0*2.0,"{:?} {order:?}",out.hit);
    assert!(out.hit.top()>0.0 && out.hit.bottom()<hgt);
    // 點選 WorkBuddy 那一項（按豎排順序算出它的中心）：選中該賬號並開啟 AI 額度頁。
    let top=h.app.island_rect(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt)),h.app.w.pos,h.app.h.pos).top();
    let total=h.app.side_bar_len()-14.0;
    let mut y=top+(h.app.h.pos-total)/2.0;
    for item in h.app.side_items() {
        let height=match item {crate::side_dock::SideItem::Quota(i,hh) if h.app.quota.config.accounts[i].id==wb=>{y+=hh/2.0;break;},crate::side_dock::SideItem::Quota(_,hh)=>hh,crate::side_dock::SideItem::Clock=>32.0,crate::side_dock::SideItem::Weather=>40.0,crate::side_dock::SideItem::Battery=>20.0,crate::side_dock::SideItem::Music=>18.0,crate::side_dock::SideItem::Focus|crate::side_dock::SideItem::Bell=>22.0};
        y+=height;
    }
    let click=Pos2::new(18.0,y);
    side_frame(&mut h,Some(click));
    {
        let (w,hgt)=(w,hgt);
        h.t+=1.0/60.0;
        let env=Env{t:h.t,clock:"09:41:00".into(),cursor:Some(click),..Default::default()};
        for events in [vec![egui::Event::PointerMoved(click),egui::Event::PointerButton{pos:click,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}],vec![egui::Event::PointerButton{pos:click,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]] {
            h.t+=1.0/60.0;
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,events,..Default::default()};
            let app=&mut h.app;let env=Env{t:h.t,..env.clone()};
            let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt)),&env);});});
        }
    }
    assert_eq!(h.app.quota.config.selected,wb);
    assert_eq!(h.app.page(),6);
}

#[test]
fn minimal_side_pill_returns_to_line_while_wechat_is_unread(){
    use crate::model::{Dock,IslandShape};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Right,Dock::Left] {
        let mut h=H::new();
        h.app.settings.dock=dock;
        h.app.settings.island_shape=IslandShape::Pill;
        h.app.settings.minimal=true;
        h.app.settings.wechat_notifications=true;
        for _ in 0..90 {side_frame_with(&mut h,None);}
        let idle=side_frame_with(&mut h,None);
        h.app.receive_wechat_reminder_from(4242,0x1234);
        for _ in 0..30 {side_frame_with(&mut h,None);}
        assert_eq!(h.app.notice.as_ref().and_then(|a|a.messenger()),Some(crate::app::Messenger::Wechat));
        for _ in 0..480 {side_frame_with(&mut h,None);}
        let collapsed=side_frame_with(&mut h,None);
        assert!(h.app.notice.is_none() && collapsed.notice_hit.is_none());
        assert_eq!(h.app.pending_messengers.len(),1);
        assert_eq!(collapsed.hit,idle.hit,"{dock:?} 有未讀時也應收回細線");
        assert_eq!(collapsed.region,idle.region);
        let edge=Some(idle.hit.center());
        for _ in 0..90 {side_frame_with(&mut h,edge);}
        assert!(h.app.open && side_frame_with(&mut h,edge).notice_hit.is_some());
    }
}

#[test]
#[cfg(feature="cn-messengers")]
fn automatic_minimal_hides_unread_hit_area_until_reopened(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [crate::model::Dock::Top,crate::model::Dock::Left,crate::model::Dock::Right] {
        let mut h=H::new();
        h.app.settings.dock=dock;
        h.app.settings.browser_minimal=true;
        h.app.settings.wechat_notifications=true;
        h.app.settings.read_notifications=true;
        h.app.receive_wechat_reminder();
        h.app.receive_notification("QQ".into(),"QQ".into(),"測試".into());
        h.app.alerts.clear();
        h.app.notice=None;
        h.app.notification_bell_until=0.0;
        h.browser_at_top=Some(true);
        for _ in 0..90 {side_frame_with(&mut h,None);}
        let out=side_frame_with(&mut h,None);
        assert!(out.notice_hit.is_none(),"{dock:?} 避讓時未讀仍佔用滑鼠區域");
        assert_eq!(h.app.pending_messengers.len(),2);
        h.app.show_page(0);
        for _ in 0..60 {side_frame_with(&mut h,None);}
        assert!(side_frame_with(&mut h,None).notice_hit.is_some());
        h.browser_at_top=Some(false);
        for _ in 0..360 {side_frame_with(&mut h,None);}
        assert!(!h.app.open && side_frame_with(&mut h,None).notice_hit.is_some());
    }
}

#[test]
fn side_dock_auto_minimal_follows_avoidance(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    let mut h=H::new();h.app.settings.dock=crate::model::Dock::Right;h.app.settings.browser_minimal=true;
    let away=Some(Pos2::new(w/2.0,hgt-5.0));
    for _ in 0..60 {side_frame(&mut h,away);}
    let normal=side_frame(&mut h,away).hit;
    h.browser_at_top=Some(true);
    let mut out=side_frame_with(&mut h,away);
    for _ in 0..60 {out=side_frame_with(&mut h,away);}
    assert!(out.hit.width()<normal.width() && out.hit.height()<normal.height(),"避讓時收成貼邊細線：{:?} → {:?}",normal,out.hit);
    h.browser_at_top=Some(false);
    for _ in 0..60 {out=side_frame_with(&mut h,away);}
    assert!((out.hit.height()-normal.height()).abs()<1.0,"離開後恢復：{:?}",out.hit);
}

fn side_frame_with(h:&mut H,cursor:Option<Pos2>)->crate::app::FrameOut{
    let at_top=h.browser_at_top;
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    h.t+=1.0/60.0;
    let env=Env{t:h.t,clock:"09:41:00".into(),cursor,battery:Some(76),browser_at_top:at_top,..Default::default()};
    let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,..Default::default()};
    let app=&mut h.app;let mut out=None;
    let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{out=Some(app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt)),&env));});});
    out.unwrap()
}

fn side_frame(h:&mut H,cursor:Option<Pos2>)->crate::app::FrameOut{
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    h.t+=1.0/60.0;
    let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-04".into(),date:"11月4日 週三".into(),cursor,bt_ready:true,battery:Some(76),..Default::default()};
    let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
        events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),..Default::default()};
    let app=&mut h.app;let mut out=None;
    let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{out=Some(app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt)),&env));});});
    h.shapes=output.shapes;
    out.unwrap()
}

#[test]
fn side_dock_opens_from_the_edge_and_collapses_back(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    for (dock,shape) in [(crate::model::Dock::Left,crate::model::IslandShape::Notch),(crate::model::Dock::Right,crate::model::IslandShape::Pill)] {
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.island_shape=shape;
        let away=Some(Pos2::new(w/2.0,hgt-5.0));
        let mut out=side_frame(&mut h,away);
        for _ in 0..60 {out=side_frame(&mut h,away);}
        assert!(!h.app.open);
        // 收起：豎向膠囊貼邊、豎直居中。
        assert!(out.hit.height()>out.hit.width(),"{dock:?} 收起時是豎向：{:?}",out.hit);
        assert!((out.hit.center().y-hgt/2.0).abs()<1.0);
        let edge=if dock==crate::model::Dock::Left{0.5}else{w-0.5};
        assert!(out.hit.contains(Pos2::new(edge,hgt/2.0)),"{dock:?} 遊標推到螢幕邊緣也能命中：{:?}",out.hit);
        assert!(out.region.contains_rect(out.hit.shrink(0.5)));
        // 推到邊緣停留 → 展開，面板從邊緣滑出且完整留在畫布內。
        for _ in 0..60 {out=side_frame(&mut h,Some(Pos2::new(edge,hgt/2.0)));}
        assert!(h.app.open && h.app.progress()>0.95,"{dock:?} 應展開");
        assert!(out.hit.width()>crate::app::PW-1.0 && out.hit.height()<200.0,"{:?}",out.hit);
        assert!(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt)).contains_rect(out.region),"區域不能超出畫布：{:?}",out.region);
        if dock==crate::model::Dock::Left {assert!(out.hit.left()<=0.5)} else {assert!(out.hit.right()>=w-0.5)}
        // 離開 → 收起。
        for _ in 0..90 {out=side_frame(&mut h,away);}
        assert!(!h.app.open && h.app.progress()<0.05);
        assert!(out.hit.width()<60.0);
    }
}

#[test]
fn workbuddy_and_deepseek_balances_stay_ordered_and_clickable(){
    use crate::quota::{Provider,Event};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.quota_accounts(5);h.app.settings.pet=false;
    let wb=h.app.quota.config.connect_workbuddy("fixture".into(),"WorkBuddy".into());
    let ds=h.app.quota.config.connect_deepseek("fixture".into(),"DeepSeek".into());
    h.app.quota_event(Event{id:wb.clone(),result:crate::quota::parse(Provider::WorkBuddy,&serde_json::json!({"code":0,"data":{"Packages":[{"CycleRemainCapacity":"28560"}]}}))});
    h.app.quota_event(Event{id:ds.clone(),result:crate::quota::parse(Provider::DeepSeek,&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"4.18","granted_balance":"0","topped_up_balance":"4.18"}]}))});
    for _ in 0..90{h.frame(vec![]);}
    let rect=|label:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None}).expect("兩個餘額應同時顯示");
    let credits=rect("2.85萬");let balance=rect("¥4.1");assert!(credits.right()+20.0<balance.left());
    assert_eq!(h.app.quota_bar_indices().iter().skip(5).map(|i|h.app.quota.config.accounts[*i].provider).collect::<Vec<_>>(),vec![Provider::WorkBuddy,Provider::DeepSeek]);
    h.click(credits.center());assert_eq!(h.app.quota.config.selected,wb);assert_eq!(h.app.page(),6);
}

#[test]
#[ignore = "按需匯出 WorkBuddy 與 DeepSeek 並排介面"]
fn workbuddy_balance_gallery(){
    use crate::quota::{Provider,Event};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_DEEPSEEK_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for skin in [crate::model::Skin::Classic,crate::model::Skin::Cream,crate::model::Skin::GlassLight,crate::model::Skin::GlassDark]{
    for (name,amount,stale,count) in [("workbuddy","4.18",false,3)] {for open in [false,true]{
        let mut h=H::new();h.app.settings.pet=false;h.app.settings.skin=skin;h.app.settings.show_battery=true;h.quota_accounts(count);
        h.app.quota.config.accounts[1].provider=Provider::Claude;h.app.quota.config.accounts[1].alias="Claude".into();h.app.quota.config.accounts[0].alias="Codex".into();
        let id=h.app.quota.config.connect_deepseek("preview".into(),"DeepSeek".into());
        let snapshot=crate::quota::parse(Provider::DeepSeek,&serde_json::json!({"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":amount,"topped_up_balance":amount,"granted_balance":"0.00"}]})).unwrap();
        h.app.quota_event(Event{id:id.clone(),result:Ok(snapshot)});if stale{h.app.quota_event(Event{id:id.clone(),result:Err("無法連線".into())});}
        if name=="loading"{h.app.quota.rows.remove(&id);h.app.quota.busy.insert(id.clone());}
        if name=="error"{h.app.quota.rows.remove(&id);h.app.quota_event(Event{id,result:Err("暫時無法查詢餘額，請稍後重試".into())});}
        let wb=h.app.quota.config.connect_workbuddy("preview".into(),"WorkBuddy".into());
        h.app.quota_event(Event{id:wb,result:crate::quota::parse(Provider::WorkBuddy,&serde_json::json!({"code":0,"data":{"Packages":[{"CycleRemainCapacity":"2000"}],"CreditGroups":[{"Kind":"plan_base","CycleRemainCapacity":"500"},{"Kind":"benefit","CycleRemainCapacity":"1500"}]}}))});
        if open{h.app.show_page(6);}
        let mut raster=crate::preview_raster::Raster::new(1280,380);
        for frame in 0..90{
            let t=frame as f64/60.0;let env=Env{t,clock:"09:41:00".into(),battery:Some(80),charging:true,cursor:if open{Some(Pos2::new(320.0,60.0))}else{None},..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,190.0))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});
            raster.update_textures(&output.textures_delta);
            if frame==89{
                raster.buf.fill([0.09,0.11,0.18,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest.join(format!("{}-{name}-{}.png",skin.key(),if open{"open"}else{"closed"})),&rgba,1280,380,image::ColorType::Rgba8).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }}}
}
#[test]
fn new_mail_notification_masks_codes_in_sender_and_history(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account{email:"a@example.com".into(),provider:crate::mail::Provider::Qq,custom:None,enabled:true}];
    let item=crate::mail::Item{received_unix:None,account:"a@example.com".into(),from:"登入驗證碼 123456".into(),subject:"驗證碼 123456".into(),time:"09:41".into(),code:Some("123456".into()),html: String::new().into(), summary:"驗證碼 123456".into(),links:vec![],meeting:None,uid:42};
    h.app.mail_event(crate::mail::Event::Mail{item,new:true});
    assert!(!h.app.history.is_empty());
    for alert in h.app.alerts.iter().chain(h.app.history.iter()) {
        assert!(!alert.body.contains("123456"),"通知和歷史記錄不能留下驗證碼明文");
    }
}

#[test]
fn notification_preview_masks_fullwidth_and_zero_width_codes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.read_notifications=true;h.app.settings.notification_preview=true;
    for body in ["驗證碼 １２３４５６，請勿洩露","驗證碼 12\u{200b}3456，請勿洩露"] {
        h.app.receive_notification("測試".into(),"external.test".into(),body.into());
        assert_eq!(h.app.alerts.back().unwrap().body,"驗證碼 ••••••，請勿洩露");
    }
}

/// 記憶體實測：模擬長時間聽歌（不斷出現新漢字）並瀏覽各頁面，輸出程式記憶體、字形圖集和貼圖佔用。
/// 執行：DAOZAI_ATLAS=4096 DAOZAI_PPP=1.5 cargo test --lib memory_profile -- --ignored --nocapture --test-threads=1
#[test]
#[ignore="記憶體實測，按需執行"]
fn memory_profile_long_listening_session(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let side:usize=std::env::var("DAOZAI_ATLAS").ok().and_then(|s|s.parse().ok()).unwrap_or(4096);
    let ppp:f32=std::env::var("DAOZAI_PPP").ok().and_then(|s|s.parse().ok()).unwrap_or(1.5);
    let songs:usize=std::env::var("DAOZAI_SONGS").ok().and_then(|s|s.parse().ok()).unwrap_or(120);
    // 匿名記憶體≈Windows 的私有記憶體；檔案對映（程式程式碼、系統字型）可在程式間共享，不計入私有記憶體。
    fn rss()->String{
        let status=std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let kb=|key:&str|status.lines().find_map(|l|l.strip_prefix(key)?.trim().trim_end_matches(" kB").trim().parse::<f64>().ok()).unwrap_or(0.0)/1024.0;
        format!("匿名={:>6.1}MB 檔案對映={:>6.1}MB",kb("RssAnon:"),kb("RssFile:"))
    }
    fn report(label:&str,h:&H){
        let [w,hgt]=h.ctx.fonts(|f|f.font_image_size());
        let tex:usize=h.ctx.tex_manager().read().allocated().map(|(_,m)|m.bytes_used()).sum();
        let galleys=h.ctx.fonts(|f|f.num_galleys_in_cache());
        // 圖集在 CPU 端按 f32 存覆蓋率，顯示卡端再按 RGBA 存一份。
        println!("{label:<16} {} 圖集={w}x{hgt}（CPU≈{:.1}MB）貼圖合計={:.1}MB 文字快取={galleys}",rss(),(w*hgt*4) as f64/1048576.0,tex as f64/1048576.0);
    }
    println!("{:<16} {}","測試程式基線",rss());
    let mut h=H::new();h.app.settings.island_lyrics=true;h.app.show_lyrics=true;
    println!("{:<16} {}","建立介面後",rss());
    let mut seed=12345u64;let mut next=||{seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);(seed>>33) as u32};
    let run=|h:&mut H,media:&crate::smtc::NowPlaying|{
        h.t+=1.0/60.0;
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),max_texture_side:Some(side),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-10-28".into(),date:"10月28日 週三".into(),media:media.clone(),cursor:h.cursor,bt_ready:true,..Default::default()};
        let app=&mut h.app;
        let out=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let _=app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);});});
        h.shapes=out.shapes;
    };
    let idle=crate::smtc::NowPlaying::default();
    for _ in 0..30 {run(&mut h,&idle);}
    report("啟動後",&h);
    for page in 0..8 {h.app.show_page(page);for _ in 0..20 {run(&mut h,&idle);}}
    report("瀏覽8個頁面後",&h);
    for song in 0..songs {
        // 常用字約 3000 個：每首歌 40 句、每句 8–12 字，從中隨機取，接近真實歌詞的用字分佈。
        let lines:Vec<String>=(0..40).map(|_|(0..8+next()%5).map(|_|char::from_u32(0x4E00+next()%3000).unwrap()).collect()).collect();
        let lrc=lines.iter().enumerate().map(|(i,l)|format!("[{:02}:{:02}.00]{l}\n",i*3/60,i*3%60)).collect::<String>();
        let title=format!("測試歌曲{song}");
        h.app.set_lyrics(&title,"歌手",200.0,crate::lyrics::parse_lrc(&lrc));
        if song==songs/2 {h.app.show_page(0);}
        for i in 0..40 {
            let media=crate::smtc::NowPlaying{title:title.clone(),artist:"歌手".into(),duration:200.0,position:i as f64*3.0+0.5,playing:false,..Default::default()};
            for _ in 0..2 {run(&mut h,&media);}
        }
        if (song+1)%20==0 {report(&format!("聽完{}首",song+1),&h);}
    }
}

#[test]
#[ignore = "按需匯出設定頁預覽：DAOZAI_SETTINGS_PREVIEW_DIR=目錄 cargo test settings_gallery -- --ignored"]
fn settings_gallery(){
    use crate::settings_theme::SettingsTheme;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_SETTINGS_PREVIEW_DIR").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    let width:f32=std::env::var("DAOZAI_SETTINGS_WIDTH").ok().and_then(|v|v.parse().ok()).unwrap_or(690.0);
    let (w,hgt)=(width,2400.0f32);
    let scale=1.5f32;
    for (mode,dark) in [(SettingsTheme::Dark,true),(SettingsTheme::Light,false)] {
        for tab in [0usize,10,1,2,4,6,7,11,9] {
            let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
            h.quota_accounts(2);
            h.app.settings.settings_theme=mode;h.app.settings_tab=tab;
            let output=std::sync::Arc::new(std::sync::Mutex::new(None));let capture=output.clone();
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
                let id=viewport.ids.this;
                let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(w,hgt))),
                    viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
                *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
            });
            let mut raster=crate::preview_raster::Raster::new((w*scale) as usize,(hgt*scale) as usize);
            for frame in 0..4 {
                let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
                raster.update_textures(&root.textures_delta);
                let child=output.lock().unwrap().take().unwrap();
                raster.update_textures(&child.textures_delta);
                if frame==3 {
                    raster.buf.fill(if dark{[0.08,0.08,0.09,1.0]}else{[0.96,0.96,0.97,1.0]});
                    raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw((w*scale) as u32,(hgt*scale) as u32,rgba).unwrap();
                    // 去掉頁面下方的空白
                    let bg=image.get_pixel(((w-20.0)*scale) as u32,((hgt-5.0)*scale) as u32).0;
                    let mut bottom=image.height()-1;
                    'scan: while bottom>200 { for x in (270*scale as u32)..image.width()-10 { if image.get_pixel(x,bottom).0!=bg {break 'scan;} } bottom-=1; }
                    image::imageops::crop_imm(&image,0,0,image.width(),(bottom+20).min(image.height())).to_image()
                        .save(dest.join(format!("settings-{}-{tab:02}.png",if dark{"dark"}else{"light"}))).unwrap();
                }
                raster.free_textures(&child.textures_delta);
            }
        }
    }
}

#[test]
#[ignore="按需匯出訊息提醒與帳號狀態提示的實際樣式"]
fn messenger_notice_status_gallery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_MESSENGER_PREVIEW").expect("需指定預覽目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    for case in 0..15{
        let mut h=H::new();h.app.settings.pet=true;h.app.settings.wechat_notifications=true;
        h.app.settings.read_notifications=true;h.app.settings.notification_preview=true;
        h.app.settings.bluetooth_notifications=true;
        let ppp=2.0;let (w,height)=(crate::app::CANVAS_W,WIN_H);
        let mut raster=crate::preview_raster::Raster::new((w*ppp) as usize,(height*ppp) as usize);
        for frame in 0..90{
            h.t=frame as f64/60.0;
            if frame==40{match case{
                0=>h.app.receive_wechat_reminder_from(100,0),
                1=>h.app.receive_notification("QQ".into(),"QQ".into(),"有新訊息 · 請到 QQ 檢視".into()),
                2=>{for pid in [100,200,300]{h.app.receive_wechat_reminder_from(pid,0);}h.app.alerts.clear();h.app.notice=None;h.app.notification_bell_until=0.0;},
                3=>h.app.messenger_unavailable("QQ 帳號已退出"),
                4=>h.app.messenger_unavailable("微信 帳號已退出"),
                5=>h.app.messenger_unavailable("暫時無法確認此 QQ 訊息的帳號"),
                6=>h.app.messenger_unavailable("QQ 視窗暫時無法開啟"),
                7=>{h.app.show_page(4);h.app.messenger_unavailable("QQ 帳號已退出");},
                8=>h.app.alert("大寫鎖定","已開啟",5,false),
                9=>h.app.alert("系統音量","音量 60%",5,false),
                10=>h.app.bluetooth_alert(&crate::bt::Device{id:"preview".into(),name:"耳機".into(),battery:Some(80),kind:crate::bt::DeviceKind::Headphones},true),
                11=>h.app.alert("電量偏低","剩餘 15% · 請連線電源",5,false),
                12=>{
                    let start=chrono::Local::now()+chrono::Duration::minutes(10);
                    h.app.meeting_alert(crate::mail::Meeting{all_day:false,title:"團隊會議".into(),start:start.timestamp(),end:None,at:start.format("%m-%d %H:%M").to_string(),uid:"preview".into()},"本機日程".into(),7);
                },
                13=>h.app.alert("喝水時間到了","休息一下，喝 250 ml 水吧",2,false),
                14=>h.app.alert("待辦到時間了","整理本週資料",4,false),
                _=>unreachable!(),
            }}
            let cursor=(case==7).then(||H::local(160.0,12.0));
            let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-08".into(),date:"11月8日 週日".into(),cursor,volume:(case==9).then_some((60,false)),media:h.media.clone(),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,height))),time:Some(h.t),focused:true,
                events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);
            });});
            raster.update_textures(&output.textures_delta);
            if frame==89{
                raster.buf.fill([0.93,0.95,0.97,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw((w*ppp)as u32,(height*ppp)as u32,rgba).unwrap();
                image::imageops::crop_imm(&image,100,0,1560,if case==7||case==12{440}else{260}).to_image().save(dest.join(format!("{case:02}.png"))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
#[cfg(feature="cn-messengers")]
fn qq_unknown_reminders_survive_viewing_other_accounts(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.read_notifications=true;
    h.app.receive_notification("QQ".into(),"QQ".into(),"未確認帳號的訊息".into());
    h.app.viewed_client(crate::app::Messenger::Qq,200);
    assert_eq!(h.app.pending_messengers,[crate::app::Unread::of(crate::app::Messenger::Qq)]);
}

#[test]
#[cfg(feature="cn-messengers")]
fn messenger_window_replacement_removes_stale_banner_targets_without_clearing_other_accounts(){
    use crate::app::{Cmd,Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for source in Messenger::ALL.into_iter().filter(|source|!matches!(source,Messenger::Discord|Messenger::Line)){
        let mut h=H::new();h.app.settings.read_notifications=true;h.app.settings.wechat_notifications=true;h.app.settings.wecom_notifications=true;h.app.settings.feishu_notifications=true;h.app.settings.dingtalk_notifications=true;
        let old=Unread{source,pid:100,window:11};let other=Unread{source,pid:200,window:22};let current=Unread{window:33,..old};
        let receive=|app:&mut App,unread:Unread|match source{
            Messenger::Wechat|Messenger::WeCom|Messenger::Feishu|Messenger::DingTalk=>app.receive_messenger_reminder(unread),
            Messenger::Qq=>app.receive_notification_from("QQ".into(),"QQ".into(),"有新訊息".into(),Some(unread)),
            Messenger::Discord|Messenger::Line=>unreachable!("本測試只覆蓋舊版 IM"),
        };
        receive(&mut h.app,old);h.frame(vec![]);receive(&mut h.app,other);receive(&mut h.app,current);
        assert_eq!(h.app.pending_messengers,[current,other]);
        assert!(h.app.alerts.iter().chain(h.app.notice.iter()).all(|alert|alert.unread!=Some(old)),"視窗被新訊息替換後，橫幅不能繼續引用已經失效的目標");
        assert!(h.app.alerts.iter().chain(h.app.notice.iter()).any(|alert|alert.unread==Some(other)),"其他帳號的橫幅必須保留");
        h.app.discard_unread(other);for _ in 0..45{h.frame(vec![]);}
        h.click_text("檢視");assert!(h.cmds.contains(&Cmd::ViewMessenger(current)));
    }
}

#[test]
#[cfg(feature="cn-messengers")]
fn qq_bound_notices_open_their_original_account_and_clear_independently(){
    use crate::app::{Cmd,Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.read_notifications=true;
    let a=Unread{source:Messenger::Qq,pid:100,window:11};let b=Unread{source:Messenger::Qq,pid:200,window:22};
    h.app.receive_notification_from("QQ".into(),"QQ".into(),"有新訊息".into(),Some(a));h.frame(vec![]);
    h.app.receive_notification_from("QQ".into(),"QQ".into(),"有新訊息".into(),Some(b));
    assert_eq!(h.app.pending_messengers,[a,b]);
    for _ in 0..45{h.frame(vec![]);}
    h.click_text("檢視");assert!(h.cmds.contains(&Cmd::ViewMessenger(a)),"橫幅應開啟它自己的帳號，不能取最近新增的另一個帳號");
    h.app.viewed_client(Messenger::Qq,200);h.frame(vec![]);
    assert_eq!(h.app.pending_messengers,[a]);
    h.cmds.clear();h.click_text("檢視");assert!(h.cmds.contains(&Cmd::ViewMessenger(a)));
    h.app.discard_unread(a);h.frame(vec![]);assert!(h.app.pending_messengers.is_empty());assert!(h.app.notice.is_none());
}

#[test]
#[cfg(feature="cn-messengers")]
fn qq_unknown_notice_can_close_without_dismissing_a_bound_account(){
    use crate::app::{Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.read_notifications=true;
    h.app.receive_notification("QQ".into(),"QQ".into(),"未確認帳號".into());h.frame(vec![]);
    let b=Unread{source:Messenger::Qq,pid:200,window:22};
    h.app.receive_notification_from("QQ".into(),"QQ".into(),"另一帳號的訊息".into(),Some(b));
    for _ in 0..45{h.frame(vec![]);}
    h.click_text("關閉");h.frame(vec![]);
    assert_eq!(h.app.pending_messengers,[b]);
}

#[test]
#[cfg(feature="cn-messengers")]
fn qq_expired_unidentified_notice_can_be_reopened_and_explicitly_cleared(){
    use crate::app::{Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.read_notifications=true;
    let unread=Unread::of(Messenger::Qq);
    h.app.receive_notification("QQ".into(),"QQ".into(),"測試訊息".into());
    for _ in 0..360{h.frame(vec![]);}assert!(h.app.notice.is_none());
    h.app.reveal_messenger_notice(unread);for _ in 0..45{h.frame(vec![]);}
    h.click_text("關閉");h.frame(vec![]);assert!(h.app.pending_messengers.is_empty());
}

#[test]
#[cfg(feature="cn-messengers")]
fn qq_unknown_reminders_are_not_numbered_as_another_known_account(){
    use crate::app::{Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.read_notifications=true;
    for (pid,window) in [(100,11),(0,0),(200,22)]{
        h.app.receive_notification_from("QQ".into(),"QQ".into(),"有新訊息".into(),(pid!=0).then_some(Unread{source:Messenger::Qq,pid,window}));
    }
    assert_eq!((h.app.unread_tag(0),h.app.unread_tag(1),h.app.unread_tag(2)),(Some(1),None,Some(2)));
    h.app.alerts.clear();h.app.notice=None;h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="?")));
}

#[test]
fn unavailable_messenger_status_stays_visible_when_the_island_is_collapsed(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for message in ["QQ 帳號已退出","微信 帳號已退出","暫時無法確認此 QQ 訊息的帳號"]{
        let mut h=H::new();h.hover(Pos2::new(600.0,180.0),1.0);
        h.app.messenger_unavailable(message);h.hover(Pos2::new(600.0,180.0),0.3);
        let bounds=h.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text) if text.galley.text()==message=>Some(text.visual_bounding_rect()),_=>None,
        }).expect("島收起時仍應完整顯示帳號狀態提示");
        assert!(h.app.progress()<0.02,"提示不應展開島體");
        assert!(h.region.contains_rect(bounds),"原生視窗區域應包含提示");
        assert!(!h.hit.intersects(bounds),"提示不應攔截滑鼠");
    }
}

#[test]
#[cfg(feature="cn-messengers")]
fn exited_messenger_unreads_clear_their_badges_and_notifications(){
    use crate::app::{Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.wechat_notifications=true;h.app.settings.read_notifications=true;
    h.app.receive_wechat_reminder_from(100,11);h.frame(vec![]);
    assert_eq!(h.app.notice.as_ref().and_then(|notice|notice.messenger()),Some(Messenger::Wechat));
    h.app.receive_wechat_reminder_from(200,22);h.app.receive_wechat_reminder_from(300,33);
    h.app.receive_notification("QQ".into(),"QQ".into(),"測試".into());
    for (pid,window) in [(200,22),(100,11)]{
        h.app.discard_unread(Unread{source:Messenger::Wechat,pid,window});h.frame(vec![]);
        assert!(h.app.pending_messengers.contains(&Unread{source:Messenger::Wechat,pid:300,window:33}));
        assert_eq!(h.app.notice.as_ref().and_then(|notice|notice.messenger()),Some(Messenger::Wechat));
    }
    h.app.discard_unread(Unread{source:Messenger::Wechat,pid:300,window:33});h.frame(vec![]);
    assert_eq!(h.app.pending_messengers,[Unread::of(Messenger::Qq)]);
    assert!(h.app.alerts.iter().all(|notice|notice.messenger()!=Some(Messenger::Wechat)));
    assert_ne!(h.app.notice.as_ref().and_then(|notice|notice.messenger()),Some(Messenger::Wechat));
    h.app.discard_unread(Unread::of(Messenger::Qq));h.frame(vec![]);
    assert!(h.app.pending_messengers.is_empty());
    assert!(h.app.alerts.iter().all(|notice|notice.messenger()!=Some(Messenger::Qq)));
    assert_ne!(h.app.notice.as_ref().and_then(|notice|notice.messenger()),Some(Messenger::Qq));
}

#[test]
fn stale_exit_results_preserve_new_windows_and_unidentified_accounts(){
    use crate::app::{Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.wechat_notifications=true;
    let old=Unread{source:Messenger::Wechat,pid:100,window:11};
    h.app.receive_wechat_reminder_from(old.pid,old.window);h.app.receive_wechat_reminder_from(100,12);
    h.app.receive_wechat_reminder();h.app.discard_unread(old);
    assert_eq!(h.app.pending_messengers,[Unread{window:12,..old},Unread::of(Messenger::Wechat)]);
    h.app.discard_unread(Unread{window:12,..old});h.frame(vec![]);
    assert_eq!(h.app.pending_messengers,[Unread::of(Messenger::Wechat)]);
    h.app.discard_unread(Unread::of(Messenger::Wechat));h.frame(vec![]);
    assert!(h.app.pending_messengers.is_empty());
}

#[test]
#[cfg(feature="cn-messengers")]
fn messenger_notifications_keep_separate_badges_until_the_client_is_viewed(){
    use crate::app::{Cmd,Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.wechat_notifications=true;h.app.settings.read_notifications=true;
    h.frame(vec![]);h.app.receive_wechat_reminder();
    for _ in 0..60{h.frame(vec![]);}
    h.click_text("檢視");assert!(h.cmds.contains(&Cmd::ViewMessenger(Unread::of(Messenger::Wechat))));
    h.click_text("關閉");assert!(h.app.notice.is_none());
    h.cursor=None;h.app.receive_notification("QQ".into(),"QQ".into(),"小林 · 測試".into());
    for _ in 0..360{h.frame(vec![]);}
    assert!(h.app.notice.is_none());
    let sources=|h:&H|h.app.pending_messengers.iter().map(|u|u.source).collect::<Vec<_>>();
    assert_eq!(sources(&h),[Messenger::Wechat,Messenger::Qq]);
    // 收起時兩條未讀疊成一個入口，點它展開面板，再在旁邊的膠囊裡逐個點開。
    h.frame(vec![]);
    let stack=h.ctx.read_response(egui::Id::new(("card",true,"messenger-stack"))).expect("疊放入口可點選").rect.center();
    h.click(stack);for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.open);
    for source in [Messenger::Wechat,Messenger::Qq]{
        h.frame(vec![]);
        let pos=h.ctx.read_response(egui::Id::new(("card",true,source.name()))).expect("膠囊圖示可點選").rect.center();
        h.cmds.clear(); h.click(pos);assert!(h.cmds.contains(&Cmd::ViewMessenger(Unread::of(source))),"{source:?} {pos:?} {:?}",h.cmds);
        h.app.viewed_messenger(source);h.frame(vec![]);
    }
    assert!(h.app.pending_messengers.is_empty());
    h.app.receive_wechat_reminder();h.app.receive_wechat_reminder();
    assert_eq!(sources(&h),[Messenger::Wechat]);
    h.app.settings.wechat_notifications=false;h.frame(vec![]);assert!(h.app.pending_messengers.is_empty());
}

#[test]
fn messenger_capsule_grows_during_opening(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.wechat_notifications=true;
    h.app.receive_wechat_reminder();
    h.app.alerts.clear();
    h.app.notice=None;
    for opening in [true,false,true,false] {
        h.cursor=opening.then_some(Pos2::new(WIN_W/2.0,12.0));
        let mut sampled=false;
        for _ in 0..90 {
            h.frame(vec![]);
            let appear=h.app.messenger_appear();
            if (0.02..0.95).contains(&appear) {
                let button=h.ctx.read_response(egui::Id::new(("card",true,"微信".to_string()))).unwrap();
                sampled|=(1.0..crate::app::UNREAD_W-1.0).contains(&button.rect.width());
            }
        }
        assert!(sampled && h.app.open==opening,"每次展開與收回都應經過中間尺寸");
    }
}

#[test]
#[cfg(feature="cn-messengers")]
fn messenger_accounts_get_number_tags_and_clear_one_by_one(){
    use crate::app::{Cmd,Messenger,Unread};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.wechat_notifications=true;h.app.settings.read_notifications=true;
    h.app.receive_wechat_reminder_from(100,11);h.app.receive_wechat_reminder_from(200,22);h.app.receive_wechat_reminder_from(100,12);
    h.app.receive_notification("QQ".into(),"QQ".into(),"測試".into());
    assert_eq!(h.app.pending_messengers,[Unread{source:Messenger::Wechat,pid:100,window:12},Unread{source:Messenger::Wechat,pid:200,window:22},Unread::of(Messenger::Qq)]);
    assert_eq!((h.app.unread_tag(0),h.app.unread_tag(1),h.app.unread_tag(2)),(Some(1),Some(2),None),"只有同一來源多個帳號才編號");
    h.app.alerts.clear();h.app.notice=None;
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.5);
    let rect=|h:&H,name:&str|h.ctx.read_response(egui::Id::new(("card",true,name.to_string()))).unwrap_or_else(||panic!("{name}")).rect;
    let panel=h.app.island_rect(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),crate::app::PW,h.app.expanded_height());
    let (a,b,q)=(rect(&h,"微信"),rect(&h,"微信2"),rect(&h,"QQ"));
    assert!(a.bottom()<=b.top()+0.5 && b.bottom()<=q.top()+0.5);
    let inset=h.app.unread_top_inset();assert!(inset>0.0,"頂部停靠時膠囊離開螢幕上沿");
    assert!((a.top()-panel.top()-inset-2.0).abs()<1.0 && (q.bottom()-panel.bottom()+2.0).abs()<1.0,"三個圖示時膠囊下沿與面板對齊 {a:?} {q:?} {panel:?}");
    h.cmds.clear();h.click(b.center());
    assert!(h.cmds.contains(&Cmd::ViewMessenger(Unread{source:Messenger::Wechat,pid:200,window:22})));
    h.app.viewed_client(Messenger::Wechat,200);
    assert_eq!(h.app.pending_messengers,[Unread{source:Messenger::Wechat,pid:100,window:12},Unread::of(Messenger::Qq)]);
    assert_eq!(h.app.unread_tag(0),None,"剩一個帳號就不再編號");
    h.app.viewed_client(Messenger::Qq,4321);
    assert!(h.app.pending_messengers.contains(&Unread::of(Messenger::Qq)),"檢視其他 QQ 不能清除無法歸屬的提醒");
    h.app.discard_unread(Unread::of(Messenger::Qq));
    assert_eq!(h.app.pending_messengers,[Unread{source:Messenger::Wechat,pid:100,window:12}]);
    // 四個以上：膠囊仍與面板等高，滾輪翻到後面的圖示。
    for pid in [300,400,500]{h.app.receive_wechat_reminder_from(pid,0);}
    h.hover(Pos2::new(WIN_W/2.0,12.0),0.2);
    let capsule=rect(&h,"微信");
    let last=|h:&H|rect(h,"微信4");
    let before=last(&h);
    assert!(before.height()<1.0 || before.top()>=panel.bottom()-3.0,"第四個先藏在下面");
    h.hover(Pos2::new(capsule.center().x,panel.top()+30.0),0.1);
    for _ in 0..10{h.wheel(-40.0);}
    h.frame(vec![]);
    assert!(last(&h).height()>20.0 && last(&h).bottom()<=panel.bottom(),"滾動後露出第四個 {:?}",last(&h));
}

#[test]
fn messenger_notifications_stay_right_and_move_outside_expanded_panel(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.wechat_notifications=true;
    h.frame(vec![]);h.app.receive_wechat_reminder();
    for _ in 0..360{h.frame(vec![]);}
    let badge=|h:&H|h.ctx.read_response(egui::Id::new(("card",true,"微信"))).unwrap().rect;
    assert!(badge(&h).left()>WIN_W/2.0);
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.5);
    assert!(h.app.open && badge(&h).left()>WIN_W/2.0);
    assert!((badge(&h).left()-(WIN_W/2.0+crate::app::PW/2.0+crate::app::UNREAD_GAP)).abs()<1.0,"展開後與面板留一小段間距 {:?}",badge(&h));
    let center=badge(&h).center();
    assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(r)
        if r.rect.contains(center) && (r.rect.width()-crate::app::UNREAD_W).abs()<0.1 && (r.rect.height()-crate::app::UNREAD_W).abs()<0.1)),"一個圖示時是正圓");
    h.click(badge(&h).center());
    assert!(h.cmds.contains(&crate::app::Cmd::ViewMessenger(crate::app::Unread::of(crate::app::Messenger::Wechat))));
}

#[test]
fn messenger_notifications_align_with_system_banner(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.wechat_notifications=true;
    h.app.settings.read_notifications=true;
    h.frame(vec![]);h.app.receive_wechat_reminder();
    for _ in 0..360{h.frame(vec![]);}
    h.app.receive_notification("ChatGPT".into(),"ChatGPT".into(),"測試".into());
    for _ in 0..60{h.frame(vec![]);}
    let rect=|name|h.ctx.read_response(egui::Id::new(("card",true,name))).unwrap().rect;
    let badge=rect("微信");let close=rect("external-dismiss");
    assert!(badge.left()>close.right());
    assert!((badge.center().y-close.center().y).abs()<1.1);
}

#[test]
#[cfg(feature="cn-messengers")]
fn messenger_notifications_side_docks_keep_one_column(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [crate::model::Dock::Left,crate::model::Dock::Right]{
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.wechat_notifications=true;h.app.settings.read_notifications=true;
        h.app.receive_wechat_reminder();h.app.receive_notification("QQ".into(),"QQ".into(),"測試".into());
        for _ in 0..720{h.frame(vec![]);}
        let win=Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H));
        let stack=h.ctx.read_response(egui::Id::new(("card",true,"messenger-stack"))).unwrap().rect;
        let bar=h.app.island_rect(win,h.app.w.pos,h.app.h.pos);
        assert!((stack.center().x-bar.center().x).abs()<1.0 && stack.top()>=bar.bottom()-0.5,"收起時疊放在豎膠囊下方");
        h.click(stack.center());for _ in 0..90{h.frame(vec![]);}
        let rect=|name|h.ctx.read_response(egui::Id::new(("card",true,name))).unwrap().rect;
        let a=rect("微信");let b=rect("QQ");
        assert_eq!(a.center().x,b.center().x);assert!(a.bottom()<=b.top()+0.01);
        let panel=h.app.island_rect(win,crate::app::PW,h.app.expanded_height());
        if dock==crate::model::Dock::Left {assert!(a.left()>=panel.right()+crate::app::UNREAD_GAP-0.5,"左停靠：膠囊在面板右側");}
        else {assert!(a.right()<=panel.left()-crate::app::UNREAD_GAP+0.5,"右停靠：膠囊在面板左側");}
        assert!((a.top()-panel.top()-2.0).abs()<1.0 && h.app.unread_top_inset()==0.0,"側邊停靠時頂端與面板對齊");
    }
}

#[test]
fn side_dock_file_drag_opens_the_shelf_and_drop_saves_it(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    let path=crate::model::config_dir().join("側邊拖入.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,"x").unwrap();
    for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
        let mut h=H::new();h.app.settings.dock=dock;
        let edge=if dock==crate::model::Dock::Left{10.0}else{w-10.0};
        let at=Pos2::new(edge,hgt/2.0);
        let mut frame=|h:&mut H,hovered:bool,dropped:bool|{
            h.t+=1.0/60.0;
            let env=Env{t:h.t,clock:"09:41:00".into(),cursor:Some(at),..Default::default()};
            let mut raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,..Default::default()};
            if hovered {raw.hovered_files.push(egui::HoveredFile{path:Some(path.clone()),..Default::default()});}
            if dropped {raw.dropped_files.push(egui::DroppedFile{path:Some(path.clone()),..Default::default()});}
            let app=&mut h.app;let mut out=None;
            let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{out=Some(app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt)),&env));});});
            out.unwrap()
        };
        let out=frame(&mut h,false,false);
        let zone=out.file_drop_zone.expect("置物架開啟時側邊也有拖放感應區");
        assert!(zone.contains(at),"{dock:?} 感應區應覆蓋貼邊的膠囊：{zone:?}");
        frame(&mut h,true,false);
        assert!(h.app.drag_hot && h.app.page==3,"{dock:?} 檔案拖到膠囊上應切到置物架");
        for _ in 0..40 {frame(&mut h,true,false);}
        assert!(h.app.open);
        frame(&mut h,false,true);
        assert!(h.app.shelf.items.contains(&path),"{dock:?} 鬆手後應放進置物架");
        h.app.shelf.items.clear();h.app.shelf.save();
    }
}

fn shoot_unread(h:&mut H,raster:&mut crate::preview_raster::Raster,dest:&std::path::Path,name:&str,frames:usize,cursor:Option<Pos2>){
    let ppp=2.0;
    let side=h.app.settings.dock.is_side();
    let (w,hgt)=if side {(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)} else {(crate::app::CANVAS_W,WIN_H)};
    let (ww,wh)=if side {(w,hgt)} else {(WIN_W,WIN_H)};
    for f in 0..frames {
        h.t+=1.0/60.0;
        let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-05".into(),date:"11月5日 週四".into(),cursor,bt_ready:true,battery:Some(76),..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
            events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let app=&mut h.app;
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(ww,wh));app.frame(ui,win,&env);});});
        raster.update_textures(&output.textures_delta);
        if f+1==frames {
            raster.buf.fill([0.20,0.24,0.32,1.0]);
            raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let image=image::RgbaImage::from_raw((w*ppp) as u32,(hgt*ppp) as u32,rgba).unwrap();
            let image=if side {image::imageops::crop_imm(&image,0,240,(w*ppp) as u32,720).to_image()} else {image::imageops::crop_imm(&image,0,0,(WIN_W*ppp) as u32,(WIN_H*ppp) as u32).to_image()};
            image.save(dest.join(format!("{name}.png"))).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
#[ignore="按需匯出微信 / QQ 未讀膠囊的實際畫面"]
fn unread_gallery(){
    use crate::app::Messenger;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_FEATURE_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let fill=|h:&mut H,n:usize|{
        h.app.settings.wechat_notifications=true;h.app.settings.read_notifications=true;h.app.settings.pet=false;
        let all=[(Messenger::Wechat,101),(Messenger::Qq,0),(Messenger::Wechat,202),(Messenger::Wechat,303),(Messenger::Qq,0)];
        for (source,pid) in all.into_iter().take(n){
            if source==Messenger::Wechat {h.app.receive_wechat_reminder_from(pid,0);} else {h.app.receive_notification("QQ".into(),"QQ".into(),"測試".into());}
        }
        h.app.alerts.clear();h.app.notice=None;h.app.notification_bell_until=0.0;
    };
    for dock in [crate::model::Dock::Top,crate::model::Dock::Left,crate::model::Dock::Right]{
        let key=dock.key();
        for n in [1,2,3,4]{
            let mut h=H::new();h.app.settings.dock=dock;fill(&mut h,n);
            let side=dock.is_side();
            let (w,hgt)=if side {(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)} else {(crate::app::CANVAS_W,WIN_H)};
            let mut r=if side {feature_raster_sized(w,hgt)} else {feature_raster()};
            let away=Some(Pos2::new(if side {w/2.0} else {WIN_W/2.0},hgt-6.0));
            let near=if dock==crate::model::Dock::Left {Pos2::new(8.0,hgt/2.0)} else if dock==crate::model::Dock::Right {Pos2::new(w-8.0,hgt/2.0)} else {Pos2::new(WIN_W/2.0,12.0)};
            if n!=3 {shoot_unread(&mut h,&mut r,&dest,&format!("unread-{key}-{n}-collapsed"),60,away);}
            shoot_unread(&mut h,&mut r,&dest,&format!("unread-{key}-{n}-open"),80,Some(near));
        }
        // 微信多開：兩個微信賬號；兩個微信賬號 + QQ。
        for (tag,qq) in [("multi2",false),("multi3",true)]{
            let mut h=H::new();h.app.settings.dock=dock;fill(&mut h,0);
            h.app.receive_wechat_reminder_from(101,0);h.app.receive_wechat_reminder_from(202,0);
            if qq {h.app.receive_notification("QQ".into(),"QQ".into(),"測試".into());}
            h.app.alerts.clear();h.app.notice=None;h.app.notification_bell_until=0.0;
            let side=dock.is_side();
            let (w,hgt)=if side {(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)} else {(crate::app::CANVAS_W,WIN_H)};
            let mut r=if side {feature_raster_sized(w,hgt)} else {feature_raster()};
            let away=Some(Pos2::new(if side {w/2.0} else {WIN_W/2.0},hgt-6.0));
            let near=if dock==crate::model::Dock::Left {Pos2::new(8.0,hgt/2.0)} else if dock==crate::model::Dock::Right {Pos2::new(w-8.0,hgt/2.0)} else {Pos2::new(WIN_W/2.0,12.0)};
            shoot_unread(&mut h,&mut r,&dest,&format!("unread-{key}-{tag}-collapsed"),60,away);
            shoot_unread(&mut h,&mut r,&dest,&format!("unread-{key}-{tag}-open"),80,Some(near));
        }
    }
}

#[test]
fn shelf_menu_last_action_fits_inside_window_and_works(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.shelf.items=vec![r"folder\menu-test.zip".into()];
    h.app.show_page(3);
    let point=H::local(209.5,101.0);
    h.hover(point,1.2);
    for pressed in [true,false]{
        h.frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Secondary,pressed,modifiers:Default::default()}]);
    }
    h.hover(point,1.5);
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==r"folder\menu-test.zip")));
    let label=h.shapes.iter().find_map(|s|match &s.shape{
        egui::Shape::Text(t) if t.galley.text()=="從置物架移除"=>Some(Rect::from_min_size(t.pos,t.galley.size())),
        _=>None,
    }).expect("右鍵選單應顯示移除操作");
    assert!(h.region.contains_rect(h.app.shelf_menu_rect.unwrap() + h.ctx.style().visuals.popup_shadow.margin()));
    assert!(h.app.shelf_menu_rect.unwrap().contains_rect(label));
    h.hover(label.center(),1.0);
    assert!(h.app.open);
    h.click(label.center());
    assert!(h.app.shelf.items.is_empty());
}

#[test]
fn calendar_add_button_does_not_overlap_the_battery_ring(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for seconds in [false,true] {
    for (index,(day_number,date,period,twelve,battery,scale)) in [
        (6,"11月6日 週五",1,false,Some(76),2.0),
        (28,"11月28日 週六",1,true,Some(76),1.25),
        (28,"11月28日 週六",2,false,Some(76),1.5),
        (6,"11月6日 週五",0,false,Some(76),1.0),
        (28,"11月28日 週六",0,true,Some(76),2.0),
        (6,"11月6日 週五",1,true,None,1.25),
        (28,"11月28日 週六",2,true,None,1.5),
        (28,"11月28日 週六",0,false,None,1.0),
    ].into_iter().enumerate(){
    let mut h=H::new();
    h.app.settings.skin=crate::model::Skin::Cream;h.app.settings.island_shape=crate::model::IslandShape::Pill;
    h.app.settings.clock_period=period;h.app.settings.clock_12h=twelve;h.app.settings.pet=false;
    h.app.settings.clock_seconds=seconds;
    let day=chrono::NaiveDate::from_ymd_opt(2026,11,day_number).unwrap();
    h.app.calendar.view.day=day;h.app.calendar.view.month=crate::calendar::month_start(day);h.app.calendar.view.loaded=Some(h.app.calendar.view.month);
    h.app.show_page(7);
    let mut raster=feature_raster();let mut shapes=Vec::new();
    for _ in 0..90 {
        h.t+=1.0/60.0;
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),focused:true,
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
        let env=Env{t:h.t,clock:"17:43:00".into(),date:date.into(),today:day.to_string(),cursor:Some(Pos2::new(WIN_W/2.0,12.0)),battery,charging:true,..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);});});
        raster.update_textures(&output.textures_delta);shapes=output.shapes;
        raster.free_textures(&output.textures_delta);
    }
    if let Some(path)=std::env::var_os("DAOZAI_CALENDAR_HEADER_PREVIEW").filter(|_|index==0&&!seconds) {
        raster.buf.fill([0.15,0.15,0.15,1.0]);raster.draw(&h.ctx.tessellate(shapes.clone(),2.0),2.0);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
        let image=image::RgbaImage::from_raw((crate::app::CANVAS_W*2.0) as u32,(WIN_H*2.0) as u32,rgba).unwrap();
        image::imageops::crop_imm(&image,200,0,880,470).to_image().save(path).unwrap();
    }
    let text=shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text) if text.galley.text()=="＋ 新增"=>Some(text.visual_bounding_rect()),_=>None}).unwrap();
    let button=shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Rect(rect) if rect.rect.contains(text.center()) && (rect.rect.width()-46.0).abs()<0.1 && (rect.rect.height()-18.0).abs()<0.1=>Some(rect.rect),_=>None}).unwrap();
    if battery.is_some(){
        let ring=shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Circle(circle) if (circle.radius-6.56).abs()<0.1 && (circle.center.y-button.center().y-0.6).abs()<0.1=>Some(Rect::from_center_size(circle.center,Vec2::splat(circle.radius*2.0+circle.stroke.width))),_=>None}).unwrap();
        assert!(!button.expand(3.0).intersects(ring),"日程新增按鈕與電量圓環應留有間距：按鈕 {button:?}，圓環 {ring:?}");
    }
    let clock=h.app.settings.clock_text("17:43:00");
    for text in shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(text) if text.galley.text()==date || clock.split_whitespace().any(|part|part==text.galley.text())=>Some(text),_=>None}){
        let bounds=text.visual_bounding_rect();
        assert!(!button.expand(3.0).intersects(bounds),"新增按鈕不能覆蓋日期或時間：{}",text.galley.text());
        assert!(bounds.right()<h.app.content_origin(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))).x+355.0,"日期與時間應留出右側播放和設定按鈕的位置");
    }
    }
    }
}

#[test]
fn wheel_short_lists_turn_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for count in [0,1,5]{
        h.app.shelf.items=(0..count).map(|i|std::path::PathBuf::from(format!("scroll-test-{i}.txt"))).collect();
        for (delta,page) in [(-40.0,4),(40.0,2)]{
            h.app.show_page(3);h.hover(H::local(300.0,60.0),0.5);h.wheel(delta);
            assert_eq!(h.app.page,page,"置物架未溢位應翻頁，條數 {count}");
        }
    }
    for count in [0,1,3]{
        h.app.todos.items=(0..count).map(|i|crate::model::TodoItem{text:format!("任務 {i}"),..Default::default()}).collect();
        for (delta,page) in [(-40.0,5),(40.0,3)]{
            h.app.show_page(4);h.hover(H::local(300.0,60.0),0.5);h.wheel(delta);
            assert_eq!(h.app.page,page,"待辦未溢位應翻頁，條數 {count}");
        }
    }
    for count in [0,1,crate::pages::CLIP_ROWS]{
        h.app.clipboard.lock().items.clear();
        for i in 0..count{h.app.clipboard.lock().push(&i.to_string(),"09:00");}
        for (delta,page) in [(-40.0,4),(40.0,2)]{
            h.app.show_page(3);h.app.shelf_tab=1;h.hover(H::local(300.0,60.0),0.5);h.wheel(delta);
            assert_eq!(h.app.page,page,"剪貼簿未溢位應翻頁，條數 {count}");
        }
    }
    let day=chrono::NaiveDate::from_ymd_opt(2026,10,28).unwrap();
    let start=day.and_hms_opt(10,0,0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap().timestamp();
    h.app.calendar.view.day=day;h.app.calendar.view.month=crate::calendar::month_start(day);h.app.calendar.view.loaded=Some(h.app.calendar.view.month);
    for (count,ended,all_day) in [(0,false,false),(1,false,false),(5,false,false),(8,true,false),(4,false,true)]{
        h.app.calendar.view.events=(0..count).map(|i|crate::mail::Meeting{all_day,title:format!("日程 {i}"),start:if ended{start-7200}else{start+i*60},end:Some(if ended{start-3600}else{start+3600}),at:String::new(),uid:format!("wheel-{i}")}).collect();
        for (delta,page) in [(-40.0,8),(40.0,6)]{
            h.app.show_page(7);h.hover(H::local(300.0,160.0),0.5);h.wheel(delta);
            assert_eq!(h.app.page,page,"日曆可見列表未溢位應翻頁，條數 {count}");
        }
    }
    h.app.calendar.view.events=(0..6).map(|i|crate::mail::Meeting{all_day:false,title:format!("日程 {i}"),start:start+i*60,end:Some(start+3600),at:String::new(),uid:format!("wheel-long-{i}")}).collect();
    h.app.show_page(7);
    for (delta,first) in [(40.0,0),(-40.0,1),(-40.0,1),(40.0,0),(40.0,0)]{
        h.hover(H::local(300.0,160.0),0.5);h.wheel(delta);
        assert_eq!(h.app.page,7,"日曆溢位後邊界也應留在本頁");assert_eq!(h.app.calendar.view.first,first);
    }
}

#[test]
fn mail_reading_wheel_over_headers_never_turns_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![crate::mail::Item{html:String::new().into(),received_unix:None,account:"work@qq.com".into(),from:"產品團隊".into(),subject:"閱讀中的郵件".into(),time:"09:41".into(),code:None,summary:"正文內容。".repeat(200).into(),links:vec![],meeting:None,uid:42}];
    h.app.show_page(5);h.app.mail.open_uid=Some(("work@qq.com".into(),42));
    for (x,y) in [(210.0,18.0),(200.0,48.0),(200.0,73.0),(200.0,92.0),(270.0,214.0)]{
        for delta in [-120.0,120.0]{
            h.hover(H::local(x,y),0.5);h.wheel(delta);
            assert_eq!(h.app.page(),5,"閱讀郵件時在 ({x},{y}) 滾輪不能翻頁");
            assert_eq!(h.app.mail.open_uid,Some(("work@qq.com".into(),42)));
        }
    }
    h.click(H::local(350.0,48.0));assert!(h.app.mail.open_uid.is_none());
    h.hover(H::local(210.0,18.0),0.5);h.wheel(-120.0);
    assert_eq!(h.app.page(),6,"返回列表後頂欄滾輪應恢復翻頁");
}

#[test]
fn reader_actions_remain_bound_to_the_opened_mailbox() {
    use crate::mail_manage::{Action,Folder,Request,Completed,Snapshot};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=managed_mail_fixture();
    let old=h.app.mail.recent[0].clone();
    h.app.mail_open_reader(&old);assert!(h.app.mail_reader_ready(&old,Folder::Inbox));
    let mut fresh=old.clone();fresh.subject="重建資料夾後的郵件".into();
    h.app.mail_event(crate::mail::Event::Status{account:old.account.clone(),status:crate::mail::Status::Ready});
    assert!(!h.app.mail_reader_ready(&old,Folder::Inbox));
    h.app.mail_managed(Completed{request:Request{account:h.app.mail.accounts[0].clone(),folder:Folder::Inbox,validity:None,known:vec![],extra:vec![],action:Action::Load},result:Ok(Snapshot{validity:8,entries:vec![(old.uid,false)],items:vec![fresh]})});
    assert!(h.app.mail.reading.is_none());assert!(!h.app.mail_reader_ready(&old,Folder::Inbox));
}

#[test]
fn local_reader_moves_and_restores_after_switching_the_main_list() {
    use crate::mail_manage::{Action,Folder};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();h.app.settings.mail_sync_operations=false;
    let item=h.app.mail.recent[0].clone();
    for folder in [Folder::Inbox,Folder::Trash]{
        h.app.mail_open_reader(&item);h.app.mail_toggle_trash();
        assert!(h.app.mail_reader_ready(&item,folder));
        h.app.mail_manage_request(Some(&item.account),folder,Action::Move{uid:item.uid});
        assert!(h.app.mail_rows().iter().any(|m|m.uid==item.uid));
    }
    h.app.mail_manage_request(Some(&item.account),Folder::Trash,Action::Move{uid:item.uid});
    assert!(h.app.mail.manage.message.contains("重新整理"));
}

#[test]
fn new_mail_preserves_an_explicit_reveal_in_the_trash(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();
    let mut item=h.app.mail.recent[0].clone();item.uid=77;item.code=Some("123456".into());
    h.app.mail.manage.trash.push(item.clone());h.app.reveal_mail_code(&item);
    let incoming=h.app.mail.recent[1].clone();h.app.mail_event(crate::mail::Event::Mail{item:incoming,new:true});
    assert!(!h.app.mail_code_hidden(&item));
    h.app.t+=16.0;assert!(h.app.mail_code_hidden(&item));
}

fn managed_mail_fixture()->H{
    use crate::mail_manage::{Folder,Snapshot};
    let mut h=H::new();h.app.settings.mail_sync_operations=true;let account=crate::mail::Account{custom:None,email:"manage@example.com".into(),provider:crate::mail::Provider::Qq,enabled:true};
    h.app.mail.accounts=vec![account.clone()];
    h.app.mail.recent=(1..=3).map(|uid|crate::mail::Item{html:String::new().into(),received_unix:None,account:account.email.clone(),from:format!("發件人 {uid}"),subject:format!("測試郵件 {uid}"),time:format!("09:4{uid}"),code:None,summary:"完整的測試郵件正文。".repeat(10).into(),links:vec![],meeting:None,uid}).collect();
    h.app.mail.manage.identities.insert(account.email.clone(),account.clone());
    h.app.mail.manage.data.insert((account.email,Folder::Inbox),Snapshot{validity:7,entries:vec![(1,false),(2,true),(3,true)],items:vec![]});
    h.app.show_page(5);h.hover(H::local(220.0,80.0),1.0);h
}
#[test]
fn managed_mail_unread_filter_open_and_mark_read(){
    use crate::mail_manage::{Action,Folder,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();
    h.click(H::local(118.0,42.0));assert!(h.app.mail.manage.unread_only);
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="測試郵件 2")));
    h.cmds.clear();h.click(H::local(180.0,64.0));h.frame(vec![]);
    assert_eq!(h.app.mail.open_uid,Some(("manage@example.com".into(),1)));
    let request=h.cmds.iter().find_map(|c|match c{crate::app::Cmd::MailManage(r)if r.action==(Action::Read{uid:1,seen:true})=>Some(r.clone()),_=>None}).expect("開啟未讀郵件應提交伺服器已讀操作");
    let mut snapshot=h.app.mail.manage.data[&("manage@example.com".into(),Folder::Inbox)].clone();snapshot.entries[0].1=true;
    h.app.mail_managed(Completed{request,result:Ok(snapshot)});h.hover(H::local(270.0,150.0),0.5);
    h.click(H::local(350.0,48.0));assert!(h.app.mail.open_uid.is_none());h.hover(H::local(200.0,80.0),0.4);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="沒有未讀郵件")));
}
#[test]
fn managed_mail_trash_icon_and_restore_keep_inbox_uid_separate(){
    use crate::mail_manage::{Action,Folder,Snapshot,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().starts_with("回收站"))),"回收站入口只顯示圖示");
    let item=h.app.mail.recent[0].clone();h.app.mail.manage.trash=vec![item];
    h.app.mail.manage.data.insert(("manage@example.com".into(),Folder::Trash),Snapshot{validity:9,entries:vec![(1,true)],items:vec![]});
    h.click(H::local(370.0,130.0));assert!(h.app.mail.manage.trash_open);
    h.hover(H::local(350.0,64.0),0.2);h.cmds.clear();h.click(H::local(350.0,64.0));
    let request=h.cmds.iter().find_map(|c|match c{crate::app::Cmd::MailManage(r)if r.action==(Action::Move{uid:1})=>Some(r.clone()),_=>None}).expect("回收站恢復按鈕應提交移動操作");
    assert_eq!(request.folder,Folder::Trash);assert_eq!(request.validity,Some(9));
    h.app.mail_managed(Completed{request,result:Ok(Snapshot{validity:9,entries:vec![],items:vec![]})});
    assert!(h.app.mail.manage.trash.is_empty());assert_eq!(h.app.mail.recent.len(),3,"同 UID 的收件箱郵件不能被回收站恢復操作移除");
    assert!(h.app.mail.manage.data.contains_key(&("manage@example.com".into(),Folder::Inbox)),"恢復時應保留收件箱正文快取的有效性，切回後只補讀新增郵件");
    h.frame(vec![]);h.click(H::local(350.0,43.0));assert!(!h.app.mail.manage.trash_open);
}
#[test]
fn managed_mail_actions_wait_for_the_server_and_batch_read_is_explicit(){
    use crate::mail_manage::{Action,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();h.cmds.clear();
    h.hover(H::local(354.0,64.0),0.2);h.click(H::local(354.0,64.0));
    let request=h.cmds.iter().find_map(|c|match c{crate::app::Cmd::MailManage(r)if r.action==(Action::Move{uid:1})=>Some(r.clone()),_=>None}).expect("刪除應提交移到回收站");
    assert_eq!(h.app.mail.recent.len(),3,"伺服器確認前不能丟掉郵件");
    h.app.mail_managed(Completed{request,result:Err("伺服器拒絕操作".into())});assert_eq!(h.app.mail.recent.len(),3);
    for snapshot in h.app.mail.manage.data.values_mut(){for (_,seen) in &mut snapshot.entries{*seen=true;}}
    h.frame(vec![]);h.cmds.clear();h.click(H::local(350.0,43.0));
    assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MailManage(r)if r.action==Action::ReadAll)),"即使最近郵件已讀，也應能處理信箱中更早的未讀郵件");
}

#[test]
fn mail_local_mode_persists_without_writing_to_the_server(){
    use crate::mail_manage::{Action,Folder,Request,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();h.app.settings.mail_sync_operations=false;h.cmds.clear();
    let original=h.app.mail.recent.clone();let account=h.app.mail.accounts[0].clone();
    let snapshot=h.app.mail.manage.data[&("manage@example.com".into(),Folder::Inbox)].clone();
    h.app.mail_manage_request(Some("manage@example.com"),Folder::Inbox,Action::Read{uid:2,seen:false});
    assert_eq!(h.app.mail_seen(&original[1],Folder::Inbox),Some(false));
    assert!(h.app.mail.manage.data[&("manage@example.com".into(),Folder::Inbox)].entries[1].1,"伺服器狀態不能被本機已讀操作改動");
    h.app.mail_manage_request(Some("manage@example.com"),Folder::Inbox,Action::Move{uid:1});
    assert_eq!(h.app.mail.recent.len(),2);assert_eq!(h.app.mail.manage.trash.len(),1);
    h.frame(vec![]);assert!(!h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MailManage(r)if r.action!=Action::Load)));
    let path=crate::model::config_dir().join("mail-local-state.json");let saved=std::fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("manage@example.com")&&!saved.contains("測試郵件"),"本機狀態僅儲存標識和操作標記");
    h.app.mail.manage=Default::default();h.app.mail.recent=original.clone();h.app.mail_reload_local();
    h.app.mail_managed(Completed{request:Request{account,folder:Folder::Inbox,validity:Some(7),known:vec![],extra:vec![],action:Action::Load},result:Ok(snapshot)});
    assert_eq!(h.app.mail.recent.len(),2,"重新讀取伺服器後，本機移除的郵件不能重新出現");assert_eq!(h.app.mail.manage.trash.len(),1);
    assert_eq!(h.app.mail_seen(&original[1],Folder::Inbox),Some(false),"本機已讀狀態必須跨重啟保留");
    h.app.mail.manage.trash_open=true;h.app.mail_manage_request(Some("manage@example.com"),Folder::Trash,Action::Move{uid:1});
    assert!(h.app.mail.manage.trash.is_empty());assert_eq!(h.app.mail.recent.len(),3);
    h.cmds.clear();h.app.settings.mail_sync_operations=true;h.app.mail_manage_mode_changed();h.frame(vec![]);
    assert!(!h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MailManage(r)if r.action!=Action::Load)),"開啟同步不能補交本機操作");
    assert_eq!(h.app.mail_seen(&original[1],Folder::Inbox),Some(true),"開啟同步後應顯示伺服器真實狀態");
}
#[test]
fn mail_sync_switch_defaults_off_and_persists(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    assert!(!crate::model::Settings::default().mail_sync_operations);
    h.app.settings.mail_sync_operations=true;h.app.settings.save();assert!(crate::model::Settings::load().mail_sync_operations);
    h.app.settings.mail_sync_operations=false;h.app.settings.save();assert!(!crate::model::Settings::load().mail_sync_operations);
}

#[test]
fn claude_local_only_settings_show_notice_without_manual_reset_controls(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn frame(h:&mut H,raster:&mut crate::preview_raster::Raster){
        h.t+=1.0/60.0;
        let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(640.0,760.0))),time:Some(h.t),..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));});
        raster.update_textures(&output.textures_delta);h.shapes=output.shapes;
    }
    fn capture(h:&H,raster:&mut crate::preview_raster::Raster,name:&str){
        if let Some(dir)=std::env::var_os("DAOZAI_CLAUDE_SETTINGS_PREVIEW"){
            let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();
            raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),1.0),1.0);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dir.join(format!("{name}.png")),&rgba,640,760,image::ColorType::Rgba8).unwrap();
        }
    }
    for dark in [true,false]{
        let mut h=H::new();h.ctx.set_visuals(if dark{egui::Visuals::dark()}else{egui::Visuals::light()});
        let account=crate::quota::Account{id:"local-test".into(),provider:crate::quota::Provider::Claude,alias:"Claude 本地帳號".into(),source:"fixture/config.json".into(),slot:Some("fixture".into()),enabled:true,pinned:true};
        h.app.quota.config.accounts.push(account.clone());
        let snapshot=crate::quota::Snapshot{at:chrono::Utc::now().timestamp(),windows:vec![crate::quota::Window{label:"5 小時".into(),seconds:Some(18000),remaining:37.5,reset:None,amount:None},crate::quota::Window{label:"本週".into(),seconds:Some(604800),remaining:62.0,reset:None,amount:None}],..Default::default()};
        h.app.quota.rows.insert(account.id.clone(),Ok(snapshot.clone()));let mut raster=crate::preview_raster::Raster::new(640,760);
        for connection in [false,true]{
            h.app.quota.provider=crate::quota::Provider::Claude;h.app.quota.connect_open=connection;
            for _ in 0..4{frame(&mut h,&mut raster);}
            for phrase in ["由於 Claude 對網路使用環境要求較高","只讀取本機用量記錄","重置時間未知","本地讀取不構成帳號安全保證"]{
                let text=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text().contains(phrase)=>Some((s.clip_rect,t.visual_bounding_rect())),_=>None}).unwrap_or_else(||panic!("缺少說明：{phrase}"));
                assert!(text.0.contains_rect(text.1),"說明不能被裁切：{phrase}");
            }
            for phrase in ["手動設定重置時間","儲存時間","清除手動時間","預設預選","手動填寫"]{
                assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"已撤回的手動時間入口及提示不應顯示：{phrase}");
            }
            assert_eq!(h.app.quota.rows[&account.id].as_ref().unwrap().windows,snapshot.windows,"撤回輸入功能不能影響用量資料");
            let name=format!("{}-{}",if connection{"connection"}else{"account"},if dark{"dark"}else{"light"});capture(&h,&mut raster,&name);
        }
        assert!(h.app.quota.gemini_connecting.is_none()&&h.app.quota.workbuddy_connecting.is_none()&&h.app.quota.login.is_none());
    }

}

#[test]
fn weather_page_shows_the_local_snapshot_and_forecast(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.pet=false;
    h.app.weather.snapshot=Some(crate::weather::Snapshot{condition:None,attributions:vec![],place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),
        sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,
        days:vec![
            crate::weather::Day{label:"今天".into(),date:"2026-11-09".into(),high:15.2,low:6.8,code:51,rain:60},
            crate::weather::Day{label:"明天".into(),date:"2026-11-10".into(),high:14.5,low:4.7,code:2,rain:0},
            crate::weather::Day{label:"週三".into(),date:"2026-11-11".into(),high:17.4,low:5.0,code:2,rain:0},
            crate::weather::Day{label:"週四".into(),date:"2026-11-12".into(),high:19.2,low:8.2,code:61,rain:25},
        ],at:chrono::Utc::now().timestamp()});
    h.app.show_page(8);h.hover(H::local(300.0,60.0),1.0);
    assert_eq!(h.app.page,8,"天氣應是第九個頁面");
    for phrase in ["9°","毛毛雨","杭州","今天","明天","週三","週四","60%","體感","6°","溼度","58%","日出 06:52","日落 17:21"]{
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"天氣頁缺少：{phrase}");
    }
    for s in &h.shapes{if let egui::Shape::Text(t)=&s.shape{assert!(s.clip_rect.contains_rect(t.visual_bounding_rect()),"天氣頁文字被裁切：{}",t.galley.text());}}
    if let Some(dest)=std::env::var_os("DAOZAI_WEATHER_PREVIEW_DIR"){
        let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
        let mut raster=feature_raster();shoot_feature(&mut h,&mut raster,&dest,"weather",30,Some(H::local(300.0,60.0)),"2026-11-09");
    }
}

#[test]
fn weather_page_explains_when_data_is_missing(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.weather.error=Some("網路不可用".into());
    h.app.show_page(8);h.hover(H::local(300.0,60.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="天氣暫時取不到")));
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("網路不可用"))));
}

#[test]
fn weather_chip_sits_after_the_clock_on_the_resident_bar(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.pet=false;
    h.app.weather.snapshot=Some(crate::weather::Snapshot{condition:None,attributions:vec![],place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    for _ in 0..60{h.frame(vec![]);}
    let layout=h.app.resident.current();
    let clock=layout.x[crate::bar_layout::CLOCK].expect("時鐘應常駐");
    let chip=layout.x[crate::bar_layout::WEATHER].expect("有快照時天氣徽標應緊跟時鐘佔位");
    assert!(chip>clock+22.0+6.9,"天氣應在時鐘右側：時鐘 {clock}，天氣 {chip}");
    let clock_text=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="09:41"=>Some(t.visual_bounding_rect()),_=>None}).expect("常駐欄應有時鐘");
    let temp=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="9°"=>Some(t.visual_bounding_rect()),_=>None}).expect("常駐欄應在時間後顯示當前溫度");
    assert!(temp.left()>clock_text.right()+4.0,"溫度應與時鐘留出間距：時鐘右邊 {:?}，溫度左邊 {:?}",clock_text.right(),temp.left());
    assert!(temp.center().y<30.0,"溫度應與圖示同一行");
    h.click(temp.center());
    assert_eq!(h.app.page(),8,"點溫度也能開啟天氣頁");
}

#[test]
fn weather_chip_hides_without_data_or_when_the_page_is_hidden(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.pet=false;
    for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.resident.current().x[crate::bar_layout::WEATHER].is_none(),"沒有快照時不佔位");
    h.app.weather.snapshot=Some(crate::weather::Snapshot{condition:None,attributions:vec![],place:"杭州".into(),position:None,auto:false,temp:9.0,feels:None,humidity:None,wind:None,code:0,day:true,high:None,low:None,sunrise:None,sunset:None,sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.resident.current().x[crate::bar_layout::WEATHER].is_some(),"有快照時佔位");
    h.app.settings.visible_pages[8]=false;
    for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.resident.current().x[crate::bar_layout::WEATHER].is_none(),"天氣頁被隱藏後不佔位");
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="9°")),"隱藏後不再畫溫度");
}

#[test]
#[ignore="按需匯出常駐天氣徽標預覽"]
fn weather_resident_bar_preview(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_WEATHER_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let mut h=H::new();h.app.settings.pet=false;
    h.app.weather.snapshot=Some(crate::weather::Snapshot{condition:None,attributions:vec![],place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    let mut raster=crate::preview_raster::Raster::new(1280,380);
    for frame in 0..90{
        let t=frame as f64/60.0;let env=Env{t,clock:"18:07:00".into(),battery:Some(80),charging:false,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,190.0))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});
        raster.update_textures(&output.textures_delta);
        if frame==89{
            raster.buf.fill([0.09,0.11,0.18,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dest.join("weather-bar.png"),&rgba,1280,380,image::ColorType::Rgba8).unwrap();
        }
    }
    // 設定頁「常駐排列」預覽：天氣徽標有獨立位置。換一個 H，避免複用紋理快取。
    let mut h=H::new();h.app.settings.pet=false;h.quota_accounts(1);
    h.app.weather.snapshot=Some(crate::weather::Snapshot{condition:None,attributions:vec![],place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    let mut raster=crate::preview_raster::Raster::new(1200,300);
    for frame in 0..3{
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(600.0,150.0))),time:Some(frame as f64),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.resident_order_settings(ui);});});
        raster.update_textures(&output.textures_delta);
        if frame==2{
            raster.buf.fill([0.13,0.13,0.15,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dest.join("weather-bar-order.png"),&rgba,1200,300,image::ColorType::Rgba8).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
fn weather_icons_stay_bright_on_light_skins(){
    use crate::skin_paint::{map_color,CREAM,GLASS_LIGHT,GLASS_DARK};
    let luma=|c:egui::Color32|0.299*c.r() as f32+0.587*c.g() as f32+0.114*c.b() as f32;
    let cloud=egui::Color32::from_rgb(238,242,250);let sun=egui::Color32::from_rgb(255,196,72);
    for palette in [CREAM,GLASS_LIGHT]{
        // 通用換色會把白雲反成深色、把太陽壓成暗金；天氣圖示改用自己的淺色版配色。
        assert!(luma(map_color(cloud,&palette))<90.0);
        assert!(luma(crate::weather_ui::light_tone(cloud,&palette))>190.0,"淺色皮膚下的雲應是淡藍灰");
        let bright=crate::weather_ui::light_tone(sun,&palette);
        assert!(bright.r()>=250&&bright.g()>=160&&bright.b()<80,"太陽保持明亮暖黃：{bright:?}");
        let half=crate::weather_ui::light_tone(sun.gamma_multiply(0.5),&palette);assert_eq!(half.a(),sun.gamma_multiply(0.5).a(),"保留透明度");
    }
    assert_eq!(crate::weather_ui::light_tone(cloud,&GLASS_DARK),map_color(cloud,&GLASS_DARK),"深色皮膚不受影響");
}

#[test]
#[ignore="按需匯出常駐天氣的全部圖示樣式"]
fn weather_icon_atlas_preview(){
    use crate::{paint::{Card,H as Align},theme::{self,W},weather::{self,Snapshot}};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_WEATHER_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let cases=[("晴 · 白天",0,true,"0"),("晴 · 夜間",0,false,"0"),("晴間多雲 · 白天",2,true,"1 / 2"),
        ("晴間多雲 · 夜間",2,false,"1 / 2"),("陰天",3,true,"3"),("霧 / 霧凇",45,true,"45 / 48"),
        ("細雨 / 凍毛毛雨",51,true,"51 / 53 / 55 / 56 / 57"),("雨 / 凍雨",63,true,"61 / 63 / 65 / 66 / 67"),("陣雨",80,true,"80 / 81 / 82"),
        ("雪 / 陣雪",73,true,"71 / 73 / 75 / 77 / 85 / 86"),("雷雨 / 冰雹",95,true,"95 / 96 / 99"),("未知程式碼 · 回退雲圖示",200,true,"其他程式碼")];
    for (name,palette,ppp) in [("weather-icons-dark",None,2.0f32),("weather-icons-cream",Some(crate::skin_paint::CREAM),2.0),("weather-icons-glass-light",Some(crate::skin_paint::GLASS_LIGHT),2.0),("weather-icons-dark-1x",None,1.0),("weather-icons-dark-125",None,1.25),("weather-icons-dark-150",None,1.5)] {
        let (pw,ph)=((924.0*ppp) as usize,(700.0*ppp) as usize);
        let mut h=H::new();h.app.settings.pet=false;
        let mut layout=crate::bar_layout::Layout::new(false,0,false,false,false,false,true);layout.widths[crate::bar_layout::WEATHER]=50.0;
        let mut raster=crate::preview_raster::Raster::new(pw,ph);
        for frame in 0..4 {
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(924.0,700.0))),time:Some(frame as f64),
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let mut output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                let c=Card::new(ui,Pos2::ZERO,ui.max_rect());c.fill(0.0,0.0,924.0,700.0,0.0,egui::Color32::from_rgb(14,15,19));
                c.text(20.0,38.0,"常駐天氣圖示",22.0,W::Bold,theme::WHITE,Align::Left);
                c.text(20.0,61.0,"11類圖示 · 頂部橫排與側邊豎排 · 當前實際繪製樣式",11.5,W::Regular,theme::GRAY,Align::Left);
                for (index,&(label,code,day,codes)) in cases.iter().enumerate(){
                    let tile=c.shifted(20.0+(index%3)as f32*298.0,84.0+(index/3)as f32*150.0);
                    tile.fill(0.0,0.0,286.0,138.0,11.0,egui::Color32::from_rgb(25,25,29));
                    tile.text(14.0,23.0,label,12.5,W::Semibold,theme::WHITE,Align::Left);
                    crate::weather_ui::icon(&tile,32.0,63.0,33.0,weather::kind(code,day));
                    let snapshot=Snapshot{place:"杭州".into(),temp:if code==73{-6.0}else{19.0},code,day,at:chrono::Utc::now().timestamp(),..Default::default()};
                    tile.fill(65.0,46.0,148.0,30.0,15.0,egui::Color32::BLACK);
                    let bar=tile.shifted(65.0,46.0);bar.text(32.0,21.0,"10:18",15.5,W::Bold,theme::WHITE,Align::Center);
                    h.app.paint_pill_weather(&bar,105.0,&layout,&snapshot);
                    tile.fill(234.0,40.0,36.0,47.0,18.0,egui::Color32::BLACK);
                    h.app.paint_side_weather(&tile.shifted(234.0,40.0),18.0,23.0,&snapshot);
                    for (x,label) in [(32.0,"放大"),(139.0,"頂部 15px"),(252.0,"側邊")]{tile.text(x,104.0,label,9.5,W::Regular,theme::GRAY,Align::Center);}
                    tile.text(14.0,125.0,&format!("WMO  {codes}"),8.5,W::Regular,theme::DIM,Align::Left);
                }
                c.text(20.0,690.0,"無資料、關閉天氣或極簡模式時不顯示；超過半小時未更新時，溫度轉灰。",10.5,W::Regular,theme::GRAY,Align::Left);
                // 和島上一樣按圖層換色，天氣圖示登記的淺色配色才會生效。
                if let Some(palette)=palette {let layer=ui.layer_id();crate::skin_paint::recolor_layer(ctx,layer,&[(0,crate::skin_paint::shape_count(ctx,layer))],&palette);}
            });});
            raster.update_textures(&output.textures_delta);
            if frame==3 {raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{name}.png")),&rgba,pw as u32,ph as u32,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
#[ignore="按需匯出天氣設定頁，檢查排版"]
fn weather_settings_preview(){
    use crate::settings_theme::SettingsTheme;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_WEATHER_SETTINGS_PREVIEW").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for (tag,theme,dropdown) in [("dark",SettingsTheme::Dark,false),("light",SettingsTheme::Light,false),("dropdown-dark",SettingsTheme::Dark,true),("dropdown-light",SettingsTheme::Light,true)] {
        let mut h=H::new();h.app.settings.weather_city="杭州".into();h.app.weather.city_input="杭州".into();h.app.weather.manual_city=true;h.app.settings.settings_theme=theme;
        h.ctx.style_mut(|style|style.animation_time=0.0);
        let current=crate::weather::Position{lat:30.27,lon:120.15,label:"杭州".into(),region:"浙江 · 中國".into()};
        h.app.weather.selected_city=Some(current.clone());h.app.weather.active_position=Some(current.clone());
        h.app.settings.weather_position=Some(current.clone());h.app.settings.weather_saved=vec![current.clone(),crate::weather::Position{lat:31.23,lon:121.47,label:"上海".into(),region:"上海 · 中國".into()}];
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,code:2,day:true,feels:Some(18.0),humidity:Some(58),wind:Some(12.0),high:Some(23.0),low:Some(15.0),
            sunrise:Some("06:52".into()),sunset:Some("17:21".into()),at:chrono::Utc::now().timestamp(),..Default::default()});
        h.app.weather.snapshot.as_mut().unwrap().position=Some(current);
        if dropdown {
            h.app.weather.city_input="佛山".into();
            h.app.weather.cities.seed("佛山",vec![
                crate::weather::Position{lat:23.02677,lon:113.13148,label:"佛山市".into(),region:"廣東 · 中國".into()},
                crate::weather::Position{lat:28.73401,lon:98.80612,label:"佛山".into(),region:"迪慶藏族自治州 · 雲南 · 中國".into()},
                crate::weather::Position{lat:30.04572,lon:106.41138,label:"佛山".into(),region:"重慶市 · 中國".into()},
            ]);
        }
        h.app.settings_tab=12;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let children=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=children.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(800.0,620.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            capture.lock().unwrap().push(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let mut raster=crate::preview_raster::Raster::new(1600,1240);
        for frame in 0..4 {
            if dropdown && frame==1 {h.ctx.memory_mut(|m|m.open_popup(egui::Id::new("weather-city-options")));}
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(frame as f64),..Default::default()};
            let root=h.ctx.run(raw,|ctx|h.app.settings_ui(ctx,false));
            let child_frames:Vec<_>=children.lock().unwrap().drain(..).collect();
            for child in &child_frames {raster.update_textures(&child.textures_delta);}
            raster.update_textures(&root.textures_delta);
            for child in child_frames {
                if frame==3 {raster.buf.fill([0.1,0.1,0.1,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("weather-settings-{tag}.png")),&rgba,1600,1240,image::ColorType::Rgba8).unwrap();}
                raster.free_textures(&child.textures_delta);
            }
            raster.free_textures(&root.textures_delta);
        }
        assert!(dest.join(format!("weather-settings-{tag}.png")).exists(),"必須匯出真實設定視窗");
    }
}

#[test]
#[cfg(windows)]
fn settings_window_memory_restores_only_the_child_viewport_and_keeps_live_resizing(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.ctx.set_embed_viewports(false);h.app.open_settings();
    let original=(h.app.settings.island_size,h.app.settings.dock,h.app.settings.display_id.clone());
    let size=std::sync::Arc::new(std::sync::Mutex::new(egui::vec2(900.0,600.0)));
    let resized=size.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;let size=*resized.lock().unwrap();
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),
            viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),
                inner_rect:Some(Rect::from_min_size(egui::pos2(100.0,140.0),size)),
                outer_rect:Some(Rect::from_min_size(egui::pos2(92.0,100.0),size+egui::vec2(16.0,48.0))),..Default::default()})).collect(),..Default::default()};
        let _=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));
    });
    let frame=|h:&mut H|h.ctx.run(egui::RawInput{viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(1.75),..Default::default()})).collect(),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
    let _=frame(&mut h);h.app.settings_open=false;let _=frame(&mut h);
    *size.lock().unwrap()=egui::vec2(690.0,560.0);h.app.open_settings();let root=frame(&mut h);
    assert!(root.viewport_output.iter().filter(|(id,_)|**id!=egui::ViewportId::ROOT).flat_map(|(_,v)|&v.commands).any(|c|matches!(c,egui::ViewportCommand::OuterPosition(_))),"重開時應恢復子視窗");
    assert!(!root.viewport_output.get(&egui::ViewportId::ROOT).unwrap().commands.iter().any(|c|matches!(c,egui::ViewportCommand::OuterPosition(_)|egui::ViewportCommand::InnerSize(_))));
    *size.lock().unwrap()=egui::vec2(1100.0,700.0);let live=frame(&mut h);
    assert!(!live.viewport_output.values().flat_map(|v|&v.commands).any(|c|matches!(c,egui::ViewportCommand::InnerSize(_))),"正常拖動不能每幀強制恢復舊尺寸");
    assert_eq!((h.app.settings.island_size,h.app.settings.dock,h.app.settings.display_id.clone()),original);
}

#[test]
fn quota_acceptance_monthly_limit_is_visible_on_the_card() {
    use crate::quota::{Provider,Account,plans};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let snapshot=plans::parse(plans::PlanProvider::OpenCodeGo,&serde_json::json!({"usage":{"rolling":{"percent":20},"weekly":{"percent":30},"monthly":{"percent":100}}})).unwrap();
    h.app.quota.config.accounts=vec![Account{id:"preview".into(),provider:Provider::OpenCodeGo,alias:"OpenCode Go".into(),source:Default::default(),slot:None,enabled:true,pinned:false}];
    h.app.quota.config.selected="preview".into();h.app.quota.rows.insert("preview".into(),Ok(snapshot));h.app.show_page(6);h.hover(H::local(230.0,65.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("本月"))),"月額度已經用完，但卡片沒有直接顯示本月限制");
}

#[test]
fn quota_acceptance_closing_settings_cancels_the_key_dialog() {
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings_tab=11;h.app.open_settings();h.ctx.set_embed_viewports(false);
    h.app.quota.provider=crate::quota::Provider::OpenCodeGo;h.app.quota.connect_open=true;h.app.quota.api_key="synthetic".into();
    let (connection,cancelled)=crate::quota::api::Connect::pending_test();h.app.quota.api_connecting=Some(connection);
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let _=ctx.run(egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,700.0))),viewports:std::iter::once((id,egui::ViewportInfo{events:vec![egui::ViewportEvent::Close],..Default::default()})).collect(),..Default::default()},|ctx|(viewport.viewport_ui_cb)(ctx));});
    let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
    assert!(cancelled.load(std::sync::atomic::Ordering::Acquire));assert!(h.app.quota.api_connecting.is_none());
    assert!(!h.app.settings_open);assert!(!h.app.quota.connect_open&&h.app.quota.api_key.is_empty(),"關閉設定視窗後，金鑰輸入和連線狀態仍未取消");
}

#[test]
fn quota_acceptance_relay_subscription_exhausted_week_is_visible() {
    use crate::quota::{Provider,Account,relay};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let snapshot=relay::parse_sub2api(&serde_json::json!({"mode":"unrestricted","isValid":true,"unit":"USD","remaining":0,"subscription":{"daily_limit_usd":10,"daily_usage_usd":2,"weekly_limit_usd":20,"weekly_usage_usd":20}})).unwrap();
    h.app.quota.config.accounts=vec![Account{id:"relay".into(),provider:Provider::Sub2Api,alias:"工作中轉".into(),source:"https://relay.example".into(),slot:None,enabled:true,pinned:false}];h.app.quota.config.selected="relay".into();h.app.quota.rows.insert("relay".into(),Ok(snapshot));h.app.show_page(6);h.hover(H::local(230.0,65.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("每週"))),"卡片必須顯示已耗盡的套餐週期");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="0%")),"套餐剩餘額度應顯示百分比");
}

#[test]
fn quota_acceptance_relay_key_only_response_shows_balance_unavailable(){
    use crate::quota::{Provider,Account,relay};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let snapshot=relay::parse_new_api(&serde_json::json!({"code":true,"data":{"object":"token_usage","unlimited_quota":true,"total_used":2}})).unwrap();
    h.app.quota.config.accounts=vec![Account{id:"relay".into(),provider:Provider::NewApi,alias:"測試站點".into(),source:"https://relay.example".into(),slot:None,enabled:true,pinned:true}];
    h.app.quota.config.selected="relay".into();h.app.quota.rows.insert("relay".into(),Ok(snapshot));h.app.show_page(6);h.hover(H::local(230.0,65.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="餘額暫不可查詢")));
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("不限額")||t.galley.text().contains("未設定限額"))));
}



#[test]
fn quota_acceptance_new_api_user_balance_is_visible(){
    use crate::quota::{Provider,Account,relay};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let snapshot=relay::parse_new_api_user(&serde_json::json!({"success":true,"data":{"quota":550000,"used_quota":1000000}}),&serde_json::json!({"success":true,"data":{"quota_per_unit":500000,"quota_display_type":"USD"}})).unwrap();
    h.app.quota.config.accounts=vec![Account{id:"relay-user".into(),provider:Provider::NewApi,alias:"測試站點".into(),source:"https://relay.example".into(),slot:None,enabled:true,pinned:true}];
    h.app.quota.config.selected="relay-user".into();h.app.quota.rows.insert("relay-user".into(),Ok(snapshot));h.app.show_page(6);h.hover(H::local(230.0,65.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="1.1 USD")));
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="餘額暫不可查詢")));
}

#[test]
fn quota_new_api_user_connection_switches_without_reusing_api_key(){
    fn draw(h:&mut H,raster:&mut crate::preview_raster::Raster,events:Vec<egui::Event>)->egui::FullOutput{
        h.t+=0.2;let output=h.ctx.run(egui::RawInput{time:Some(h.t),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,850.0))),events,..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));});
        raster.update_textures(&output.textures_delta);output
    }
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dark in [true,false]{
        let mut h=H::new();h.ctx.set_visuals(if dark{egui::Visuals::dark()}else{egui::Visuals::light()});
        h.app.settings_open=true;h.app.quota.provider=crate::quota::Provider::NewApi;h.app.quota.connect_open=true;h.app.quota.source="https://relay.example/tenant".into();h.app.quota.api_key="synthetic-api-key".into();
        let mut raster=crate::preview_raster::Raster::new(780,850);
        let _=draw(&mut h,&mut raster,vec![]);let _=draw(&mut h,&mut raster,vec![]);let output=draw(&mut h,&mut raster,vec![]);
        let pos=output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="使用者 Token（賬戶餘額）"=>Some(t.visual_bounding_rect().center()),_=>None}).expect("應提供使用者 Token 連線入口");
        let _=draw(&mut h,&mut raster,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let _=draw(&mut h,&mut raster,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert!(h.app.quota.relay_user_token);assert!(h.app.quota.api_key.is_empty());assert_eq!(h.app.quota.source,"https://relay.example/tenant");
        h.app.quota.relay_user_id="42".into();h.app.quota.api_key="synthetic-user-access-token".into();
        let _=draw(&mut h,&mut raster,vec![]);let output=draw(&mut h,&mut raster,vec![]);
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="使用者 ID")));
        assert!(!output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("synthetic-user-access-token"))));
        if let Some(dest)=std::env::var_os("DAOZAI_NEW_API_PREVIEW"){
            raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let path=std::path::PathBuf::from(dest);std::fs::create_dir_all(&path).unwrap();image::save_buffer(path.join(if dark{"new-api-user-dark.png"}else{"new-api-user-light.png"}),&rgba,780,850,image::ColorType::Rgba8).unwrap();
        }
        h.app.quota_close_connection();assert!(h.app.quota.api_key.is_empty()&&h.app.quota.relay_user_id.is_empty());assert!(!h.app.quota.relay_user_token);
    }
}

#[test]
fn quota_cc_switch_general_and_custom_connections_render_in_both_themes(){
    use crate::quota::{Provider,relay};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for provider in [Provider::RelayGeneral,Provider::RelayCustom]{for dark in [true,false]{
        let mut h=H::new();h.ctx.set_visuals(if dark{egui::Visuals::dark()}else{egui::Visuals::light()});
        h.app.settings_open=true;h.app.quota.connect_open=true;h.app.quota.provider=provider;h.app.quota.relay_manual=true;h.app.quota.alias="我的中轉站".into();h.app.quota.source="https://relay.example".into();h.app.quota.api_key="synthetic-private-key".into();h.app.quota.relay_script=relay::script::DEFAULT_SCRIPT.into();
        let mut raster=crate::preview_raster::Raster::new(780,850);
        for frame in 0..4{
            let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,850.0))),..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));});raster.update_textures(&output.textures_delta);
            if frame==3{
                let texts:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t.galley.text()),_=>None}).collect();let has_button=texts.contains(&"驗證並連線");assert!(!texts.iter().any(|s|s.contains("synthetic-private-key")));
                if provider==Provider::RelayCustom{assert!(texts.contains(&"查詢指令碼"));}
                if let Some(dest)=std::env::var_os("DAOZAI_CC_SWITCH_PREVIEW"){
                    raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let path=std::path::PathBuf::from(dest);std::fs::create_dir_all(&path).unwrap();image::save_buffer(path.join(format!("{}-{}.png",if provider==Provider::RelayCustom{"custom"}else{"general"},if dark{"dark"}else{"light"})),&rgba,780,850,image::ColorType::Rgba8).unwrap();
                }
                assert!(has_button,"{provider:?} 連線按鈕應可見");
            }
        }
        h.app.quota_close_connection();assert!(h.app.quota.relay_script.is_empty()&&h.app.quota.api_key.is_empty());
    }}
}

#[test]
fn quota_acceptance_grouped_connections_preserve_saved_sources() {
    use crate::quota::{Provider,Account};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for provider in [Provider::ZCode,Provider::Glm,Provider::NewApi,Provider::OneApi,Provider::Sub2Api] {
        let mut h=H::new();h.app.settings_open=true;h.app.quota.provider=provider;h.app.quota.connect_open=true;
        let source=if provider.relay(){"https://relay.example/path"}else if provider==Provider::Glm{"global"}else{"fixture/credentials.json"};
        let account=Account{id:"existing".into(),provider,alias:"原帳號".into(),source:source.into(),slot:Some("existing-reference".into()),enabled:true,pinned:false};
        h.app.quota.config.accounts=vec![account.clone()];h.app.quota.config.selected=account.id.clone();h.app.quota.connect_target=Some(account.id.clone());h.app.quota.source=source.into();
        for _ in 0..3{
            let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,850.0))),..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));});h.shapes=output.shapes;
        }
        let expected=if provider.relay(){"中轉站"}else{"GLM / ZCode"};
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==expected)),"應顯示統一入口：{expected}");
        assert_eq!(h.app.quota.provider,provider);assert_eq!(h.app.quota.source,source);assert_eq!(h.app.quota.config.accounts,[account]);
        if provider.relay(){assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="高階設定")));}
    }
}

#[test]
fn quota_glm_get_key_link_follows_region_without_submitting_the_key(){
    fn draw(h:&mut H,events:Vec<egui::Event>)->egui::FullOutput{
        h.t+=0.2;
        h.ctx.run(egui::RawInput{time:Some(h.t),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,850.0))),events,..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));})
    }
    fn click(h:&mut H,output:&egui::FullOutput,label:&str)->egui::FullOutput{
        let pos=output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("找不到控制元件：{label}"));
        let _=draw(h,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let _=draw(h,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        let _=draw(h,vec![]);draw(h,vec![])
    }
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dark in [true,false]{
        let mut h=H::new();h.ctx.set_visuals(if dark{egui::Visuals::dark()}else{egui::Visuals::light()});
        h.app.settings_open=true;h.app.quota.provider=crate::quota::Provider::Glm;h.app.quota.connect_open=true;h.app.quota.source="cn".into();h.app.quota.api_key="synthetic-glm-key".into();
        let _=draw(&mut h,vec![]);let _=draw(&mut h,vec![]);let mut output=draw(&mut h,vec![]);
        for (region,url) in [("中國區","https://bigmodel.cn/coding-plan/personal/overview"),("國際區","https://z.ai/manage-apikey/apikey-list")]{
            output=click(&mut h,&output,region);h.app.pending.clear();output=click(&mut h,&output,"獲取金鑰");
            assert!(h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenUrl(target)if target==url)));
            assert_eq!(h.app.quota.api_key,"synthetic-glm-key");assert!(h.app.quota.api_connecting.is_none());assert!(h.app.quota.config.accounts.is_empty());
        }
    }
}

#[test]
fn quota_relay_automatic_is_the_default_and_manual_types_are_hidden() {
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings_open=true;h.app.quota.provider=crate::quota::Provider::NewApi;h.app.quota.connect_open=true;
    for _ in 0..3{
        let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,850.0))),..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));});h.shapes=output.shapes;
    }
    let texts=h.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t.galley.text()),_=>None}).collect::<Vec<_>>();
    assert!(texts.contains(&"自動識別站點型別"));assert!(texts.contains(&"高階設定"));
    assert!(!texts.contains(&"站點型別")&&!texts.contains(&"One API")&&!texts.contains(&"Sub2API"));
}

#[test]
fn quota_relay_manual_types_require_advanced_settings_and_preserve_the_site(){
    fn draw(h:&mut H,events:Vec<egui::Event>)->egui::FullOutput{
        h.t+=0.2;
        h.ctx.run(egui::RawInput{time:Some(h.t),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(780.0,850.0))),events,..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|h.app.quota_settings(ui));})
    }
    fn click(h:&mut H,output:&egui::FullOutput,label:&str)->egui::FullOutput{
        let pos=output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("找不到控制元件：{label}"));
        let _=draw(h,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let _=draw(h,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        let _=draw(h,vec![]);draw(h,vec![])
    }
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dark in [true,false]{
        let mut h=H::new();h.ctx.set_visuals(if dark{egui::Visuals::dark()}else{egui::Visuals::light()});
        h.app.settings_open=true;h.app.quota.provider=crate::quota::Provider::NewApi;h.app.quota.connect_open=true;h.app.quota.source="https://relay.example/tenant/v1".into();
        let mut raster=crate::preview_raster::Raster::new(780,850);
        let first=draw(&mut h,vec![]);raster.update_textures(&first.textures_delta);
        let second=draw(&mut h,vec![]);raster.update_textures(&second.textures_delta);
        let output=draw(&mut h,vec![]);raster.update_textures(&output.textures_delta);
        if let Some(dest)=std::env::var_os("DAOZAI_RELAY_PREVIEW"){
            let jobs=h.ctx.tessellate(output.shapes.clone(),output.pixels_per_point);raster.draw(&jobs,output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let path=std::path::PathBuf::from(dest);std::fs::create_dir_all(&path).unwrap();image::save_buffer(path.join(if dark{"connect-auto-dark.png"}else{"connect-auto-light.png"}),&rgba,780,850,image::ColorType::Rgba8).unwrap();
        }
        let output=click(&mut h,&output,"高階設定");let output=click(&mut h,&output,"自動識別（推薦）");let output=click(&mut h,&output,"Sub2API");
        assert!(h.app.quota.relay_manual);assert_eq!(h.app.quota.provider,crate::quota::Provider::Sub2Api);assert_eq!(h.app.quota.source,"https://relay.example/tenant/v1");
        let output=click(&mut h,&output,"Sub2API");let _=click(&mut h,&output,"自動識別（推薦）");assert!(!h.app.quota.relay_manual);
    }
}

#[test]
fn quota_acceptance_details_open_the_correct_account() {
    use crate::quota::{Provider,Account,relay};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let s=relay::parse_sub2api(&serde_json::json!({"mode":"unrestricted","isValid":true,"unit":"USD","balance":0})).unwrap();
    let a=Account{id:"selected-relay".into(),provider:Provider::Sub2Api,alias:"原站點".into(),source:"https://relay.example".into(),slot:None,enabled:true,pinned:false};
    h.app.quota.config.accounts=vec![a.clone()];h.app.quota.config.selected=a.id.clone();h.app.quota.rows.insert(a.id.clone(),Ok(s));h.app.show_page(6);h.hover(H::local(230.0,65.0),1.0);
    h.click_text("明細 ›");assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,11);assert_eq!(h.app.quota.config.selected,a.id);
}
#[test]
fn quota_acceptance_period_colors_match_the_drawn_rings() {
    use crate::quota::{Accent,Account,Provider,Snapshot,Window};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let id="period-colors";
    h.app.quota.config.accounts=vec![Account{id:id.into(),provider:Provider::MiniMax,alias:"MiniMax".into(),source:Default::default(),slot:None,enabled:true,pinned:false}];
    h.app.quota.config.selected=id.into();h.app.quota.config.colors.insert(id.into(),Accent::Pink);
    let outer=egui::Color32::from_rgb(255,43,132);let inner=crate::paint::mix(outer,egui::Color32::WHITE,0.16);
    for seconds in [[None,None],[Some(18000),Some(604800)]] {
        h.app.quota.rows.insert(id.into(),Ok(Snapshot{windows:vec![
            Window{label:"當前週期".into(),seconds:seconds[0],remaining:62.0,reset:None,amount:None},
            Window{label:"本週".into(),seconds:seconds[1],remaining:35.0,reset:None,amount:None},
        ],extra:vec![Window{label:"工具".into(),seconds:None,remaining:80.0,reset:None,amount:None}],..Default::default()}));
        let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(400.0,200.0))),..Default::default()},|ctx|{
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                let c=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect());h.app.page_quota(&c);
            });
        });
        let expected=if seconds[0].is_none(){[inner,outer,crate::theme::GRAY]}else{[outer,inner,crate::theme::GRAY]};
        for (i,color) in expected.into_iter().enumerate(){
            let center=egui::pos2(159.0,79.0+i as f32*23.0);
            let dot=output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Circle(c)if(c.radius-2.5).abs()<0.01&&c.center.distance(center)<0.01=>Some(c.fill),_=>None}).expect("應顯示週期色點");
            assert_eq!(dot,color,"週期色點應對應實際圓環，未畫入圓環的工具額度使用灰色");
        }
    }
}

#[test]
#[ignore="按需匯出套餐、中轉、深淺主題和連線視窗的實際介面"]
fn quota_acceptance_gallery() {
    use crate::quota::{Provider,Snapshot,Account,plans,relay};
    use serde_json::json;
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_QUOTA_PREVIEW").unwrap());std::fs::create_dir_all(&dest).unwrap();
    let providers=[Provider::OpenCodeGo,Provider::MiniMax,Provider::Glm,Provider::NewApi,Provider::OneApi,Provider::Sub2Api];
    let snapshots:Vec<Snapshot>=vec![
        plans::parse(plans::PlanProvider::OpenCodeGo,&json!({"usage":{"rolling":{"percent":25,"resetsAt":"2030-01-01T00:00:00Z"},"weekly":{"percent":50,"resetsAt":"2030-01-03T00:00:00Z"},"monthly":{"percent":100,"resetsAt":"2030-02-01T00:00:00Z"}}})).unwrap(),
        plans::parse(plans::PlanProvider::MiniMaxChina,&json!({"model_remains":[{"model_name":"general","current_interval_remaining_percent":62,"current_weekly_remaining_percent":35}]})).unwrap(),
        plans::parse(plans::PlanProvider::GlmChina,&json!({"data":{"limits":[{"type":"TOKENS_LIMIT","percentage":30,"unit":3,"number":5,"nextResetTime":1900000000000i64},{"type":"CREDIT_LIMIT","percentage":52,"unit":6,"number":1,"nextResetTime":1900604800000i64},{"type":"TIME_LIMIT","percentage":10}]}})).unwrap(),
        relay::parse_new_api(&json!({"code":true,"data":{"object":"token_usage","total_granted":500000,"total_available":250000,"total_used":250000,"unlimited_quota":false,"user_quota":{"total_available":100000,"total_used":400000}}})).unwrap(),
        relay::parse_one_api(&json!({"object":"billing_subscription","hard_limit_usd":100}),&json!({"object":"list","total_usage":2500})).unwrap(),
        relay::parse_sub2api(&json!({"mode":"unrestricted","isValid":true,"unit":"USD","remaining":0,"subscription":{"daily_limit_usd":10,"daily_usage_usd":5,"weekly_limit_usd":20,"weekly_usage_usd":20}})).unwrap(),
    ];
    for (index,provider) in providers.into_iter().enumerate() {
        let mut h=H::new();h.app.settings.pet=false;
        let a=Account{id:"preview".into(),provider,alias:provider.name().into(),source:if provider.relay(){"https://relay.example".into()}else{"cn".into()},slot:Some("synthetic-reference".into()),enabled:true,pinned:true};
        h.app.quota.config.accounts=vec![a.clone()];h.app.quota.config.selected=a.id.clone();h.app.quota.rows.insert(a.id,Ok(snapshots[index].clone()));
        h.app.show_page(6);let mut raster=feature_raster();
        shoot_feature(&mut h,&mut raster,&dest,&format!("island-{index}"),60,Some(H::local(240.0,65.0)),"2026-11-10");
    }
    for (group,indices) in [("plans",vec![0usize,1,2]),("relay",vec![3usize,4,5])] {
        for light in [false,true] {
            let mut h=H::new();h.app.settings.settings_theme=if light{crate::settings_theme::SettingsTheme::Light}else{crate::settings_theme::SettingsTheme::Dark};
            for &i in &indices {
                let id=format!("preview-{i}");h.app.quota.config.accounts.push(Account{id:id.clone(),provider:providers[i],alias:providers[i].name().into(),source:if providers[i].relay(){"https://relay.example".into()}else{"cn".into()},slot:Some("synthetic-reference".into()),enabled:true,pinned:true});
                h.app.quota.rows.insert(id.clone(),Ok(snapshots[i].clone()));if h.app.quota.config.selected.is_empty(){h.app.quota.config.selected=id;}
            }
            h.app.settings_tab=11;h.app.open_settings();h.ctx.set_embed_viewports(false);
            let size=egui::vec2(780.0,1080.0);let ppp=1.25;let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(ppp),focused:Some(true),..Default::default()})).collect(),focused:true,..Default::default()};*capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));});
            let mut raster=crate::preview_raster::Raster::new((size.x*ppp)as usize,(size.y*ppp)as usize);
            for f in 0..5{
                let root=h.ctx.run(egui::RawInput{viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
                raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
                if f==4{raster.buf.fill([0.1,0.1,0.11,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("settings-{group}-{}.png",if light{"light"}else{"dark"})),&rgba,(size.x*ppp)as u32,(size.y*ppp)as u32,image::ColorType::Rgba8).unwrap();}
                raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
        }
    }
    for (i,provider) in providers.into_iter().enumerate() {
        let mut h=H::new();h.app.quota.provider=provider;h.app.quota.connect_open=true;
        h.app.settings.settings_theme=crate::settings_theme::SettingsTheme::Dark;h.app.settings_tab=11;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;*capture.lock().unwrap()=Some(ctx.run(egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,600.0))),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.5),focused:Some(true),..Default::default()})).collect(),..Default::default()},|ctx|(viewport.viewport_ui_cb)(ctx)));});
        let mut raster=crate::preview_raster::Raster::new(1035,900);
        for f in 0..5{
            let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
            if f==4{raster.buf.fill([0.1,0.1,0.11,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("connect-{i}.png")),&rgba,1035,900,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
        }
    }
    let images:Vec<_>=(0..6).map(|i|image::open(dest.join(format!("island-{i}.png"))).unwrap().to_rgba8()).collect();
    let width=images.iter().map(|i|i.width()).max().unwrap();let height=images.iter().map(|i|i.height()).max().unwrap();
    let mut sheet=image::RgbaImage::from_pixel(width*2,height*3,image::Rgba([18,20,26,255]));
    for (i,img) in images.iter().enumerate(){image::imageops::replace(&mut sheet,img,((i%2)as u32*width)as i64,((i/2)as u32*height)as i64);}sheet.save(dest.join("island-overview.png")).unwrap();
}

#[test]
fn media_sampling_is_fast_only_while_the_music_page_is_watched(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),duration:120.0,playing:true,..Default::default()};
    h.frame(vec![]);
    h.app.open=false;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_BACKGROUND,"收起且不顯示歌詞時只需跟上歌名");
    h.app.settings.island_lyrics=true;h.frame(vec![]);h.app.open=false;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_LYRICS,"收起顯示歌詞時次快");
    h.app.open=true;h.app.page=0;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_FAST,"展開音樂頁保持原來的 250 毫秒");
    h.app.page=1;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_BACKGROUND,"展開其他頁面時不必按最快頻率取樣");
}

#[test]
fn calendar_signature_matches_without_copying(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let h=H::new();
    let known=h.app.calendar_configs();
    assert!(h.app.calendar_configs_match(&known));
    assert_eq!(h.app.calendar_configs_match(&[]),known.is_empty());
}

fn raised(h:&H,title:&str)->bool{
    h.app.alerts.iter().chain(h.app.notice.as_ref()).any(|alert|alert.title==title)
}

#[test]
fn closed_modules_stop_their_reminders_and_timers(){
    use crate::app::{FOCUS_PAGE,TODO_PAGE,WATER_PAGE};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for open in [true,false] {
        let mut h=H::new();h.frame(vec![]);
        h.app.pomo.running=true;h.app.pomo.remaining=0.5;
        for page in [FOCUS_PAGE,TODO_PAGE,WATER_PAGE] {h.app.set_page_visible(page,open);}
        h.app.water.remind=true;h.app.water.drunk_ml=0;h.app.last_reminder=h.t-1e6;
        h.app.todos.items.push(crate::model::TodoItem{text:"交週報".into(),due:Some("2026-10-28 09:40".into()),..Default::default()});
        if !open {assert!(!h.app.pomo.running,"關閉專注模組時立即停止計時");h.app.pomo.running=true;}
        for _ in 0..60 {h.frame(vec![]);}
        assert_eq!(raised(&h,"喝水時間到了"),open,"飲水模組開啟={open}");
        assert_eq!(raised(&h,"待辦到時間了"),open,"待辦模組開啟={open}");
        assert_eq!(raised(&h,"專注計時"),open,"專注模組開啟={open}");
        if !open {assert!(!h.app.pomo.running && h.app.todos.items[0].due.is_some() && !h.app.todos.items[0].reminded,"關閉期間不消耗提醒，重新開啟後仍會提醒");}
    }
}

#[test]
fn background_services_follow_the_module_switch(){
    use crate::app::{MAIL_PAGE,QUOTA_PAGE,SHELF_PAGE};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.mail_enabled=true;h.app.settings.clipboard_history=true;h.app.quota.config.enabled=true;
    assert!(h.app.mail_active() && h.app.clipboard_active() && h.app.quota_active());
    for page in [MAIL_PAGE,QUOTA_PAGE,SHELF_PAGE] {h.app.set_page_visible(page,false);}
    assert!(!h.app.mail_active() && !h.app.clipboard_active() && !h.app.quota_active(),"頁面開關就是模組總開關");
    h.frame(vec![]);
    assert!(!h.app.clipboard.is_enabled(),"置物架關閉後不再記錄剪貼簿");
    for page in [MAIL_PAGE,QUOTA_PAGE,SHELF_PAGE] {h.app.set_page_visible(page,true);}
    h.app.settings.mail_enabled=false;h.app.settings.clipboard_history=false;h.app.quota.config.enabled=false;
    assert!(!h.app.mail_active() && !h.app.clipboard_active() && !h.app.quota_active(),"模組內的開關仍然有效");
}

#[test]
fn every_module_settings_tab_maps_to_its_page(){
    let titles=["常規","音樂與歌詞","專注","健康提醒","待辦","置物","通知","信箱","日程","關於","頁面管理","AI Usage","天氣"];
    let mut pages:Vec<_>=(0..titles.len()).filter_map(crate::extras::settings_tab_module).collect();
    for tab in 0..titles.len() {
        if let Some(page)=crate::extras::settings_tab_module(tab) {assert!(crate::app::PAGE_NAMES[page].chars().next()==titles[tab].chars().next(),"{} ↔ {}",titles[tab],crate::app::PAGE_NAMES[page]);}
    }
    pages.sort();assert_eq!(pages,(0..crate::app::PAGE_NAMES.len()).collect::<Vec<_>>(),"每個功能頁面各有一個設定分頁");
}

#[test]
fn gws_connected_event_auto_binds_gmail_and_calendar_and_respects_auto_bind_flag() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    assert!(!h.app.settings.gws_onboarded);
    h.app.settings.gws_onboarded = true;
    h.app.settings.save();
    assert!(crate::model::Settings::load().gws_onboarded);

    let status = crate::gws::AuthStatus {
        installed: true,
        client_config_exists: true,
        token_valid: true,
        user: "alice@gmail.com".into(),
        scopes: vec![
            "https://www.googleapis.com/auth/gmail.readonly".into(),
            "https://www.googleapis.com/auth/calendar.readonly".into(),
        ],
        has_gmail: true,
        has_calendar: true,
    };
    h.app.pending.clear();
    h.app.gws_event(crate::gws::Event {
        phase: crate::gws::Phase::Connected(status.clone()),
        auto_bind: true,
    });

    assert_eq!(h.app.mail.accounts.len(), 1);
    assert_eq!(h.app.mail.accounts[0].provider, crate::mail::Provider::GoogleWorkspace);
    assert_eq!(h.app.mail.accounts[0].email, "alice@gmail.com");
    assert!(h.app.pending.iter().any(|c| matches!(c, crate::app::Cmd::MailSync)));

    assert_eq!(h.app.calendar.feeds.len(), 1);
    assert_eq!(h.app.calendar.feeds[0].config.provider, crate::calendar::Provider::GoogleWorkspace);
    assert_eq!(h.app.calendar.feeds[0].config.username, "alice@gmail.com");
    assert!(h.app.pending.iter().any(|c| matches!(c, crate::app::Cmd::CalendarSync)));
    assert_eq!(crate::mail::load_accounts().len(), 1);
    assert_eq!(crate::calendar::load_configs().len(), 1);

    h.app.mail.accounts.clear();
    h.app.mail_accounts_changed();
    let _ = h.app.forget_calendar();
    let _ = crate::calendar::forget_account(&crate::calendar::Config {
        provider: crate::calendar::Provider::GoogleWorkspace,
        username: "alice@gmail.com".into(),
        href: None,
        server: None,
    });
    h.app.pending.clear();

    h.app.gws_event(crate::gws::Event {
        phase: crate::gws::Phase::Connected(status),
        auto_bind: false,
    });
    assert!(h.app.mail.accounts.is_empty());
    assert!(h.app.calendar.feeds.is_empty());
    assert!(matches!(h.app.gws.phase, crate::gws::Phase::Connected(_)));
}

#[test]
fn gws_install_button_shows_only_when_missing_and_requests_the_download() {
    use crate::gws::Phase;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_mail_settings();h.ctx.set_embed_viewports(false);
    let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
    let events=std::sync::Arc::new(Mutex::new(Vec::new()));let input=events.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,focused:true,events:std::mem::take(&mut *input.lock().unwrap()),
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,3200.0))),
            viewports:std::iter::once((id,egui::ViewportInfo{focused:Some(true),..Default::default()})).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    let show=|h:&mut H|{
        for _ in 0..3{let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
        output.lock().unwrap().take().unwrap()
    };
    let label_at=|child:&egui::FullOutput,label:&str|child.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None});
    // 已經有 gws（已登入或尚未登入）時不顯示下載按鈕。
    for phase in [Phase::Idle,Phase::NotLoggedIn,Phase::NeedsSetup,Phase::Installing,Phase::Failed("x".into())] {
        h.app.gws.phase=phase.clone();
        let child=show(&mut h);
        assert!(label_at(&child,"下載並安裝 gws").is_none(),"{phase:?} 不應出現下載按鈕");
    }
    // 沒裝，或上次安裝失敗：顯示按鈕，點下去先進入安裝中並交給背景服務。
    for phase in [Phase::NotInstalled,Phase::InstallFailed("網路中斷".into())] {
        h.app.gws.phase=phase.clone();h.app.pending.clear();
        let child=show(&mut h);
        let point=label_at(&child,"下載並安裝 gws").unwrap_or_else(||panic!("{phase:?} 應出現下載按鈕"));
        // 先把指標移上去（egui 按鈕要前一畫格已懸停），再分別按下、放開。
        for step in [None,Some(true),Some(false)]{
            let mut sent=vec![egui::Event::PointerMoved(point)];
            if let Some(pressed)=step{sent.push(egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()});}
            *events.lock().unwrap()=sent;
            let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
        }
        let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
        assert!(h.app.pending.iter().any(|c|matches!(c,crate::app::Cmd::GwsInstall)),"{phase:?}：按下後應要求下載安裝");
        assert_eq!(h.app.gws.phase,Phase::Installing);
        let child=show(&mut h);
        assert!(child.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("正在下載並安裝 gws"))),"安裝中應顯示進度狀態");
    }
}

#[test]
fn gws_install_events_explain_source_and_failure() {
    use crate::gws::{Event,Phase};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.gws_event(Event{phase:Phase::NotInstalled,auto_bind:false});
    assert!(h.app.gws.note.contains("下載並安裝 gws")&&h.app.gws.note.contains("GitHub")&&h.app.gws.note.contains(crate::gws_install::VERSION),"沒裝時要說明可一鍵下載與來源版本：{}",h.app.gws.note);
    h.app.gws_event(Event{phase:Phase::Installing,auto_bind:false});
    assert_eq!(h.app.gws.phase,Phase::Installing);
    h.app.gws_event(Event{phase:Phase::InstallFailed("下載的檔案雜湊與預期不符".into()),auto_bind:false});
    assert!(h.app.gws.note.contains("雜湊與預期不符")&&h.app.gws.note.contains("重試"),"{}",h.app.gws.note);
    assert!(h.app.mail.accounts.is_empty()&&h.app.calendar.feeds.is_empty(),"安裝失敗不會綁定任何帳號");
}

#[test]
fn mail_filter_defaults_to_important_and_persists() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    assert_eq!(h.app.settings.mail_filter, 0);
    for mode in [1u8, 2u8, 0u8] {
        h.app.settings.mail_filter = mode;
        h.app.settings.save();
        assert_eq!(crate::model::Settings::load().mail_filter, mode);
    }
}

#[test]
fn multiple_ics_calendars_save_without_password_and_coexist_in_feeds() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    assert!(h.app.calendar.feeds.is_empty());

    let ics1 = crate::calendar::Config {
        provider: crate::calendar::Provider::Google,
        username: "Work".into(),
        href: Some("https://calendar.google.com/calendar/ical/work/public/basic.ics".into()),
        server: Some("https://calendar.google.com/calendar/ical/work/public/basic.ics".into()),
    };
    let ics2 = crate::calendar::Config {
        provider: crate::calendar::Provider::Google,
        username: "Family".into(),
        href: Some("https://calendar.google.com/calendar/ical/family/public/basic.ics".into()),
        server: Some("https://calendar.google.com/calendar/ical/family/public/basic.ics".into()),
    };

    for cfg in [ics1.clone(), ics2.clone()] {
        h.app.calendar.test_password.clear();
        h.app.calendar_event(crate::calendar::Event::Test {
            ok: true,
            message: "連線成功".into(),
            count: 0,
            config: Some(cfg),
        });
    }

    assert_eq!(h.app.calendar.feeds.len(), 2);
    assert_eq!(crate::calendar::load_configs(), vec![ics1.clone(), ics2]);

    h.app.calendar_event(crate::calendar::Event::Calendars {
        config: ics1.clone(),
        list: vec![crate::calendar::CalendarInfo {
            href: ics1.server.clone().unwrap(),
            name: "Work".into(),
        }],
    });
    h.app.select_calendar_account(ics1);
    h.app.settings_open = true;
    h.app.settings_tab = 8;
    for _ in 0..3 {
        let out = h.ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(690.0, 4000.0))),
                ..Default::default()
            },
            |ctx| h.app.settings_ui(ctx, false),
        );
        h.shapes = out.shapes;
    }
    assert!(
        h.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().contains("已選 1 / 1"))),
        "單個日曆也應顯示勾選清單，方便停用或核對名稱"
    );
}

#[test]
fn line_dual_channel_notifications_deduplicate_and_wake_main_window() {
    use crate::app::{Cmd, Messenger, Unread};
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.settings.read_notifications = false;
    h.app.settings.line_notifications = true;
    let unread = Unread { source: Messenger::Line, pid: 4321, window: 0x5678 };

    // 1. 即使 Windows 系統通知未連線，LINE 工作列閃動仍能建立提醒並保留未讀狀態。
    h.app.receive_messenger_reminder(unread);
    h.frame(vec![]);
    assert_eq!(h.app.pending_messengers, vec![unread]);
    assert!(h.app.notice.as_ref().is_some_and(|n| n.messenger() == Some(Messenger::Line)));

    // 2. 若同一條訊息接著透過 Windows 系統通知送達，應平滑升級為帶正文的通知而不重複排隊兩條。
    h.app.settings.read_notifications = true;
    h.app.settings.notification_preview = true;
    h.app.receive_notification_from(
        "LINE".into(),
        "NAVER.WIN32_LINEwin8_8ptj331gd3tyt!LINE".into(),
        "王小明 · 今晚七點吃飯嗎".into(),
        Some(unread),
    );
    h.frame(vec![]);
    assert_eq!(h.app.alerts.iter().filter(|a| a.messenger() == Some(Messenger::Line)).count(), 0);
    assert_eq!(h.app.notice.as_ref().map(|n| n.body.as_str()), Some("王小明 · 今晚七點吃飯嗎"));

    for _ in 0..45 { h.frame(vec![]); }
    h.click_text("檢視");
    assert!(h.cmds.contains(&Cmd::ViewMessenger(unread)));

    // 3. 關閉 LINE 開關後，兩通道皆不再提醒。
    h.app.discard_unread(unread);
    h.app.settings.line_notifications = false;
    h.app.receive_messenger_reminder(unread);
    h.app.receive_notification_from("LINE".into(), "LINE".into(), "新訊息".into(), Some(unread));
    assert!(h.app.pending_messengers.is_empty());
    assert!(h.app.alerts.is_empty() && h.app.notice.is_none());
}

#[test]
fn fullscreen_show_notices_toggle_pops_island_for_alerts_while_keeping_health_silent() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.settings.hide_fullscreen = true;
    h.app.settings.fullscreen_show_notices = true;
    h.app.settings.read_notifications = true;
    h.app.settings.notification_preview = true;
    h.app.settings.lunch_rest = false;
    h.app.settings.water_start = 0;
    h.app.settings.water_end = 0;
    h.fullscreen = true;
    h.frame(vec![]);

    // 閒置無通知時應保持隱藏。
    assert!(h.app.fullscreen_hidden);
    assert!(h.app.should_hide_for_fullscreen(true));

    // 全螢幕期間健康提醒仍保持靜默，不會浮現島體。
    h.app.alert("喝水時間到了", "喝點水吧", 2, true);
    h.frame(vec![]);
    assert!(h.app.notice.is_none());
    assert!(h.app.should_hide_for_fullscreen(true));

    // 收到 LINE / 外部通知時暫時浮現島體，並正常倒數 4.5 秒後自動重新隱藏。
    h.app.receive_notification("LINE".into(), "NAVER.WIN32_LINEwin8_8ptj331gd3tyt!LINE".into(), "會議改期".into());
    h.frame(vec![]);
    assert!(h.app.notice.is_some());
    assert!(!h.app.should_hide_for_fullscreen(true));
    assert_eq!(h.app.notice.as_ref().map(|n| n.body.as_str()), Some("會議改期"));

    h.app.t += 4.3;
    h.app.tick_alerts();
    assert!(h.app.notice.is_some());
    assert!(!h.app.should_hide_for_fullscreen(true));

    h.app.t += 0.3;
    h.app.tick_alerts();
    assert!(h.app.notice.is_none());
    assert!(h.app.should_hide_for_fullscreen(true));
}

#[test]
fn liquid_glass_and_settings_controls_persist_and_render_without_clipping() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    // h.app.settings.skin = crate::model::Skin::LiquidGlass;
    h.app.settings.fullscreen_show_notices = true;
    h.app.settings.line_notifications = false;
    h.app.settings.save();
    let loaded = crate::model::Settings::load();
    // assert_eq!(loaded.skin, crate::model::Skin::LiquidGlass);
    assert!(loaded.fullscreen_show_notices);
    assert!(!loaded.line_notifications);

    // 檢查常規頁（皮膚按鈕與全螢幕通知開關）與通知頁（LINE 開關）於預設設定視窗寬度下均無裁切。
    h.ctx.set_embed_viewports(false);
    let captured = std::sync::Arc::new(Mutex::new(None));
    let capture = captured.clone();
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let id = viewport.ids.this;
        let raw = egui::RawInput {
            viewport_id: id,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(690.0, 2400.0))),
            viewports: std::iter::once((id, egui::ViewportInfo { focused: Some(true), ..Default::default() })).collect(),
            ..Default::default()
        };
        *capture.lock().unwrap() = Some(ctx.run(raw, |ctx| (viewport.viewport_ui_cb)(ctx)));
    });
    for (tab, expected_labels) in [
        (0usize, vec!["經典", "奶油", "毛玻璃·淺", "毛玻璃·深", /* "液態玻璃", */ "懸浮雲", "全螢幕隱藏時仍彈出通知"]),
        (6usize, vec!["LINE 新訊息提醒"]),
    ] {
        h.app.settings_tab = tab;
        h.app.open_settings();
        for _ in 0..3 { let _ = h.ctx.run(Default::default(), |ctx| h.app.settings_ui(ctx, false)); }
        let out = captured.lock().unwrap().take().unwrap();
        for label in expected_labels {
            let found = out.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some((s.clip_rect, t.visual_bounding_rect())),
                _ => None,
            }).unwrap_or_else(|| panic!("設定分頁 {tab} 缺少文字 {label}"));
            assert!(found.0.contains_rect(found.1), "設定分頁 {tab} 文字 {label} 被裁切：{:?} vs {:?}", found.0, found.1);
        }
    }
}

#[test]
#[ignore="按需用本機 ai-quota 設定與快取匯出 Antigravity / Codex 用量畫面，不發出網路請求"]
fn quota_local_accounts_gallery(){
    use crate::quota::{Config,Event,Snapshot};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_QUOTA_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    let mut config=Config::load();
    let cache:serde_json::Value=serde_json::from_slice(&std::fs::read(crate::model::config_dir().join("ai-quota-cache.json")).unwrap()).unwrap();
    // 只匯出啟用且有快取的帳號，缺資料的帳號略過而不是中斷。
    config.accounts.retain(|a|a.enabled&&cache[&a.id]["snapshot"].is_object());assert!(!config.accounts.is_empty(),"本機沒有可匯出的用量帳號快取");
    let mut scenes:Vec<Option<String>>=config.accounts.iter().map(|a|Some(a.id.clone())).collect();scenes.push(None);
    for scene in scenes{
        let mut h=H::new();h.app.settings.skin=crate::model::Skin::Classic;h.app.settings.pet=false;
        h.app.quota.config=config.clone();if let Some(id)=&scene{h.app.quota.config.selected=id.clone();}
        for a in &config.accounts{let Ok(snapshot)=serde_json::from_value::<Snapshot>(cache[&a.id]["snapshot"].clone()) else{continue;};h.app.quota_event(Event{id:a.id.clone(),result:Ok(snapshot)});}
        let Some(id)=scene else{
            h.app.settings.settings_theme=crate::settings_theme::SettingsTheme::Dark;h.app.settings_tab=11;h.app.open_settings();h.ctx.set_embed_viewports(false);
            let size=egui::vec2(690.0,620.0);let ppp=1.5;let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};*capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));});
            let mut raster=crate::preview_raster::Raster::new(1035,930);
            for frame in 0..8{
                let root=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
                if frame==7{raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join("settings.png"),&rgba,1035,930,image::ColorType::Rgba8).unwrap();}
                raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
            continue;
        };
        // 同一服務可能有多個帳號：檔名加上序號，避免互相覆蓋。
        let index=config.accounts.iter().position(|a|a.id==id).unwrap();let name=format!("{}-{}",index+1,config.accounts[index].provider.name().to_lowercase());
        h.app.show_page(6);let mut raster=feature_raster();
        for frame in 0..75{
            h.t+=1.0/60.0;let cursor=Some(H::local(300.0,60.0));
            let env=Env{t:h.t,clock:"20:10:00".into(),today:"2026-11-27".into(),date:"11月27日 週五".into(),cursor,bt_ready:true,..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),events:vec![egui::Event::PointerMoved(cursor.unwrap())],viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);});});raster.update_textures(&output.textures_delta);
            if frame==74{
                raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw(raster.w as u32,raster.h as u32,rgba).unwrap();image::imageops::crop_imm(&image,200,0,880,380).to_image().save(dest.join(format!("island-{name}.png"))).unwrap();
            }raster.free_textures(&output.textures_delta);
        }
    }
}
