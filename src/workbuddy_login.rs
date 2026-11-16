//! 預設瀏覽器登入：官網生成一次性請求，島仔輪詢對應的授權結果。
use super::{Auth,Session,Snapshot,Failure,slot,query,save_session};
use serde_json::Value;
use std::io::Read;
use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
const ORIGIN:&str="https://www.codebuddy.cn";

fn read(response:ureq::Response)->Result<Value,String>{
    serde_json::from_reader(response.into_reader().take(2*1024*1024)).map_err(|_|"官網登入響應無效".into())
}
fn response(value:Value)->Result<Value,String>{
    if value["code"].as_i64()!=Some(0){return Err("WorkBuddy 暫時無法建立登入請求，請稍後重試".into());}
    value.get("data").cloned().ok_or_else(||"官網未返回登入請求".into())
}
fn login_request(value:Value)->Result<(String,String),String>{
    let data=response(value)?;
    let state=data["state"].as_str().filter(|s|!s.is_empty()).ok_or("官網未返回登入標識")?;
    let link=data["authUrl"].as_str().ok_or("官網未返回登入地址")?;
    let url=url::Url::parse(link).map_err(|_|"官網登入地址無效")?;
    if url.origin().ascii_serialization()!=ORIGIN || url.path()!="/login"
        || !url.query_pairs().any(|(k,v)|k=="state"&&v==state){
        return Err("官網登入地址與請求不匹配".into());
    }
    Ok((state.into(),link.into()))
}
#[cfg(windows)]
fn open_browser(url:&str)->Result<(),String>{
    use windows::{core::{w,PCWSTR},Win32::UI::{Shell::ShellExecuteW,WindowsAndMessaging::SW_SHOWNORMAL}};
    let wide=url.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let result=unsafe{ShellExecuteW(None,w!("open"),PCWSTR(wide.as_ptr()),PCWSTR::null(),PCWSTR::null(),SW_SHOWNORMAL)};
    if result.0 as isize<=32{return Err("無法開啟預設瀏覽器，請檢查 Windows 預設應用設定".into());}
    Ok(())
}
#[cfg(not(windows))]
fn open_browser(_url:&str)->Result<(),String>{Err("WorkBuddy 登入暫僅支援 Windows".into())}

pub(super) fn connect(expected:Option<&str>,cancel:Arc<AtomicBool>)->Result<(String,Snapshot),String>{
    let agent=super::super::http_agent();
    let value=read(agent.post(&format!("{ORIGIN}/v2/plugin/auth/state?platform=daozai"))
        .set("X-No-Authorization","true").set("X-No-User-Id","true").set("Content-Type","application/json")
        .send_string("{}").map_err(|_|"無法建立 WorkBuddy 登入請求")?)?;
    let (state,link)=login_request(value)?;
    open_browser(&link)?;
    wait_for_login(&agent,&state,expected,cancel)
}
pub(super) fn wait_for_login(agent:&ureq::Agent,state:&str,expected:Option<&str>,cancel:Arc<AtomicBool>)->Result<(String,Snapshot),String>{
    let mut poll=url::Url::parse(&format!("{ORIGIN}/v2/plugin/auth/token")).unwrap();
    poll.query_pairs_mut().append_pair("state",&state);
    let start=Instant::now();
    while start.elapsed()<Duration::from_secs(600){
        for _ in 0..20{
            if cancel.load(Ordering::Relaxed){return Err("已取消 WorkBuddy 登入".into());}
            std::thread::sleep(Duration::from_millis(100));
        }
        let result=agent.get(poll.as_str()).set("X-No-Authorization","true").call();
        let value:Value=match result{
            Ok(r)=>read(r)?,
            Err(ureq::Error::Status(400,r))=>read(r)?,
            Err(ureq::Error::Transport(_))=>continue,
            Err(_)=>return Err("WorkBuddy 登入請求已失效，請重新連線".into()),
        };
        if value["code"].as_i64()==Some(11217){continue;}
        let auth:Auth=serde_json::from_value(value["data"].clone()).map_err(|_|"官網未返回有效授權，請重新連線")?;
        if auth.access_token.is_empty(){return Err("官網登入授權為空，請重新連線".into());}
        let mut session=Session{origin:ORIGIN.into(),ua:String::new(),cookie:String::new(),auth:Some(auth)};
        let accounts=super::with_transport_retry(&agent,ORIGIN,|agent|super::api(agent,&session,"/v2/plugin/accounts")).map_err(Failure::message)?;
        let list=accounts.pointer("/data/accounts").and_then(Value::as_array).ok_or("官網未返回帳號列表")?;
        let personal=list.iter().filter(|a|a["enterpriseId"].as_str().unwrap_or_default().is_empty()&&a["isCurrentOneIdEnterprise"]!=true);
        let account=personal.filter(|a|a["uid"].as_str().is_some_and(|uid|expected.is_none_or(|id|slot(uid)==id))).next()
            .ok_or("請在官網登入原來繫結的個人帳號後重試")?;
        let id=slot(account["uid"].as_str().ok_or("官網未返回帳號標識")?);
        session.auth.as_mut().unwrap().uid=account["uid"].as_str().unwrap().into();
        let snapshot=query(&agent,&session,&id).map_err(Failure::message)?;
        if cancel.load(Ordering::Relaxed){return Err("已取消 WorkBuddy 登入".into());}
        save_session(&id,&session)?;
        return Ok((id,snapshot));
    }
    Err("官網登入等待超時，請重新連線".into())
}
#[cfg(test)]
mod tests{
    use super::*;
    use serde_json::json;
    #[test]
    fn login_url_must_match_official_origin_and_state(){
        let make=|url|json!({"code":0,"data":{"state":"test","authUrl":url}});
        assert!(login_request(make("https://www.codebuddy.cn/login?state=test&platform=daozai")).is_ok());
        for url in ["https://example.com/login?state=test","https://www.codebuddy.cn/login?state=other","http://www.codebuddy.cn/login?state=test"]{
            assert!(login_request(make(url)).is_err());
        }
    }
}
