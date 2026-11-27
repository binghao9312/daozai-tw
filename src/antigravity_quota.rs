//! Antigravity 額度：呼叫官方 `agy -p /usage --output-format json` 取得各模型組的 5 小時與每週額度。
//! 斜線指令由 CLI 本機處理，不送出對話、不消耗額度；登入與續期完全交給 agy，島仔不讀取任何令牌。
use super::{Account,Config,Provider,Snapshot,Window};
use serde_json::Value;
use std::{io::Read,path::{Path,PathBuf},process::{Command,Stdio},time::{Duration,Instant}};

const TIMEOUT:Duration=Duration::from_secs(60);
const MAX_OUTPUT:u64=1024*1024;

/// 本機 agy 執行檔：優先官方安裝位置，其次 PATH。
pub fn path()->Option<PathBuf>{
    let local=std::env::var_os("LOCALAPPDATA").map(|d|PathBuf::from(d).join("agy").join("bin").join("agy.exe")).filter(|p|p.is_file());
    local.or_else(||std::env::var_os("PATH").and_then(|paths|std::env::split_paths(&paths).map(|d|d.join(if cfg!(windows){"agy.exe"}else{"agy"})).find(|p|p.is_file())))
}

pub(crate) fn add(config:&mut Config,source:&Path,alias:&str)->Result<(),String>{
    if !source.is_file(){return Err("未找到 Antigravity CLI（agy），請先安裝並登入".into());}
    let source=std::fs::canonicalize(source).unwrap_or_else(|_|source.to_owned());
    if let Some(a)=config.accounts.iter_mut().find(|a|a.provider==Provider::Antigravity){a.source=source;a.enabled=true;if !alias.trim().is_empty(){a.alias=alias.trim().into();}return Ok(());}
    let id=format!("Antigravity-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let pinned=config.pinned_indices().len()<3;if config.accounts.is_empty(){config.selected=id.clone();}
    config.accounts.push(Account{id,provider:Provider::Antigravity,alias:if alias.trim().is_empty(){"Antigravity".into()}else{alias.trim().into()},source,slot:None,enabled:true,pinned});
    Ok(())
}

fn label(group:&str)->String{
    let trimmed=group.trim().trim_end_matches(" Models").trim_end_matches(" models").trim();
    match trimmed{t if t.eq_ignore_ascii_case("Claude and GPT")=>"Claude / GPT".into(),""=>"模型".into(),t=>t.into()}
}

pub fn parse(v:&Value)->Result<Snapshot,String>{
    let data=&v["command"]["data"];
    if v["command"]["name"].as_str()!=Some("usage")||!data.is_object(){
        return Err(if v["status"].as_str().is_some_and(|s|s!="SUCCESS"){"Antigravity 尚未登入或無法取得用量，請在 agy 中登入"}else{"Antigravity CLI 沒有返回用量資料，請更新 agy"}.into());
    }
    let mut groups=Vec::new();
    for group in data["groups"].as_array().into_iter().flatten(){
        let name=label(group["name"].as_str().unwrap_or_default());let mut windows=Vec::new();
        for bucket in group["buckets"].as_array().into_iter().flatten(){
            let Some(fraction)=bucket["remaining_fraction"].as_f64().filter(|f|f.is_finite()) else{continue;};
            let seconds=match bucket["window"].as_str(){Some("5h")=>Some(18000),Some("weekly")=>Some(604800),Some("daily")=>Some(86400),_=>None};
            let reset=bucket["reset_time"].as_str().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d|d.timestamp()).filter(|t|*t>0);
            windows.push(Window{label:format!("{name} · {}",super::cadence(seconds)),seconds,remaining:(fraction*100.0).clamp(0.0,100.0) as f32,reset,amount:None});
        }
        windows.sort_by_key(|w|w.seconds.unwrap_or(i64::MAX));
        if !windows.is_empty(){groups.push(windows);}
    }
    // 主環顯示目前最吃緊的模型組；同樣吃緊時保持 agy 的原始順序。其餘模型組放在明細。
    let tightest=groups.iter().enumerate().min_by(|(ia,a),(ib,b)|{
        let low=|w:&Vec<Window>|w.iter().map(|w|w.remaining).fold(f32::MAX,f32::min);
        low(a).partial_cmp(&low(b)).unwrap_or(std::cmp::Ordering::Equal).then(ia.cmp(ib))
    }).map(|(i,_)|i).ok_or("Antigravity 未返回任何模型組用量")?;
    let windows=groups.remove(tightest);
    Ok(Snapshot{kiro:None,relay:None,credits:None,wallet:None,plan:String::new(),windows,extra:groups.into_iter().flatten().collect(),at:chrono::Utc::now().timestamp()})
}

fn run(agy:&Path)->Result<Vec<u8>,(String,u64)>{
    let mut command=Command::new(agy);
    command.args(["-p","/usage","--output-format","json","--print-timeout","45s"]).current_dir(std::env::temp_dir()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
    let mut child=command.spawn().map_err(|_|("無法啟動 Antigravity CLI（agy）".to_owned(),60))?;
    let mut stdout=child.stdout.take().ok_or(("無法讀取 Antigravity CLI 輸出".to_owned(),60))?;
    let reader=std::thread::spawn(move||{let mut out=Vec::new();let _=(&mut stdout).take(MAX_OUTPUT+1).read_to_end(&mut out);out});
    let start=Instant::now();
    loop{
        match child.try_wait(){
            Ok(Some(_))=>break,
            Ok(None) if start.elapsed()<TIMEOUT=>std::thread::sleep(Duration::from_millis(100)),
            _=>{let _=child.kill();let _=child.wait();return Err(("Antigravity CLI 回應逾時，稍後自動重試".into(),120));}
        }
    }
    let out=reader.join().map_err(|_|("讀取 Antigravity CLI 輸出失敗".to_owned(),60))?;
    if out.len() as u64>MAX_OUTPUT{return Err(("Antigravity CLI 輸出過大".into(),300));}
    Ok(out)
}

pub fn fetch(account:&Account)->Result<Snapshot,(String,u64)>{
    let agy=if account.source.is_file(){account.source.clone()}else{path().ok_or(("未找到 Antigravity CLI（agy），請先安裝並登入".to_owned(),300))?};
    let out=run(&agy)?;
    let text=String::from_utf8_lossy(&out);let text=text.trim_start_matches('\u{feff}').trim();
    let v:Value=serde_json::from_str(text).map_err(|_|("Antigravity CLI 沒有返回有效的 JSON，請更新 agy".to_owned(),300))?;
    // 保險：斜線指令若被當成一般提示詞送出就會消耗額度，此時拒絕並拉長重試間隔。
    if v["num_turns"].as_i64().is_some_and(|n|n>0){return Err(("agy 未把 /usage 當成指令處理，已暫停查詢，請更新 agy".into(),3600));}
    parse(&v).map_err(|e|(e,120))
}

#[cfg(test)] mod tests {
    use super::*;use serde_json::json;
    fn usage(gemini:f64,third:f64,third_reset:&str)->Value{json!({"conversation_id":"","status":"SUCCESS","num_turns":0,"command":{"name":"usage","data":{"groups":[
        {"name":"Gemini Models","buckets":[{"id":"gemini-weekly","window":"weekly","remaining_fraction":gemini,"reset_time":"2026-12-04T11:43:48Z"},{"id":"gemini-5h","window":"5h","remaining_fraction":0.9899,"reset_time":"2026-11-27T16:43:48Z"}]},
        {"name":"Claude and GPT models","buckets":[{"id":"3p-weekly","window":"weekly","remaining_fraction":third,"reset_time":third_reset},{"id":"3p-5h","window":"5h","remaining_fraction":1,"reset_time":third_reset}]}]}}})}
    #[test] fn antigravity_groups_map_to_rings_and_details(){
        let s=parse(&usage(0.98998,1.0,"1970-01-01T00:00:00Z")).unwrap();
        assert_eq!(s.rings().len(),2);assert_eq!(s.windows[0].label,"Gemini · 5 小時");assert_eq!(s.windows[1].label,"Gemini · 本週");
        assert_eq!(s.windows[0].seconds,Some(18000));assert_eq!(s.windows[1].reset,Some(1796384628));assert!((s.windows[1].remaining-98.998).abs()<0.01);
        assert_eq!(s.extra.len(),2);assert_eq!(s.extra[0].label,"Claude / GPT · 5 小時");assert!(s.extra.iter().all(|w|w.reset.is_none()),"未開始的視窗沒有重置時間");
    }
    #[test] fn antigravity_tightest_group_takes_the_rings(){
        let s=parse(&usage(0.99,0.4,"2026-11-28T00:00:00Z")).unwrap();
        assert!(s.windows.iter().all(|w|w.label.starts_with("Claude / GPT")));assert_eq!(s.windows[1].remaining,40.0);assert!(s.extra.iter().all(|w|w.label.starts_with("Gemini")));
    }
    #[test] fn antigravity_rejects_non_command_or_empty_output(){
        assert!(parse(&json!({"status":"SUCCESS","response":"hello","num_turns":1})).is_err());
        assert!(parse(&json!({"status":"ERROR"})).unwrap_err().contains("登入"));
        assert!(parse(&json!({"command":{"name":"usage","data":{"groups":[]}}})).is_err());
        assert!(parse(&json!({"command":{"name":"usage","data":{"groups":[{"name":"X","buckets":[{"window":"5h","remaining_fraction":"bad"}]}]}}})).is_err());
    }
    #[test] fn antigravity_add_is_single_and_idempotent(){
        let path=std::env::temp_dir().join(format!("daozai-agy-{}.exe",std::process::id()));std::fs::write(&path,b"x").unwrap();
        let mut c=Config::default();add(&mut c,&path,"").unwrap();add(&mut c,&path,"個人").unwrap();std::fs::remove_file(&path).unwrap();
        assert_eq!(c.accounts.len(),1);assert_eq!(c.accounts[0].alias,"個人");assert_eq!(c.selected,c.accounts[0].id);
        assert!(add(&mut c,Path::new("missing-agy.exe"),"").is_err());
    }
    #[test]
    #[ignore="按需呼叫本機 agy 驗證真實用量輸出"]
    fn antigravity_live_usage(){
        let agy=path().expect("本機應已安裝 agy");let account=Account{id:"live".into(),provider:Provider::Antigravity,alias:"live".into(),source:agy,slot:None,enabled:true,pinned:true};
        let s=fetch(&account).unwrap();println!("{:?}",s.windows.iter().chain(&s.extra).map(|w|(&w.label,w.remaining,w.reset)).collect::<Vec<_>>());
    }
}
