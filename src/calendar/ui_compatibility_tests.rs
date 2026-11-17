use super::*;
use crate::calendar::{Config,Event,Provider};

#[test]
fn ics_import_retains_good_records_and_distinguishes_duplicates_and_bad_records(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let at=(chrono::Utc::now()+chrono::Duration::days(1)).format("%Y%m%dT%H%M%SZ").to_string();
    let ics=format!("BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:{at}\nSUMMARY:同一時間的面試甲\nEND:VEVENT\nBEGIN:VEVENT\nDTSTART:{at}\nSUMMARY:同一時間的面試乙\nEND:VEVENT\nBEGIN:VEVENT\nUID:bad\nDTSTART;TZID=Unsupported/Zone:20261112T100000\nEND:VEVENT\nBEGIN:VEVENT\nUID:old\nDTSTART:20000101T000000Z\nEND:VEVENT\nBEGIN:VEVENT\nUID:future\nDTSTART:21000101T000000Z\nEND:VEVENT\nBEGIN:VTODO\nSUMMARY:待辦\nEND:VTODO\nEND:VCALENDAR");
    let files=vec![("mock.ics".into(),ics)];let note=h.app.import_ics(&files);
    assert_eq!(h.app.calendar.local.len(),2,"缺少 UID 的同一時刻不同日程不能互相覆蓋");
    for phrase in ["已匯入 2 個","1 條日程無法解析","2 個超出匯入範圍","1 條待辦"]{assert!(note.contains(phrase),"{note}");}
    let ids=h.app.calendar.local.iter().map(|entry|entry.event.uid.clone()).collect::<Vec<_>>();assert_ne!(ids[0],ids[1]);
    let note=h.app.import_ics(&files);assert_eq!(h.app.calendar.local.len(),2);assert!(note.contains("2 個已存在已跳過"));
    let legacy=format!("ics--{}",h.app.calendar.local[0].event.start);h.app.calendar.local[0].event.uid=legacy.clone();
    h.app.import_ics(&files);assert_eq!(h.app.calendar.local.len(),2);assert_eq!(h.app.calendar.local[0].event.uid,legacy,"已有本機記錄應原樣保留");
}

#[test]
fn ics_import_reports_the_capacity_limit_without_calling_new_records_duplicates(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let at=(chrono::Utc::now()+chrono::Duration::days(1)).format("%Y%m%dT%H%M%SZ").to_string();
    let first=format!("BEGIN:VEVENT\nUID:existing\nDTSTART:{at}\nEND:VEVENT");h.app.import_ics(&[("first.ics".into(),first)]);
    let entry=h.app.calendar.local[0].clone();h.app.calendar.local=(0..5000).map(|i|{let mut entry=entry.clone();entry.event.uid=format!("existing-{i}");entry}).collect();
    let new=format!("BEGIN:VEVENT\nUID:new\nDTSTART:{at}\nEND:VEVENT");let note=h.app.import_ics(&[("new.ics".into(),new)]);
    assert_eq!(h.app.calendar.local.len(),5000);assert!(note.contains("5000"));assert!(note.contains("1 個未匯入"));assert!(!note.contains("已存在"));
}

#[test]
fn calendar_partial_warnings_are_scoped_and_clear_after_a_complete_refresh(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let config=Config{provider:Provider::DingTalk,username:"mock".into(),href:None,server:None};
    h.app.calendar.config=Some(config.clone());let month=h.app.calendar.view.month;
    h.app.calendar_event(Event::MonthLoaded{config:config.clone(),month,result:Ok(vec![])});
    let message="1 處日曆讀取失敗；讀取日程失敗（403）".to_string();
    h.app.calendar_event(Event::Warning{config:config.clone(),month:Some(month),message:message.clone()});
    assert_eq!(h.app.calendar.view.warning,message);assert!(h.app.calendar.view.error.is_empty());
    let other=Config{provider:Provider::Feishu,..config.clone()};
    h.app.calendar_event(Event::Warning{config:other,month:Some(month),message:"其他帳號".into()});
    h.app.calendar_event(Event::Warning{config:config.clone(),month:Some(month+chrono::Duration::days(32)),message:"舊月份".into()});
    assert_eq!(h.app.calendar.view.warning,message);
    h.app.show_page(crate::app::CALENDAR_PAGE);h.hover(H::local(200.0,12.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="部分同步")));
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="部分日程未同步")));
    for shape in &h.shapes{if let egui::Shape::Text(text)=&shape.shape{if ["部分同步","部分日程未同步"].contains(&text.galley.text()){assert!(shape.clip_rect.contains_rect(text.visual_bounding_rect()));}}}
    h.app.calendar_event(Event::MonthLoaded{config:config.clone(),month,result:Ok(vec![])});
    h.app.calendar_event(Event::Warning{config,month:Some(month),message:String::new()});assert!(h.app.calendar.view.warning.is_empty());
}

#[test]
#[ignore="按需匯出日曆部分同步提示的實際樣式"]
fn calendar_partial_sync_preview(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let destination=std::path::PathBuf::from(std::env::var_os("DAOZAI_CALENDAR_PREVIEW").expect("需指定預覽目錄"));std::fs::create_dir_all(&destination).unwrap();
    for empty in [false,true]{
        let mut h=H::new();let config=Config{provider:Provider::DingTalk,username:"mock".into(),href:None,server:None};
        h.app.calendar.config=Some(config.clone());let month=h.app.calendar.view.month;
        let start=(chrono::Local::now()+chrono::Duration::minutes(30)).timestamp();
        let events=if empty{vec![]}else{vec![crate::mail::Meeting{uid:"preview".into(),title:"面試安排".into(),start,end:Some(start+1800),all_day:false,at:"面試時間".into()}]};
        h.app.calendar_event(Event::MonthLoaded{config:config.clone(),month,result:Ok(events)});
        h.app.calendar_event(Event::Warning{config,month:Some(month),message:"1 處日曆讀取失敗；日曆伺服器拒絕訪問（403）".into()});h.app.show_page(crate::app::CALENDAR_PAGE);
        let mut raster=feature_raster();let day=chrono::Local::now().date_naive().to_string();
        shoot_feature(&mut h,&mut raster,&destination,if empty{"calendar-partial-empty"}else{"calendar-partial-events"},100,Some(H::local(200.0,12.0)),&day);
    }
}

#[test]
fn unticking_a_calendar_is_remembered_for_that_account_and_resyncs(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let nanos=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir=std::env::temp_dir().join(format!("daozai-calendar-pick-{}-{nanos}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();std::env::set_var("DAOZAI_DATA_DIR",&dir);
    let mut h=H::new();
    let config=Config{provider:Provider::Custom,username:"pick@example.com".into(),href:None,server:Some("https://dav.example.com/".into())};
    crate::calendar::save_config(&config).unwrap();h.app.calendar=crate::app::CalendarState::load();
    let list:Vec<_>=["work","family","birthdays"].iter().map(|id|crate::calendar::CalendarInfo{href:format!("https://dav.example.com/{id}/"),name:id.to_string()}).collect();
    h.app.calendar_event(Event::Calendars{config:config.clone(),list:list.clone()});
    assert_eq!(h.app.calendar.feeds[0].calendars,list);
    assert!(h.app.calendar.feeds[0].skipped.is_empty(),"沒選過時預設全部同步");

    h.app.select_calendar_account(config.clone());h.app.settings_open=true;h.app.settings_tab=8;
    let shown=|h:&mut H,needle:&str|{
        for _ in 0..3{let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,560.0))),..Default::default()},|ctx|{h.app.settings_ui(ctx,false);});h.shapes=out.shapes;}
        h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text().contains(needle)))
    };
    assert!(shown(&mut h,"已選 3 / 3"),"設定頁要列出日曆，預設全選");

    h.app.pending.clear();
    h.app.set_calendar_choice(&config,Some(&list[1].href),false);
    assert_eq!(h.app.calendar.feeds[0].skipped,vec![list[1].href.clone()]);
    assert_eq!(crate::calendar::skipped_calendars(&config),vec![list[1].href.clone()],"取消勾選要存到磁碟");
    assert!(h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::CalendarSync)),"取消勾選後要立刻重新同步");
    assert_eq!(crate::app::CalendarState::load().feeds[0].skipped,vec![list[1].href.clone()],"重新啟動後仍記得");
    assert!(shown(&mut h,"已選 2 / 3"));

    h.app.set_calendar_choice(&config,Some(&list[1].href),true);
    assert!(h.app.calendar.feeds[0].skipped.is_empty()&&crate::calendar::skipped_calendars(&config).is_empty(),"勾回去＝恢復全部同步");
    h.app.set_calendar_choice(&config,Some(&list[0].href),false);h.app.set_calendar_choice(&config,Some(&list[2].href),false);
    h.app.set_calendar_choice(&config,None,true);
    assert!(crate::calendar::skipped_calendars(&config).is_empty(),"「全部選取」清掉所有取消勾選");
    std::env::remove_var("DAOZAI_DATA_DIR");let _=std::fs::remove_dir_all(&dir);
}
