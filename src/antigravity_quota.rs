//! Antigravity 用量：呼叫官方 `agy -p /usage --output-format json` 取得各模型組的 5 小時與每週額度。
//! 斜線指令由 CLI 本機處理，不送出對話、不消耗額度；萬一被當成對話送出，會停止查詢直到 agy 更新或使用者重新連線；登入與續期完全交給 agy，島仔不讀取任何令牌。
use super::{Account,Config,Provider,Snapshot,Window};
use serde_json::Value;
use std::{io::Read,path::{Path,PathBuf},process::{Command,Stdio},sync::mpsc,time::{Duration,Instant}};

const TIMEOUT:Duration=Duration::from_secs(60);
const MAX_OUTPUT:u64=1024*1024;
/// 逾時通常代表網路卡住，但也可能是 /usage 被當成對話送出；拉長重試間隔，避免每輪都可能消耗額度。
const TIMEOUT_WAIT:u64=1800;
const TRIPPED:&str="agy 把 /usage 當成一般對話處理，已停止查詢以免消耗額度；請更新 agy 後重新連線";

/// 本機 agy 執行檔：優先官方安裝位置，其次 PATH。
pub fn path()->Option<PathBuf>{
    let local=std::env::var_os("LOCALAPPDATA").map(|d|PathBuf::from(d).join("agy").join("bin").join("agy.exe")).filter(|p|p.is_file());
    local.or_else(||std::env::var_os("PATH").and_then(|paths|std::env::split_paths(&paths).map(|d|d.join(if cfg!(windows){"agy.exe"}else{"agy"})).find(|p|p.is_file())))
}

pub(crate) fn add(config:&mut Config,source:&Path,alias:&str)->Result<(),String>{
    if !source.is_file(){return Err("未找到 Antigravity CLI（agy），請先安裝並登入".into());}
    let source=std::fs::canonicalize(source).unwrap_or_else(|_|source.to_owned());
    // 使用者重新連線代表要再試一次：解除先前的誤耗額度保護。
    clear_guard(&guard_path());
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
    if v["status"].as_str().is_some_and(|s|s!="SUCCESS"){return Err("Antigravity 尚未登入或無法取得用量，請在 agy 中登入".into());}
    let data=&v["command"]["data"];
    if v["command"]["name"].as_str()!=Some("usage")||!data.is_object(){return Err("Antigravity CLI 沒有返回用量資料，請更新 agy".into());}
    let mut groups=Vec::new();
    for group in data["groups"].as_array().into_iter().flatten(){
        let name=label(group["name"].as_str().unwrap_or_default());let mut windows=Vec::new();
        for bucket in group["buckets"].as_array().into_iter().flatten(){
            let Some(fraction)=bucket["remaining_fraction"].as_f64().filter(|f|f.is_finite()&&*f>=0.0) else{continue;};
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
    // 讀取放在獨立執行緒並經 channel 回傳：agy 的子程序若繼承 stdout 不結束，也不會卡死查詢執行緒。
    let(tx,rx)=mpsc::channel();
    std::thread::spawn(move||{let mut out=Vec::new();let _=(&mut stdout).take(MAX_OUTPUT+1).read_to_end(&mut out);let _=tx.send(out);});
    let start=Instant::now();
    loop{
        match child.try_wait(){
            Ok(Some(_))=>break,
            Ok(None) if start.elapsed()<TIMEOUT=>std::thread::sleep(Duration::from_millis(100)),
            _=>{kill_tree(&mut child);return Err(("Antigravity CLI 回應逾時，稍後自動重試".into(),TIMEOUT_WAIT));}
        }
    }
    let out=rx.recv_timeout(Duration::from_secs(5)).map_err(|_|("讀取 Antigravity CLI 輸出逾時".to_owned(),TIMEOUT_WAIT))?;
    if out.len() as u64>MAX_OUTPUT{return Err(("Antigravity CLI 輸出過大".into(),300));}
    Ok(out)
}

fn kill_tree(child:&mut std::process::Child){
    #[cfg(windows)]{
        use std::os::windows::process::CommandExt;
        let _=Command::new("taskkill.exe").args(["/PID",&child.id().to_string(),"/T","/F"]).creation_flags(0x08000000).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _=child.kill();let _=child.wait();
}

/// 回應是否顯示 /usage 被當成一般對話送出（會消耗額度）：有回合、有 token、建立了對話，或成功但不是 usage 指令。
fn consumed(v:&Value)->bool{
    v["num_turns"].as_i64().is_some_and(|n|n>0)
        ||v["usage"]["total_tokens"].as_i64().is_some_and(|n|n>0)
        ||v["conversation_id"].as_str().is_some_and(|id|!id.trim().is_empty())
        ||(v["status"].as_str()==Some("SUCCESS")&&v["command"]["name"].as_str()!=Some("usage"))
}

/// 解析 agy 輸出；`Err((_,None))` 代表必須啟動誤耗額度保護。
fn decode(out:&[u8])->Result<Snapshot,(String,Option<u64>)>{
    let text=String::from_utf8_lossy(out);let text=text.trim_start_matches('\u{feff}').trim();
    let v:Value=serde_json::from_str(text).map_err(|_|("Antigravity CLI 沒有返回有效的 JSON，請更新 agy".to_owned(),Some(300)))?;
    if consumed(&v){return Err((TRIPPED.into(),None));}
    parse(&v).map_err(|e|(e,Some(120)))
}

fn guard_path()->PathBuf{crate::model::config_dir().join("antigravity-guard.json")}
fn stamp(agy:&Path)->Option<String>{
    let modified=std::fs::metadata(agy).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}:{}",modified.as_secs(),modified.subsec_nanos()))
}
/// 保護仍有效時返回提示：同一個 agy 且執行檔沒有更新過。agy 更新後自動解除；記在檔案裡，重啟島仔也不會再次觸發。
fn guarded(guard:&Path,agy:&Path)->Option<String>{
    let v:Value=serde_json::from_slice(&std::fs::read(guard).ok()?).ok()?;
    if v["agy"].as_str()==agy.to_str()&&v["stamp"].as_str()==stamp(agy).as_deref(){return Some(TRIPPED.into());}
    clear_guard(guard);None
}
fn trip_guard(guard:&Path,agy:&Path){
    if let Some(parent)=guard.parent(){let _=std::fs::create_dir_all(parent);}
    let _=std::fs::write(guard,serde_json::json!({"agy":agy.to_str(),"stamp":stamp(agy)}).to_string());
}
fn clear_guard(guard:&Path){let _=std::fs::remove_file(guard);}

pub fn fetch(account:&Account)->Result<Snapshot,(String,u64)>{
    let agy=if account.source.is_file(){account.source.clone()}else{path().ok_or(("未找到 Antigravity CLI（agy），請先安裝並登入".to_owned(),300))?};
    let guard=guard_path();
    // 保護期間不再啟動 agy；回傳 0 讓服務不另設冷卻，agy 一更新下一輪就能恢復。
    if let Some(message)=guarded(&guard,&agy){return Err((message,0));}
    match decode(&run(&agy)?){
        Ok(snapshot)=>Ok(snapshot),
        Err((message,Some(wait)))=>Err((message,wait)),
        Err((message,None))=>{trip_guard(&guard,&agy);Err((message,0))}
    }
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
        assert!(parse(&json!({"status":"ERROR","command":{"name":"usage","data":{}}})).unwrap_err().contains("登入"),"失敗狀態優先提示登入");
        let mut negative=usage(0.5,1.0,"2026-11-28T00:00:00Z");negative["command"]["data"]["groups"][0]["buckets"][0]["remaining_fraction"]=json!(-1);
        let s=parse(&negative).unwrap();assert!(s.windows.iter().chain(s.extra.iter()).all(|w|w.remaining>0.0),"負值代表未知，不能顯示成 0%");
        assert!(parse(&json!({"status":"ERROR"})).unwrap_err().contains("登入"));
        assert!(parse(&json!({"command":{"name":"usage","data":{"groups":[]}}})).is_err());
        assert!(parse(&json!({"command":{"name":"usage","data":{"groups":[{"name":"X","buckets":[{"window":"5h","remaining_fraction":"bad"}]}]}}})).is_err());
    }
    #[test] fn antigravity_usage_sent_as_chat_trips_the_guard(){
        let ok=serde_json::to_vec(&usage(0.9,1.0,"2026-11-28T00:00:00Z")).unwrap();assert!(decode(&ok).is_ok());
        for chat in [json!({"status":"SUCCESS","num_turns":1,"conversation_id":"","response":"hi"}),
            json!({"status":"SUCCESS","num_turns":0,"conversation_id":"","usage":{"total_tokens":42},"command":{"name":"usage","data":{"groups":[]}}}),
            json!({"status":"SUCCESS","num_turns":0,"conversation_id":"abc","response":"hi"}),
            json!({"status":"SUCCESS","response":"usage path does not exist"})]{
            assert!(matches!(decode(&serde_json::to_vec(&chat).unwrap()),Err((_,None))),"應啟動保護：{chat}");
        }
        assert!(matches!(decode(b"not json"),Err((_,Some(_)))),"非 JSON 只是暫時錯誤");
    }
    #[test] fn antigravity_guard_survives_restart_and_clears_after_agy_update(){
        let dir=std::env::temp_dir().join(format!("daozai-agy-guard-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
        let agy=dir.join("agy.exe");let guard=dir.join("guard.json");std::fs::write(&agy,b"v1").unwrap();
        assert!(guarded(&guard,&agy).is_none());trip_guard(&guard,&agy);
        assert!(guarded(&guard,&agy).is_some(),"同一版 agy 應持續停止查詢；記在檔案裡，重啟後也一樣");
        let file=std::fs::File::options().write(true).open(&agy).unwrap();file.set_modified(std::time::SystemTime::now()+Duration::from_secs(5)).unwrap();drop(file);
        assert!(guarded(&guard,&agy).is_none(),"agy 更新後自動解除");assert!(!guard.exists());
        trip_guard(&guard,&agy);clear_guard(&guard);assert!(guarded(&guard,&agy).is_none(),"重新連線可手動解除");
        std::fs::remove_dir_all(&dir).unwrap();
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
