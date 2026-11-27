//! 只讀 Claude 本地用量；不解密登入令牌，不請求遠端介面。
use super::{Account,Snapshot,Window};
use ring::digest;
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf}};

#[path="claude_usage_cache.rs"]
mod desktop_cache;

const HISTORY:&str="plan-usage-history.json";
const CODE_USAGE:&str="daozai-usage.json";
const COLLECTOR:&str="daozai-statusline.ps1";
pub(super) struct Profile {pub slot:String,pub label:String}

pub(super) fn paths(roaming:&Path,local:&Path)->Vec<PathBuf>{
    let mut paths=vec![roaming.join("Claude/config.json"),local.join("Claude/config.json")];
    if let Ok(entries)=fs::read_dir(local.join("Claude-Data")){
        for entry in entries.flatten(){if entry.file_name().to_string_lossy().to_ascii_lowercase().starts_with("claude_"){paths.push(entry.path().join("config.json"));}}
    }
    if let Ok(entries)=fs::read_dir(local.join("Packages")){
        for entry in entries.flatten(){if entry.file_name().to_string_lossy().to_ascii_lowercase().starts_with("claude_"){paths.push(entry.path().join("LocalCache/Roaming/Claude/config.json"));}}
    }
    paths.retain(|p|p.is_file());paths.sort();paths.dedup();paths
}
pub(super) fn is_desktop(path:&Path)->bool{matches!(path.file_name().and_then(|s|s.to_str()),Some("config.json"|HISTORY))}
pub(super) fn local_source(path:&Path)->PathBuf{
    if path.file_name().is_some_and(|s|s==".credentials.json"||s=="settings.json"){path.with_file_name(CODE_USAGE)}else{path.to_owned()}
}
fn read_json(path:&Path)->Result<Value,String>{
    let raw=fs::read(path).map_err(|_|"尚無 Claude 本地用量記錄，請在官方客戶端使用後重新整理".to_string())?;
    serde_json::from_slice(raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw)).map_err(|_|"Claude 本地用量記錄暫不可讀，等待客戶端更新".into())
}
fn slot(owner:&str,org:&str)->String{digest::digest(&digest::SHA256,format!("{owner}|{org}").as_bytes()).as_ref().iter().map(|b|format!("{b:02x}")).collect()}
fn desktop_identity(path:&Path)->Result<(String,i64),String>{
    // 只反序列化帳號標識，忽略同檔案中的加密登入快取。
    #[derive(serde::Deserialize)]
    struct Identity {#[serde(rename="lastKnownAccountUuid")] owner:String}
    use std::io::Read;
    let mut file=fs::File::open(path.with_file_name("config.json")).map_err(|_|"無法確認 Claude 桌面版本地帳號")?;
    let stamp=file.metadata().and_then(|m|m.modified()).map_err(|_|"無法確認 Claude 帳號配置的更新時間")?;
    let mut raw=Vec::new();file.read_to_end(&mut raw).map_err(|_|"無法讀取 Claude 帳號配置")?;
    let identity:Identity=serde_json::from_slice(raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw)).map_err(|_|"請先在 Claude 桌面版登入".to_string())?;
    if identity.owner.is_empty(){return Err("請先在 Claude 桌面版登入".into());}Ok((identity.owner,chrono::DateTime::<chrono::Utc>::from(stamp).timestamp_millis()))
}
fn code_identity(path:&Path)->Result<(String,String),String>{
    let dir=path.parent().ok_or("未找到 Claude Code 本地目錄")?;
    let identity=if dir.join(".claude.json").is_file(){dir.join(".claude.json")}else if dir.file_name().is_some_and(|n|n==".claude"){PathBuf::from(format!("{}.json",dir.to_string_lossy()))}else{dir.join(".claude.json")};
    #[derive(serde::Deserialize)]
    struct Identity {#[serde(rename="oauthAccount")] account:IdentityAccount}
    #[derive(serde::Deserialize)]
    struct IdentityAccount {#[serde(rename="accountUuid")] owner:String,#[serde(rename="organizationUuid")] org:String}
    let raw=fs::read(identity).map_err(|_|"請先在 Claude Code 登入訂閱帳號".to_string())?;
    let value:Identity=serde_json::from_slice(raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw)).map_err(|_|"無法確認 Claude Code 本地帳號".to_string())?;
    if value.account.owner.is_empty()||value.account.org.is_empty(){return Err("無法確認 Claude Code 本地帳號".into());}
    Ok((value.account.owner,value.account.org))
}
fn desktop_record(path:&Path,account:Option<&Account>,verified_cache:&Path)->Result<(String,Snapshot),String>{
    let (owner,login_stamp)=desktop_identity(path)?;let data=read_json(&path.with_file_name(HISTORY))?;
    if data["version"].as_u64()!=Some(2){return Err("本地記錄缺少帳號歸屬，請更新 Claude 桌面版後重試".into());}
    let now=chrono::Utc::now().timestamp_millis();
    let sample=data["samples"].as_array().into_iter().flatten().filter(|s|s["t"].as_i64().is_some_and(|t|t>0&&t<=now)).max_by_key(|s|s["t"].as_i64()).ok_or("請在 Claude 桌面版開啟用量頁面，等待當前帳號產生新記錄")?;
    let org=sample["org"].as_str().filter(|s|!s.is_empty()).ok_or("本地用量記錄缺少帳號歸屬，等待 Claude 更新")?;
    let account_slot=slot(&owner,org);
    if account.is_some_and(|a|a.slot.as_deref()!=Some(account_slot.as_str())){return Err("Claude 本地帳號已變化，請重新連線".into());}
    let mut snapshot=Snapshot{at:sample["t"].as_i64().unwrap()/1000,..Default::default()};
    for (key,label,seconds) in [("fh","5 小時",18000),("sd","本週",604800)]{
        if let Some(used)=sample["u"][key].as_f64().filter(|n|n.is_finite()&&(0.0..=100.0).contains(n)){
            snapshot.windows.push(Window{label:label.into(),seconds:Some(seconds),remaining:(100.0-used) as f32,reset:None,amount:None});
        }
    }
    // 歷史和瀏覽器快取均按組織儲存。帳號配置較新時，只能回顯島仔已為該帳號核驗過的快照。
    if let Some(cached)=desktop_cache::read(&path.with_file_name("Cache").join("Cache_Data"),org,snapshot.at,now/1000)
        .filter(|s|s.at*1000>login_stamp){
        snapshot=cached;
    }else if sample["t"].as_i64().unwrap()<=login_stamp{
        snapshot=account.and_then(|a|{
            let raw=fs::read(verified_cache).ok()?;
            a.cached_snapshot(&serde_json::from_slice(&raw).ok()?)
        }).ok_or("請在 Claude 桌面版開啟用量頁面，等待當前帳號產生新記錄")?;
    }
    if snapshot.windows.is_empty(){return Err("Claude 本地記錄暫無可用額度".into());}
    if desktop_identity(path)?!=(owner,login_stamp){return Err("Claude 帳號配置已變化，請重新整理重試".into());}
    Ok((account_slot,snapshot))
}
fn code_slot(path:&Path)->Result<String,String>{let(owner,org)=code_identity(path)?;Ok(format!("code-{}",slot(&owner,&org)))}
pub(super) fn profiles(path:&Path)->Result<Vec<Profile>,String>{
    let path=local_source(path);
    let (slot,label)=if is_desktop(&path){(desktop_record(&path,None,&crate::model::config_dir().join("ai-quota-cache.json"))?.0,"Claude · 桌面版本地記錄")}else{(code_slot(&path)?,"Claude · Code 本地記錄")};
    Ok(vec![Profile{slot,label:label.into()}])
}
pub(super) fn fetch(account:&Account)->Result<Snapshot,String>{
    let path=local_source(&account.source);
    let (slot,mut snapshot)=if is_desktop(&path){desktop_record(&path,Some(account),&crate::model::config_dir().join("ai-quota-cache.json"))?}else{
        let slot=code_slot(&path)?;
        // 沒有記錄時，先看是不是狀態列被其他工具改回去了，免得使用者以為是 Claude Code 沒送資料。
        let data=read_json(&path).map_err(|error|if collector_connected(&path){error}else{COLLECTOR_LOST.into()})?;
        if data["version"].as_u64()!=Some(1){return Err("Claude Code 本地記錄格式不匹配，請啟用本地採集".into());}
        let cache_slot=format!("code-{}",self::slot(data["account"].as_str().unwrap_or_default(),data["organization"].as_str().unwrap_or_default()));
        if slot!=cache_slot{return Err("Claude Code 帳號已變化，等待新帳號的本地記錄".into());}
        let at=data["at"].as_i64().filter(|at|*at>0&&*at<=chrono::Utc::now().timestamp()+60).ok_or("Claude 本地記錄時間無效")?;
        let mut snapshot=Snapshot{at,..Default::default()};
        for (key,label,seconds) in [("five_hour","5 小時",18000),("seven_day","本週",604800)]{
            let window=&data["rate_limits"][key];
            if let Some(used)=window["used_percentage"].as_f64().filter(|n|n.is_finite()&&(0.0..=100.0).contains(n)){
                let reset=window["resets_at"].as_i64().filter(|t|*t>0);
                if reset.is_some_and(|t|t<=chrono::Utc::now().timestamp()){continue;}
                snapshot.windows.push(Window{label:label.into(),seconds:Some(seconds),remaining:(100.0-used) as f32,reset,amount:None});
            }
        }
        if snapshot.windows.is_empty(){return Err(if collector_connected(&path){"尚無有效的 Claude Code 本地用量，等待下一次使用".into()}else{COLLECTOR_LOST.into()});}
        (slot,snapshot)
    };
    if account.slot.as_deref()!=Some(slot.as_str()){return Err("Claude 本地帳號已變化，請重新連線".into());}
    if is_desktop(&path){
        // 桌面版記錄沒有重置時間：5 小時視窗過了 5 小時、本週視窗過了 7 天，舊數字早已不成立，
        // 不能當成現在的額度；全部過期時說明原因，不再顯示舊數字。
        snapshot.drop_expired_windows(chrono::Utc::now().timestamp());
        if snapshot.windows.is_empty(){
            let at=chrono::DateTime::from_timestamp(snapshot.at,0).map(|t|t.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string()).unwrap_or_default();
            return Err(format!("本地記錄停在 {at}，已超過用量視窗長度；請開啟 Claude 桌面版的用量頁面，等待它產生新記錄"));
        }
    }
    snapshot.windows.sort_by_key(|w|w.seconds);Ok(snapshot)
}
pub(super) fn code_path()->PathBuf{
    std::env::var_os("CLAUDE_CONFIG_DIR").filter(|s|!s.is_empty()).map(PathBuf::from).unwrap_or_else(||std::env::var_os("USERPROFILE").or_else(||std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default().join(".claude")).join(CODE_USAGE)
}
/// Claude Code 的狀態列指令只有一格，常被其他工具佔用。島仔不覆蓋它，而是接在它前面：
/// `島仔收集腳本 -Tee | 原指令`。收集腳本把輸入逐字節交給原指令，自己不輸出，所以原指令的輸入和輸出都不變。
/// `|` 在 cmd、bash、PowerShell 下都是管道，不必猜 Claude Code 用哪個 shell 執行這條指令。
const ORIGINAL_RECORD:&str="daozai-statusline-original.txt";
const COLLECTOR_LOST:&str="Claude Code 的狀態列沒有接上島仔的收集腳本（可能被其他工具改回）；請在這裡重新按「啟用 Code 本地採集」";
fn collector_command(script:&Path)->String{
    format!("powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File \"{}\"",script.to_string_lossy().replace('\\',"/"))
}
/// 狀態列是否還接著島仔的收集腳本；讀不到設定時不下結論，免得錯怪使用者的環境。
fn collector_connected(usage:&Path)->bool{
    let Ok(settings)=read_json(&usage.with_file_name("settings.json")) else{return true;};
    settings["statusLine"]["command"].as_str().is_some_and(|command|command.contains(COLLECTOR))
}
/// 只把設定檔裡原指令那一段文字換掉，保留使用者設定檔的排版與鍵順序；對不上時才整份改寫。
fn write_status_line_command(settings_path:&Path,settings:&mut Value,old:Option<&str>,new:&str)->Result<(),String>{
    if let Some(old)=old {
        if let (Ok(raw),Ok(old_json),Ok(new_json))=(fs::read_to_string(settings_path),serde_json::to_string(old),serde_json::to_string(new)) {
            if raw.matches(&old_json).count()==1 {
                let edited=raw.replacen(&old_json,&new_json,1);
                if serde_json::from_slice::<Value>(edited.trim_start_matches('\u{feff}').as_bytes()).is_ok() {
                    return crate::model::atomic_write(settings_path,edited.as_bytes()).map_err(|_|"無法更新 Claude Code 狀態列配置".to_string());
                }
            }
        }
    }
    if settings["statusLine"].is_null(){settings["statusLine"]=json!({"type":"command","command":new});}else{settings["statusLine"]["command"]=json!(new);}
    crate::model::atomic_write(settings_path,&serde_json::to_vec_pretty(settings).map_err(|_|"無法儲存 Claude Code 設定")?).map_err(|_|"無法更新 Claude Code 狀態列配置".into())
}
/// 啟用 Code 本地採集。返回 `true` 表示 Claude Code 原本已有自己的狀態列，已接在它前面（而不是覆蓋）。
pub(super) fn install_code_collector(path:&Path)->Result<bool,String>{
    code_identity(path)?;let dir=path.parent().ok_or("未找到 Claude Code 目錄")?;let settings_path=dir.join("settings.json");
    let mut settings=if settings_path.is_file(){read_json(&settings_path)?}else{json!({})};
    if !settings.is_object(){return Err("Claude Code 的 settings.json 頂層必須是 JSON 物件，請修正後重試".into());}
    let script=dir.join(COLLECTOR);let command=collector_command(&script);let chain=format!("{command} -Tee |");
    let current=settings["statusLine"].clone();
    // 先決定要寫什麼，再動檔案；任何一步判斷不了就原樣保留。
    let mut chained=false;
    let existing=current["command"].as_str().map(str::to_owned);
    let new_command=if current.is_null(){Some(command.clone())}else{
        let existing=existing.as_deref().ok_or("Claude Code 的狀態列不是指令類型，無法轉接；已保留原設定")?;
        if existing.contains(COLLECTOR){chained=existing.starts_with(&chain);None}
        else if existing.trim().is_empty(){Some(command.clone())}
        else{chained=true;Some(format!("{chain} {}",existing.trim()))}
    };
    let script_content=format!("\u{feff}{}",include_str!("../scripts/claude-statusline.ps1"));
    crate::model::atomic_write(&script,script_content.as_bytes()).map_err(|_|"無法儲存 Claude 本地採集指令碼".to_string())?;
    let Some(new_command)=new_command else{return Ok(chained);};
    if chained {
        // 留一份原指令，萬一要取消轉接也找得回來。
        crate::model::atomic_write(&dir.join(ORIGINAL_RECORD),existing.as_deref().unwrap_or_default().as_bytes()).map_err(|_|"無法備份原有的狀態列指令，未做任何轉接".to_string())?;
    }
    write_status_line_command(&settings_path,&mut settings,existing.as_deref(),&new_command)?;
    Ok(chained)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore="按需驗證已開啟的 Claude 桌面版本地記錄，不傳送網路請求"]
    fn local_claude_desktop_probe(){
        let roaming=std::env::var_os("APPDATA").map(PathBuf::from).unwrap();let local=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap();let mut count=0;
        for path in paths(&roaming,&local){
            let Ok(profiles)=profiles(&path)else{continue;};let before=fs::read(&path).unwrap();let history=fs::read(path.with_file_name(HISTORY)).unwrap();
            for profile in profiles{let a=account(path.clone(),profile.slot);let start=std::time::Instant::now();let snapshot=super::super::fetch(&ureq::Agent::new(),&a).unwrap();println!("desktop_local_windows={:?};record_at={};elapsed_ms={}",snapshot.windows.iter().map(|w|(&w.label,w.remaining,w.reset)).collect::<Vec<_>>(),snapshot.at,start.elapsed().as_millis());count+=1;}
            assert_eq!(fs::read(&path).unwrap(),before);assert_eq!(fs::read(path.with_file_name(HISTORY)).unwrap(),history);
        }
        assert!(count>0,"請先開啟 Claude 桌面版的用量介面");
        let config=super::super::Config::load();for a in config.accounts.iter().filter(|a|a.provider==super::super::Provider::Claude&&is_desktop(&a.source)){assert!(fetch(a).is_ok(),"已有桌面版帳號應能直接使用本地資料");}
    }
    #[test]
    #[ignore="只讀驗證已繫結的桌面版歷史，不開啟客戶端或傳送網路請求"]
    fn local_claude_closed_desktop_probe(){
        let config=super::super::Config::load();let mut count=0;
        for account in config.accounts.iter().filter(|a|a.provider==super::super::Provider::Claude&&is_desktop(&a.source)){
            let identity=fs::read(&account.source).unwrap();let history_path=account.source.with_file_name(HISTORY);let history=fs::read(&history_path).unwrap();
            let snapshot=fetch(account).expect("已繫結帳號關閉客戶端後仍應讀到本地歷史");println!("windows={:?};record_at={}",snapshot.windows.iter().map(|w|(&w.label,w.remaining,w.reset)).collect::<Vec<_>>(),snapshot.at);
            assert_eq!(fs::read(&account.source).unwrap(),identity);assert_eq!(fs::read(history_path).unwrap(),history);count+=1;
        }assert!(count>0,"尚無已繫結的 Claude 桌面版帳號");
    }
    fn dir()->PathBuf{let p=std::env::temp_dir().join(format!("daozai-local-claude-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&p).unwrap();p}
    fn earlier_login(path:&Path){
        let stamp=std::time::SystemTime::now()-std::time::Duration::from_secs(300);
        fs::File::options().write(true).open(path).unwrap().set_modified(stamp).unwrap();
    }
    fn account(path:PathBuf,slot:String)->Account{Account{id:"local-test".into(),provider:super::super::Provider::Claude,alias:"本地測試".into(),source:path,slot:Some(slot),enabled:true,pinned:false}}
    fn code_fixture(dir:&Path)->PathBuf{fs::write(dir.join(".claude.json"),json!({"oauthAccount":{"accountUuid":"code-user","organizationUuid":"code-org"}}).to_string()).unwrap();dir.join(CODE_USAGE)}
    #[test]
    fn desktop_windows_older_than_their_own_length_are_not_shown_as_current(){
        let dir=dir();let path=dir.join("config.json");let now=chrono::Utc::now().timestamp();
        fs::write(&path,json!({"lastKnownAccountUuid":"owner"}).to_string()).unwrap();
        fs::File::options().write(true).open(&path).unwrap().set_modified(std::time::SystemTime::now()-std::time::Duration::from_secs(20*3600)).unwrap();
        let write=|used:Value|fs::write(dir.join(HISTORY),json!({"version":2,"samples":[{"t":(now-18*3600)*1000,"org":"local-org","u":used}]}).to_string()).unwrap();
        write(json!({"fh":26,"sd":10}));
        let saved=account(path.clone(),profiles(&path).unwrap().remove(0).slot);
        let snapshot=fetch(&saved).unwrap();
        assert_eq!(snapshot.windows.iter().map(|w|w.label.as_str()).collect::<Vec<_>>(),["本週"],"18 小時前的 5 小時視窗早已重置，不能再顯示；一週視窗仍然有效");
        write(json!({"fh":26}));
        let error=fetch(&saved).unwrap_err();
        assert!(error.contains("已超過用量視窗長度")&&error.contains("桌面版"),"{error}");
        let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn local_desktop_exit_keeps_history_for_the_previously_bound_identity(){
        let dir=dir();let path=dir.join("config.json");let now=chrono::Utc::now().timestamp();
        fs::write(&path,json!({"lastKnownAccountUuid":"owner","windowBounds":{"width":900}}).to_string()).unwrap();earlier_login(&path);
        let history=json!({"version":2,"samples":[{"t":(now-60)*1000,"org":"local-org","u":{"fh":24,"sd":84}}]}).to_string();fs::write(dir.join(HISTORY),&history).unwrap();
        let saved=account(path.clone(),profiles(&path).unwrap().remove(0).slot);let before=fetch(&saved).unwrap();
        let cache=dir.join("verified.json");
        fs::write(&cache,json!({&saved.id:{"provider":"Claude","slot":saved.slot,"source":saved.source,"local_identity_checked":saved.cache_identity_marker(),"snapshot":before}}).to_string()).unwrap();
        // 退出只儲存視窗狀態，帳號標識沒有變化。
        fs::write(&path,json!({"lastKnownAccountUuid":"owner","windowBounds":{"width":1200}}).to_string()).unwrap();
        let (_,after)=desktop_record(&path,Some(&saved),&cache).expect("已繫結帳號退出後仍應顯示自己的歷史記錄");assert_eq!((after.at,&after.windows),(before.at,&before.windows));assert_eq!(fs::read_to_string(dir.join(HISTORY)).unwrap(),history);
        let (_,restored)=desktop_record(&path,Some(&saved.clone()),&cache).unwrap();assert_eq!((restored.at,&restored.windows),(before.at,&before.windows),"島仔重新啟動後也應保留歷史用量");
        fs::write(&path,json!({"lastKnownAccountUuid":"different-owner"}).to_string()).unwrap();assert!(fetch(&saved).is_err(),"真正切換帳號後不能繼續讀取當前來源的歷史");let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn bound_desktop_login_does_not_adopt_a_peer_record_when_switching_back(){
        let dir=dir();let path=dir.join("config.json");let now=chrono::Utc::now().timestamp();
        let write_owner=|owner:&str,stamp:i64|{
            fs::write(&path,json!({"lastKnownAccountUuid":owner}).to_string()).unwrap();
            fs::File::options().write(true).open(&path).unwrap().set_modified(std::time::UNIX_EPOCH+std::time::Duration::from_secs(stamp as u64)).unwrap();
        };
        let sample=|at:i64,used:i32|fs::write(dir.join(HISTORY),json!({"version":2,"samples":[{"t":at*1000,"org":"shared","u":{"fh":used}}]}).to_string()).unwrap();
        let cache=dir.join("verified.json");
        write_owner("A",now-100);sample(now-90,11);
        let a=account(path.clone(),profiles(&path).unwrap().remove(0).slot);
        let first=fetch(&a).unwrap();
        fs::write(&cache,json!({&a.id:{"provider":"Claude","slot":a.slot,"source":a.source,"local_identity_checked":a.cache_identity_marker(),"snapshot":first}}).to_string()).unwrap();
        write_owner("B",now-60);sample(now-50,86);assert!(fetch(&a).is_err());
        write_owner("A",now-10);
        desktop_cache::fixture(&dir.join("Cache/Cache_Data"),"shared",now-49,br#"{"five_hour":{"utilization":87}}"#);
        let (_,result)=desktop_record(&path,Some(&a),&cache).unwrap();
        assert_eq!(result.at,first.at);assert_eq!(result.windows,first.windows);
        let mut legacy:Value=serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
        legacy[&a.id]["local_identity_checked"]=true.into();fs::write(&cache,legacy.to_string()).unwrap();
        assert!(desktop_record(&path,Some(&a),&cache).is_err());
        fs::remove_file(&cache).unwrap();assert!(desktop_record(&path,Some(&a),&cache).is_err());
        sample(now-2,35);assert_eq!(fetch(&a).unwrap().windows[0].remaining,65.0);
        let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn local_desktop_cache_resets_use_the_same_observation_and_never_request_the_network(){
        let dir=dir();let source=dir.join("config.json");let now=chrono::Utc::now().timestamp();
        let identity=json!({"lastKnownAccountUuid":"owner","oauth:tokenCacheV2":"do-not-use"}).to_string();fs::write(&source,&identity).unwrap();earlier_login(&source);
        let history=json!({"version":2,"samples":[{"t":(now-30)*1000,"org":"local-org","u":{"fh":24,"sd":80}}]}).to_string();fs::write(dir.join(HISTORY),&history).unwrap();
        let cache=dir.join("Cache/Cache_Data");let body=serde_json::to_vec(&json!({"five_hour":{"utilization":26,"resets_at":chrono::DateTime::from_timestamp(now+3600,0).unwrap().to_rfc3339()},"seven_day":{"utilization":84,"resets_at":chrono::DateTime::from_timestamp(now+86400,0).unwrap().to_rfc3339()}})).unwrap();
        desktop_cache::fixture(&cache,"local-org",now-10,&zstd::stream::encode_all(body.as_slice(),1).unwrap());
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();let agent=ureq::AgentBuilder::new().proxy(ureq::Proxy::new(format!("http://{}",listener.local_addr().unwrap())).unwrap()).timeout(std::time::Duration::from_millis(100)).build();let account=account(source.clone(),slot("owner","local-org"));
        let snapshot=super::super::fetch(&agent,&account).unwrap();assert_eq!(snapshot.windows.iter().map(|w|w.remaining).collect::<Vec<_>>(),vec![74.0,16.0]);assert_eq!(snapshot.windows[0].reset,Some(now+3600));assert_eq!(snapshot.at,now-10,"不能把讀取時間當成官方記錄時間");
        for (org,at,bytes) in [("other-org",now-10,body.clone()),("local-org",now-60,body),("local-org",now-10,b"invalid-response".to_vec())]{desktop_cache::fixture(&cache,org,at,&bytes);let snapshot=super::super::fetch(&agent,&account).unwrap();assert_eq!(snapshot.windows.iter().map(|w|w.remaining).collect::<Vec<_>>(),vec![76.0,20.0]);assert!(snapshot.windows.iter().all(|w|w.reset.is_none()),"不可用快取必須降級為未知，不能猜測重置時間");}
        assert_eq!(listener.accept().unwrap_err().kind(),std::io::ErrorKind::WouldBlock);assert_eq!(fs::read_to_string(source).unwrap(),identity);assert_eq!(fs::read_to_string(dir.join(HISTORY)).unwrap(),history);let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn local_desktop_switching_organizations_or_users_does_not_reuse_old_records(){
        let dir=dir();let path=dir.join("config.json");fs::write(&path,json!({"lastKnownAccountUuid":"owner"}).to_string()).unwrap();earlier_login(&path);let now=chrono::Utc::now().timestamp_millis();
        let old=account(path.clone(),slot("owner","old-org"));
        fs::write(dir.join(HISTORY),json!({"version":2,"samples":[{"t":now-1000,"org":"old-org","u":{"fh":2}},{"t":now,"org":"new-org","u":{"fh":17,"sd":101}}]}).to_string()).unwrap();
        assert!(fetch(&old).is_err());let current=account(path.clone(),slot("owner","new-org"));let snapshot=fetch(&current).unwrap();assert_eq!(snapshot.windows.len(),1);assert_eq!(snapshot.windows[0].remaining,83.0);
        fs::write(&path,json!({"lastKnownAccountUuid":"other-owner"}).to_string()).unwrap();assert!(fetch(&current).is_err());
        fs::write(dir.join(HISTORY),json!({"version":1,"samples":[{"t":now,"fh":25,"sd":70}]}).to_string()).unwrap();assert!(profiles(&path).is_err(),"缺少帳號歸屬的舊版歷史不能匯入");let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn switched_desktop_login_waits_for_its_own_usage(){
        let dir=dir();let path=dir.join("config.json");let now=chrono::Utc::now().timestamp();
        let write_owner=|owner:&str,at:u64|{
            fs::write(&path,json!({"lastKnownAccountUuid":owner}).to_string()).unwrap();
            fs::File::options().write(true).open(&path).unwrap().set_modified(std::time::UNIX_EPOCH+std::time::Duration::from_secs(at)).unwrap();
        };
        write_owner("old",(now-120) as u64);
        let write_sample=|at:i64|fs::write(dir.join(HISTORY),json!({"version":2,"samples":[{"t":at*1000,"org":"shared","u":{"fh":30}}]}).to_string()).unwrap();
        write_sample(now-60);assert!(profiles(&path).is_ok());
        write_owner("new",(now-20) as u64);
        desktop_cache::fixture(&dir.join("Cache/Cache_Data"),"shared",now-60,br#"{"five_hour":{"utilization":30}}"#);
        assert!(profiles(&path).is_err());
        desktop_cache::fixture(&dir.join("Cache/Cache_Data"),"shared",now-5,br#"{"five_hour":{"utilization":47}}"#);
        let active=account(path.clone(),slot("new","shared"));assert_eq!(fetch(&active).unwrap().windows[0].remaining,53.0);
        desktop_cache::fixture(&dir.join("Cache/Cache_Data"),"shared",now-60,br#"{"five_hour":{"utilization":30}}"#);
        write_sample(now-10);assert_eq!(profiles(&path).unwrap()[0].slot,slot("new","shared"));
        let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn invalid_code_settings_do_not_install_or_change_files(){
        let dir=dir();let path=code_fixture(&dir);let settings=dir.join("settings.json");
        for raw in ["[]","null","42"] {
            fs::write(&settings,raw).unwrap();assert!(install_code_collector(&path).is_err());
            assert_eq!(fs::read_to_string(&settings).unwrap(),raw);assert!(!dir.join(COLLECTOR).exists());
        }
        let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn local_code_data_keeps_resets_and_rejects_a_previous_login(){
        let dir=dir();let path=code_fixture(&dir);let now=chrono::Utc::now().timestamp();
        let data=json!({"version":1,"account":"code-user","organization":"code-org","at":now-10,"rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":now+400},"seven_day":{"used_percentage":41.2,"resets_at":now+4000}}});fs::write(&path,data.to_string()).unwrap();
        let account=account(path.clone(),code_slot(&path).unwrap());let snapshot=fetch(&account).unwrap();assert_eq!(snapshot.windows[0].remaining,76.5);assert_eq!(snapshot.windows[0].reset,Some(now+400));assert_eq!(snapshot.at,now-10);
        let mut expired=data.clone();expired["rate_limits"]["five_hour"]["resets_at"]=json!(now-1);fs::write(&path,expired.to_string()).unwrap();assert_eq!(fetch(&account).unwrap().windows.len(),1);
        fs::write(dir.join(".claude.json"),json!({"oauthAccount":{"accountUuid":"other-user","organizationUuid":"code-org"}}).to_string()).unwrap();assert!(fetch(&account).is_err());let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn local_collector_install_preserves_other_settings_and_existing_status_lines(){
        let dir=dir();let path=code_fixture(&dir);let settings_path=dir.join("settings.json");fs::write(&settings_path,json!({"permissions":{"allow":["Read"]},"hooks":{"Stop":[]}}).to_string()).unwrap();
        install_code_collector(&path).unwrap();let first=read_json(&settings_path).unwrap();assert_eq!(first["permissions"]["allow"][0],"Read");assert!(first["hooks"]["Stop"].is_array());install_code_collector(&path).unwrap();assert_eq!(read_json(&settings_path).unwrap(),first);
        let mut customized=first.clone();customized["statusLine"]["padding"]=json!(2);
        let customized=customized.to_string();fs::write(&settings_path,&customized).unwrap();install_code_collector(&path).unwrap();assert_eq!(fs::read_to_string(&settings_path).unwrap(),customized);
        // 原本就有別人的狀態列：不覆蓋也不拒絕，接在它前面；其餘設定與狀態列欄位原樣保留。
        let custom=json!({"statusLine":{"type":"command","command":"my-existing-statusline","padding":1},"padding":2}).to_string();fs::write(&settings_path,&custom).unwrap();
        assert!(install_code_collector(&path).unwrap(),"原有狀態列要轉接，不能拒絕");
        let chained=read_json(&settings_path).unwrap();let expected=format!("{} -Tee | my-existing-statusline",collector_command(&dir.join(COLLECTOR)));
        assert_eq!(chained["statusLine"]["command"],expected);assert_eq!(chained["statusLine"]["type"],"command");assert_eq!(chained["statusLine"]["padding"],1);assert_eq!(chained["padding"],2);
        assert_eq!(fs::read_to_string(dir.join(ORIGINAL_RECORD)).unwrap(),"my-existing-statusline","要留一份原指令");
        let after=fs::read_to_string(&settings_path).unwrap();assert!(install_code_collector(&path).unwrap());assert_eq!(fs::read_to_string(&settings_path).unwrap(),after,"再按一次不能重複轉接");
        let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn chaining_edits_only_the_original_command_text_and_refuses_unknown_status_lines(){
        let dir=dir();let path=code_fixture(&dir);let settings_path=dir.join("settings.json");
        // 使用者自己的排版與鍵順序（不是字母序）要原樣保留，只換掉指令那一段。
        let raw="{\n  \"z_last\": 1,\n  \"statusLine\": { \"type\": \"command\", \"command\": \"C:/tools/status.cmd\" },\n  \"a_first\": [1, 2]\n}\n";
        fs::write(&settings_path,raw).unwrap();assert!(install_code_collector(&path).unwrap());
        let new_command=format!("{} -Tee | C:/tools/status.cmd",collector_command(&dir.join(COLLECTOR)));
        assert_eq!(fs::read_to_string(&settings_path).unwrap(),raw.replace("\"C:/tools/status.cmd\"",&serde_json::to_string(&new_command).unwrap()));
        // 狀態列不是指令類型：看不懂就不動，也不寫收集腳本。
        let _=fs::remove_file(dir.join(COLLECTOR));
        let odd=json!({"statusLine":{"type":"static","text":"hello"}}).to_string();fs::write(&settings_path,&odd).unwrap();
        assert!(install_code_collector(&path).is_err());assert_eq!(fs::read_to_string(&settings_path).unwrap(),odd);assert!(!dir.join(COLLECTOR).exists());
        let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn a_status_line_that_lost_the_collector_is_reported_instead_of_waiting_forever(){
        let dir=dir();let path=code_fixture(&dir);let settings_path=dir.join("settings.json");let account=account(path.clone(),code_slot(&path).unwrap());
        let generic=fetch(&account).unwrap_err();assert!(!generic.contains("沒有接上"),"讀不到設定時不能亂怪使用者：{generic}");
        fs::write(&settings_path,json!({"statusLine":{"type":"command","command":"C:/tools/status.cmd"}}).to_string()).unwrap();
        assert_eq!(fetch(&account).unwrap_err(),COLLECTOR_LOST,"狀態列被改回別人的指令，要明說");
        install_code_collector(&path).unwrap();assert_eq!(fetch(&account).unwrap_err(),generic,"接上之後只是還沒有資料");
        let _=fs::remove_dir_all(dir);
    }
    #[cfg(windows)]
    fn run_collector(args:&[&str],input:&[u8],dir:&Path)->std::process::Output{
        use std::{io::Write,process::{Command,Stdio}};use std::os::windows::process::CommandExt;
        let mut child=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"]).arg(dir.join(COLLECTOR)).args(args).creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();child.wait_with_output().unwrap()
    }
    #[cfg(windows)]
    #[test]
    fn tee_mode_passes_the_input_through_byte_for_byte_and_still_records_the_quota(){
        let dir=dir();let path=code_fixture(&dir);install_code_collector(&path).unwrap();
        let input=json!({"model":{"display_name":"Claude 測試"},"workspace":{"current_dir":"D:\\專案\\島仔"},"rate_limits":{"five_hour":{"used_percentage":12.5,"resets_at":chrono::Utc::now().timestamp()+1000}}}).to_string();
        let output=run_collector(&["-Tee"],input.as_bytes(),&dir);assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout,input.as_bytes(),"轉接時原指令收到的輸入必須和 Claude Code 給的一模一樣（含中文）");
        assert_eq!(read_json(&path).unwrap()["rate_limits"]["five_hour"]["used_percentage"],12.5);
        // 輸入不是 JSON 時也要原樣交給下一個指令，不能吞掉。
        let garbage=run_collector(&["-Tee"],b"not json \xe6\xb8\xac",&dir);assert_eq!(garbage.stdout,b"not json \xe6\xb8\xac");
        let _=fs::remove_dir_all(dir);
    }
    #[cfg(windows)]
    #[test]
    fn a_chained_status_line_keeps_the_original_command_working_through_a_real_pipe(){
        use std::{process::{Command,Stdio},io::Write};use std::os::windows::process::CommandExt;
        let dir=dir();let path=code_fixture(&dir);
        fs::write(dir.join("settings.json"),json!({"statusLine":{"type":"command","command":"findstr \"^\""}}).to_string()).unwrap();
        assert!(install_code_collector(&path).unwrap());
        let command=read_json(&dir.join("settings.json")).unwrap()["statusLine"]["command"].as_str().unwrap().to_owned();
        let run=|command:&str,input:&[u8]|{
            let mut child=Command::new("cmd.exe").raw_arg(format!("/d /s /c \"{command}\"")).creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
            let mut stdin=child.stdin.take().unwrap();let input=input.to_vec();let writer=std::thread::spawn(move||{let _=stdin.write_all(&input);});
            let output=child.wait_with_output().unwrap();let _=writer.join();output
        };
        let input=json!({"model":{"display_name":"Claude"},"rate_limits":{"five_hour":{"used_percentage":30.0,"resets_at":chrono::Utc::now().timestamp()+1000}}}).to_string();
        let output=run(&command,input.as_bytes());assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(),input,"最後輸出要是原指令的輸出");
        assert_eq!(read_json(&path).unwrap()["rate_limits"]["five_hour"]["used_percentage"],30.0,"同一份輸入也要被島仔記錄");
        // 原指令不讀輸入就退出（例如 Orca 的腳本不在它的終端裡時）：管道提前關閉，島仔照樣記錄、整體不報錯。
        let _=fs::remove_file(&path);let quits=command.replace("findstr \"^\"","cmd /d /c exit 0");
        let big=json!({"junk":"x".repeat(300_000),"rate_limits":{"five_hour":{"used_percentage":44.0,"resets_at":chrono::Utc::now().timestamp()+1000}}}).to_string();
        let output=run(&quits,big.as_bytes());assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        assert_eq!(read_json(&path).unwrap()["rate_limits"]["five_hour"]["used_percentage"],44.0);
        let _=fs::remove_dir_all(dir);
    }
    #[cfg(windows)]
    #[test]
    fn local_statusline_collector_saves_only_quota_fields(){
        use std::{io::Write,process::{Command,Stdio}};use std::os::windows::process::CommandExt;
        let dir=dir();let path=code_fixture(&dir);install_code_collector(&path).unwrap();
        let mut child=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"]).arg(dir.join(COLLECTOR)).creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
        let input=json!({"model":{"display_name":"Claude"},"transcript_path":"private-chat-path","messages":["private-chat-content"],"rate_limits":{"five_hour":{"used_percentage":12.5,"resets_at":chrono::Utc::now().timestamp()+1000}}});
        child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();let output=child.wait_with_output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let raw=fs::read_to_string(&path).unwrap();assert!(!raw.contains("private-chat"));let data=read_json(&path).unwrap();assert_eq!(data["account"],"code-user");assert_eq!(data["rate_limits"]["five_hour"]["used_percentage"],12.5);assert!(fetch(&account(path.clone(),code_slot(&path).unwrap())).is_ok());let _=fs::remove_dir_all(dir);
    }
    #[test]
    fn local_desktop_history_works_without_a_login_token_and_keeps_the_record_time(){
        let dir=std::env::temp_dir().join(format!("daozai-local-claude-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&dir).unwrap();let source=dir.join("config.json");
        let identity=serde_json::json!({"lastKnownAccountUuid":"local-user","oauth:tokenCacheV2":"unreadable-token"}).to_string();
        fs::write(&source,&identity).unwrap();earlier_login(&source);
        let at=chrono::Utc::now().timestamp_millis()-60_000;
        let history=serde_json::json!({"version":2,"samples":[{"t":at-1000,"org":"local-org","u":{"fh":8,"sd":70}},{"t":at,"org":"local-org","u":{"fh":11,"sd":76}}]}).to_string();
        fs::write(dir.join("plan-usage-history.json"),&history).unwrap();
        let slot=digest::digest(&digest::SHA256,b"local-user|local-org").as_ref().iter().map(|b|format!("{b:02x}")).collect::<String>();
        let mut account=Account{id:"local-test".into(),provider:super::super::Provider::Claude,alias:"本地測試".into(),source:source.clone(),slot:Some(slot),enabled:true,pinned:false};
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
        let agent=ureq::AgentBuilder::new().proxy(ureq::Proxy::new(format!("http://{}",listener.local_addr().unwrap())).unwrap()).timeout(std::time::Duration::from_millis(100)).build();
        let snapshot=super::super::fetch(&agent,&account).expect("Claude 必須只讀取本地用量，不依賴登入令牌");
        assert_eq!(snapshot.windows.iter().map(|w|w.remaining).collect::<Vec<_>>(),vec![89.0,24.0]);
        assert_eq!(snapshot.at,at/1000);assert!(snapshot.windows.iter().all(|w|w.reset.is_none()),"歷史快取沒有重置時間，不能推算");
        account.slot=Some("another-account".into());assert!(super::super::fetch(&ureq::Agent::new(),&account).is_err(),"本地快取不能串號");
        fs::remove_file(dir.join("plan-usage-history.json")).unwrap();account.slot=None;
        assert!(super::super::fetch(&agent,&account).unwrap_err().0.contains("本地"),"沒有快取也不能回退聯網");
        assert_eq!(listener.accept().unwrap_err().kind(),std::io::ErrorKind::WouldBlock,"Claude 本地讀取不能發起任何代理連線");
        assert_eq!(fs::read_to_string(&source).unwrap(),identity);let _=fs::remove_dir_all(dir);
    }
}
