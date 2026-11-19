//! Google Workspace CLI (`gws`) 整合：首次啟動自動檢查/登入，免應用程式密碼存取 Gmail 與 Google 日曆。

use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use base64::Engine;
use chrono::NaiveDate;
use eframe::egui;

pub const LOGIN_SCOPES: &str = "https://www.googleapis.com/auth/gmail.readonly,https://www.googleapis.com/auth/calendar.readonly,email,profile,openid";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthStatus {
    pub installed: bool,
    pub client_config_exists: bool,
    pub token_valid: bool,
    pub user: String,
    pub scopes: Vec<String>,
    pub has_gmail: bool,
    pub has_calendar: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    LoggingIn,
    Connected(AuthStatus),
    NotLoggedIn,
    NeedsSetup,
    NotInstalled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub phase: Phase,
    pub auto_bind: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Executable {
    Binary(PathBuf),
    NodeScript { node: PathBuf, script: PathBuf },
    CmdWrapper(PathBuf),
}

pub fn has_gmail_scope(scopes: &[String]) -> bool {
    scopes.iter().any(|scope| {
        scope.contains("/auth/gmail.readonly")
            || scope.contains("/auth/gmail.modify")
            || scope.contains("mail.google.com")
    })
}

pub fn has_calendar_scope(scopes: &[String]) -> bool {
    scopes.iter().any(|scope| scope.contains("/auth/calendar"))
}

fn parse_json_object(raw: &str) -> Result<serde_json::Value, String> {
    let start = raw
        .find('{')
        .ok_or_else(|| "gws 未返回有效的 JSON 資料".to_string())?;
    serde_json::from_str::<serde_json::Value>(&raw[start..])
        .map_err(|e| format!("解析 gws JSON 失敗：{e}"))
}

pub fn parse_auth_status(raw: &str) -> Result<AuthStatus, String> {
    let json = parse_json_object(raw)?;
    let client_config_exists = json
        .get("client_config_exists")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let token_valid = json
        .get("token_valid")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user = json
        .get("user")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let scopes: Vec<String> = json
        .get("scopes")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect();
    let has_gmail = has_gmail_scope(&scopes);
    let has_calendar = has_calendar_scope(&scopes);
    Ok(AuthStatus {
        installed: true,
        client_config_exists,
        token_valid,
        user,
        scopes,
        has_gmail,
        has_calendar,
    })
}

pub fn gmail_uid(id: &str) -> u32 {
    let digest = ring::digest::digest(&ring::digest::SHA256, id.as_bytes());
    let b = digest.as_ref();
    u32::from_be_bytes([b[0], b[1], b[2], b[3]]).max(1)
}

pub fn parse_gmail_list(raw: &str) -> Result<Vec<String>, String> {
    let json = parse_json_object(raw)?;
    Ok(json
        .get("messages")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(|v| v.as_str()).map(str::to_owned))
        .filter(|id| !id.is_empty())
        .collect())
}

fn is_seen_from_labels(json: &serde_json::Value) -> bool {
    !json
        .get("labelIds")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .any(|label| label.as_str() == Some("UNREAD"))
}

pub fn parse_gmail_minimal_seen(raw: &str) -> Result<bool, String> {
    let json = parse_json_object(raw)?;
    Ok(is_seen_from_labels(&json))
}

pub fn parse_gmail_message(raw: &str, account_email: &str) -> Result<Option<(crate::mail::Item, bool)>, String> {
    let json = parse_json_object(raw)?;
    let Some(id) = json.get("id").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let Some(raw_b64) = json.get("raw").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw_b64.trim_end_matches('='))
        .map_err(|e| format!("解碼 Gmail 原始信件失敗：{e}"))?;
    let parsed = crate::mail::parse_message(&decoded);
    let secs = json
        .get("internalDate")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()))
        .map(|ms| ms / 1000);
    let time = secs
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .map(|dt| dt.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let seen = is_seen_from_labels(&json);
    let item = crate::mail::Item {
        account: account_email.to_string(),
        from: if parsed.from.is_empty() {
            account_email.to_string()
        } else {
            parsed.from.clone()
        },
        subject: if parsed.subject.is_empty() {
            "（無主題）".into()
        } else {
            parsed.subject.clone()
        },
        time,
        code: crate::mail::code_of(&parsed),
        received_unix: secs,
        summary: crate::mail::body_of(&parsed).into(),
        html: parsed.html.clone().into(),
        links: crate::mail::links_of(&parsed),
        meeting: crate::mail::meeting_of(&parsed.calendar),
        uid: gmail_uid(id),
    };
    Ok(Some((item, seen)))
}

pub fn parse_calendar_list(raw: &str) -> Result<Vec<crate::calendar::CalendarInfo>, String> {
    let json = parse_json_object(raw)?;
    let mut entries: Vec<(bool, crate::calendar::CalendarInfo)> = Vec::new();
    for item in json.get("items").and_then(|v| v.as_array()).into_iter().flatten() {
        if item.get("deleted").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let primary = item.get("primary").and_then(|v| v.as_bool()) == Some(true);
        let name = ["summaryOverride", "summary"]
            .into_iter()
            .filter_map(|key| item.get(key).and_then(|v| v.as_str()).map(str::trim))
            .find(|s| !s.is_empty())
            .unwrap_or(id)
            .to_string();
        entries.push((
            primary,
            crate::calendar::CalendarInfo {
                href: id.to_string(),
                name,
            },
        ));
    }
    entries.sort_by_key(|(primary, _)| !*primary);
    Ok(entries.into_iter().map(|(_, info)| info).collect())
}

fn parse_event_date(value: Option<&serde_json::Value>) -> Option<(bool, chrono::DateTime<chrono::Local>)> {
    let obj = value?.as_object()?;
    if let Some(dt) = obj.get("dateTime").and_then(|v| v.as_str()) {
        let parsed = chrono::DateTime::parse_from_rfc3339(dt.trim()).ok()?;
        return Some((false, parsed.with_timezone(&chrono::Local)));
    }
    if let Some(date_str) = obj.get("date").and_then(|v| v.as_str()) {
        let date = NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").ok()?;
        let local = date
            .and_hms_opt(0, 0, 0)?
            .and_local_timezone(chrono::Local)
            .earliest()?;
        return Some((true, local));
    }
    None
}

pub fn parse_calendar_events(
    raw: &str,
    lo: chrono::DateTime<chrono::Utc>,
    hi: chrono::DateTime<chrono::Utc>,
) -> Result<crate::calendar::ParsedEvents, String> {
    let json = parse_json_object(raw)?;
    let mut report = crate::calendar::ReadReport::default();
    let mut events = Vec::new();
    let mut seen = HashSet::new();

    for item in json.get("items").and_then(|v| v.as_array()).into_iter().flatten() {
        report.received += 1;
        if item
            .get("status")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("cancelled"))
        {
            report.cancelled += 1;
            continue;
        }
        let Some((all_day, start_dt)) = parse_event_date(item.get("start")) else {
            report.invalid += 1;
            report.problem("部分日程的時間或時區無法解析");
            continue;
        };
        let end_parsed = parse_event_date(item.get("end"))
            .map(|(_, dt)| dt.timestamp())
            .filter(|ts| *ts > start_dt.timestamp())
            .or_else(|| {
                all_day
                    .then(|| {
                        start_dt
                            .date_naive()
                            .succ_opt()?
                            .and_hms_opt(0, 0, 0)?
                            .and_local_timezone(chrono::Local)
                            .earliest()
                            .map(|dt| dt.timestamp())
                    })
                    .flatten()
            });
        let title = item
            .get("summary")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("會議")
            .to_string();
        let uid = ["iCalUID", "id"]
            .into_iter()
            .filter_map(|key| item.get(key).and_then(|v| v.as_str()).map(str::trim))
            .find(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
        let meeting = crate::mail::Meeting {
            all_day,
            title,
            start: start_dt.timestamp(),
            end: end_parsed,
            at: start_dt
                .format(if all_day { "%m-%d 全天" } else { "%m-%d %H:%M" })
                .to_string(),
            uid,
        };
        if meeting.start >= hi.timestamp()
            || meeting.end.unwrap_or(meeting.start + 1) <= lo.timestamp()
        {
            report.outside += 1;
            continue;
        }
        if seen.insert(crate::calendar::dedupe_key(&meeting)) {
            if events.len() == 5000 {
                report.limited = true;
                break;
            }
            events.push(meeting);
        }
    }
    events.sort_by_key(|m| m.start);
    Ok(crate::calendar::ParsedEvents { events, report })
}

fn executable_from_dir(dir: &Path) -> Option<Executable> {
    #[cfg(windows)]
    {
        let exe = dir.join("gws.exe");
        if exe.is_file() {
            return Some(Executable::Binary(exe));
        }
        let native_bin = dir
            .join("node_modules")
            .join("@googleworkspace")
            .join("cli")
            .join("bin")
            .join("gws.exe");
        if native_bin.is_file() {
            return Some(Executable::Binary(native_bin));
        }
        let run_js = dir
            .join("node_modules")
            .join("@googleworkspace")
            .join("cli")
            .join("run.js");
        let node_exe = dir.join("node.exe");
        if run_js.is_file() && node_exe.is_file() {
            return Some(Executable::NodeScript {
                node: node_exe,
                script: run_js,
            });
        }
        let cmd = dir.join("gws.cmd");
        if cmd.is_file() {
            return Some(Executable::CmdWrapper(cmd));
        }
        None
    }
    #[cfg(not(windows))]
    {
        let bin = dir.join("gws");
        if bin.is_file() {
            return Some(Executable::Binary(bin));
        }
        None
    }
}

pub fn find_executable() -> Option<Executable> {
    if let Some(override_bin) = std::env::var_os("DAOZAI_GWS_BIN") {
        let path = PathBuf::from(override_bin);
        if path.is_file() {
            return Some(Executable::Binary(path));
        }
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let native_bin = PathBuf::from(appdata)
            .join("npm")
            .join("node_modules")
            .join("@googleworkspace")
            .join("cli")
            .join("bin")
            .join("gws.exe");
        if native_bin.is_file() {
            return Some(Executable::Binary(native_bin));
        }
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if let Some(found) = executable_from_dir(&dir) {
                return Some(found);
            }
        }
    }
    None
}

fn build_command(exe: &Executable, args: &[&str]) -> Command {
    match exe {
        Executable::Binary(path) => {
            let mut cmd = Command::new(path);
            cmd.args(args);
            cmd
        }
        Executable::NodeScript { node, script } => {
            let mut cmd = Command::new(node);
            cmd.arg(script);
            cmd.args(args);
            cmd
        }
        Executable::CmdWrapper(path) => {
            let mut cmd = Command::new("cmd.exe");
            cmd.arg("/C");
            cmd.arg(path);
            cmd.args(args);
            cmd
        }
    }
}

fn run_command(
    exe: &Executable,
    args: &[&str],
    timeout: Duration,
    hide_window: bool,
) -> Result<(bool, String, String), String> {
    let mut cmd = build_command(exe, args);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    if hide_window {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    let _ = hide_window;

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("無法啟動 gws CLI：{e}"))?;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("等待 gws CLI 失敗：{e}"));
            }
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    let Some(status) = status else {
        return Err("gws CLI 執行逾時".into());
    };
    Ok((status.success(), stdout, stderr))
}

fn summarize_error(stdout: &str, stderr: &str) -> String {
    if let Ok(json) = parse_json_object(stdout).or_else(|_| parse_json_object(stderr)) {
        if let Some(msg) = json
            .get("error")
            .and_then(|e| e.get("message").or(Some(e)))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return msg.to_string();
        }
    }
    let line = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("Using keyring backend"))
        .unwrap_or("gws CLI 返回錯誤");
    line.chars().take(180).collect()
}

fn run_json(args: &[&str], timeout: Duration) -> Result<String, String> {
    let exe = find_executable().ok_or_else(|| "找不到 Google Workspace CLI（gws）".to_string())?;
    let (ok, stdout, stderr) = run_command(&exe, args, timeout, true)?;
    if ok {
        Ok(stdout)
    } else {
        Err(summarize_error(&stdout, &stderr))
    }
}

fn fetch_gmail_profile_email() -> Option<String> {
    let raw = run_json(
        &["gmail", "users", "getProfile", "--params", "{\"userId\":\"me\"}"],
        Duration::from_secs(15),
    )
    .ok()?;
    let json = parse_json_object(&raw).ok()?;
    json.get("emailAddress")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn fetch_primary_calendar_email() -> Option<String> {
    let raw = run_json(
        &["calendar", "calendarList", "list", "--params", "{\"maxResults\":20}"],
        Duration::from_secs(15),
    )
    .ok()?;
    let list = parse_calendar_list(&raw).ok()?;
    list.into_iter()
        .map(|c| c.href)
        .find(|id| id.contains('@'))
}

pub fn auth_status() -> Result<Option<AuthStatus>, String> {
    if find_executable().is_none() {
        return Ok(None);
    }
    let raw = run_json(&["auth", "status"], Duration::from_secs(15))?;
    let mut status = parse_auth_status(&raw)?;
    if status.token_valid && status.user.is_empty() {
        if status.has_gmail {
            if let Some(email) = fetch_gmail_profile_email() {
                status.user = email;
            }
        }
        if status.user.is_empty() && status.has_calendar {
            if let Some(email) = fetch_primary_calendar_email() {
                status.user = email;
            }
        }
    }
    Ok(Some(status))
}

pub fn login() -> Result<AuthStatus, String> {
    let exe = find_executable().ok_or_else(|| "找不到 Google Workspace CLI（gws）".to_string())?;
    if let Some(current) = auth_status()? {
        if !current.client_config_exists {
            return Err("尚未設定 Google OAuth 用戶端，請先執行 gws auth setup".into());
        }
    }
    let (ok, stdout, stderr) = run_command(
        &exe,
        &["auth", "login", "--scopes", LOGIN_SCOPES],
        Duration::from_secs(300),
        true,
    )?;
    if !ok {
        return Err(summarize_error(&stdout, &stderr));
    }
    let status = auth_status()?
        .ok_or_else(|| "找不到 Google Workspace CLI（gws）".to_string())?;
    if !status.token_valid {
        return Err("Google Workspace 登入未完成，請重試".into());
    }
    if !status.has_gmail && !status.has_calendar {
        return Err("已登入 Google Workspace，但未授權 Gmail 或日曆權限".into());
    }
    Ok(status)
}

#[cfg(windows)]
fn quote_cmd_arg(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub fn launch_setup_terminal() -> Result<(), String> {
    let exe = find_executable().ok_or_else(|| "找不到 Google Workspace CLI（gws）".to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let inner = match &exe {
            Executable::Binary(path) | Executable::CmdWrapper(path) => {
                format!("{} auth setup --login", quote_cmd_arg(&path.to_string_lossy()))
            }
            Executable::NodeScript { node, script } => format!(
                "{} {} auth setup --login",
                quote_cmd_arg(&node.to_string_lossy()),
                quote_cmd_arg(&script.to_string_lossy())
            ),
        };
        let mut cmd = Command::new("cmd.exe");
        // CREATE_NEW_CONSOLE (0x10): 直接彈出獨立終端機視窗，讓使用者依 gws auth setup 精靈操作。
        cmd.creation_flags(0x00000010);
        cmd.raw_arg(format!("/K \"{inner}\""));
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("無法開啟設定終端機：{e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Err("僅支援在 Windows 自動開啟設定終端機，請手動執行 gws auth setup --login".into())
    }
}

pub fn probe_mail(_email: &str) -> Result<(), String> {
    let Some(status) = auth_status()? else {
        return Err("找不到 Google Workspace CLI（gws）".into());
    };
    if !status.token_valid {
        return Err("尚未登入 Google Workspace CLI（gws）".into());
    }
    if !status.has_gmail {
        return Err("目前的 gws 授權缺少 Gmail 讀取權限，請重新登入授權".into());
    }
    let _ = list_messages("INBOX", false, 1)?;
    Ok(())
}

pub fn list_messages(label: &str, include_spam_trash: bool, max_results: u32) -> Result<Vec<String>, String> {
    let params = serde_json::json!({
        "userId": "me",
        "labelIds": [label],
        "includeSpamTrash": include_spam_trash,
        "maxResults": max_results,
    })
    .to_string();
    let raw = run_json(
        &["gmail", "users", "messages", "list", "--params", &params],
        Duration::from_secs(30),
    )?;
    parse_gmail_list(&raw)
}

pub fn fetch_mail_item(account_email: &str, id: &str) -> Result<Option<(crate::mail::Item, bool)>, String> {
    let params = serde_json::json!({
        "userId": "me",
        "id": id,
        "format": "raw",
    })
    .to_string();
    let raw = run_json(
        &["gmail", "users", "messages", "get", "--params", &params],
        Duration::from_secs(30),
    )?;
    parse_gmail_message(&raw, account_email)
}

pub fn fetch_mail_seen(id: &str) -> Result<bool, String> {
    let params = serde_json::json!({
        "userId": "me",
        "id": id,
        "format": "minimal",
    })
    .to_string();
    let raw = run_json(
        &["gmail", "users", "messages", "get", "--params", &params],
        Duration::from_secs(20),
    )?;
    parse_gmail_minimal_seen(&raw)
}

#[cfg(windows)]
pub fn manage_mail(request: &crate::mail_manage::Request) -> Result<crate::mail_manage::Snapshot, String> {
    use crate::mail_manage::{Action, Folder, Snapshot};
    const GWS_VALIDITY: u32 = 1;
    if request.action != Action::Load {
        return Err("Google Workspace CLI 目前為唯讀模式，請關閉「同步郵件操作到信箱」以在本機整理，或在 Gmail 網頁版操作".into());
    }
    let (label, include_spam_trash) = match request.folder {
        Folder::Inbox => ("INBOX", false),
        Folder::Trash => ("TRASH", true),
    };
    let ids = list_messages(label, include_spam_trash, 20)?;
    let mut entries = Vec::new();
    let mut items = Vec::new();
    let mut seen_uids = HashSet::new();
    for id in ids {
        let uid = gmail_uid(&id);
        if !seen_uids.insert(uid) {
            continue;
        }
        if request.validity == Some(GWS_VALIDITY) && request.known.contains(&uid) {
            let seen = fetch_mail_seen(&id).unwrap_or(true);
            entries.push((uid, seen));
        } else if let Some((item, seen)) = fetch_mail_item(&request.account.email, &id)? {
            entries.push((item.uid, seen));
            items.push(item);
        }
    }
    Ok(Snapshot {
        validity: GWS_VALIDITY,
        entries,
        items,
    })
}

pub fn fetch_calendar_range(
    cfg: &crate::calendar::Config,
    lo: chrono::DateTime<chrono::Utc>,
    hi: chrono::DateTime<chrono::Utc>,
) -> Result<crate::calendar::Fetched, String> {
    let list_raw = run_json(
        &["calendar", "calendarList", "list", "--params", "{\"maxResults\":100}"],
        Duration::from_secs(30),
    )?;
    let calendars = parse_calendar_list(&list_raw)?;
    let Some(first_href) = calendars.first().map(|c| c.href.clone()) else {
        return Err("沒有找到可讀取的 Google 日曆".into());
    };
    let skipped = crate::calendar::skipped_calendars(cfg);
    let mut report = crate::calendar::ReadReport {
        calendars: calendars.len(),
        found: calendars.clone(),
        ..Default::default()
    };
    let chosen: Vec<_> = calendars
        .iter()
        .filter(|info| !skipped.contains(&info.href))
        .collect();
    if chosen.is_empty() {
        return Ok(crate::calendar::Fetched {
            href: first_href,
            events: Vec::new(),
            report,
        });
    }

    let time_min = lo.to_rfc3339();
    let time_max = hi.to_rfc3339();
    let mut events = Vec::new();
    let mut any_ok = false;
    let mut first_error = None;

    for info in chosen {
        let params = serde_json::json!({
            "calendarId": info.href,
            "timeMin": time_min,
            "timeMax": time_max,
            "singleEvents": true,
            "orderBy": "startTime",
            "maxResults": 250,
        })
        .to_string();
        match run_json(
            &["calendar", "events", "list", "--params", &params],
            Duration::from_secs(30),
        )
        .and_then(|raw| parse_calendar_events(&raw, lo, hi))
        {
            Ok(parsed) => {
                any_ok = true;
                events.extend(parsed.events);
                report.merge(parsed.report);
            }
            Err(err) => {
                report.failed += 1;
                if report.issue.is_none() {
                    report.issue = Some(err.clone());
                }
                first_error.get_or_insert(err);
            }
        }
    }

    if !any_ok {
        return Err(first_error.unwrap_or_else(|| "無法讀取 Google 日曆".into()));
    }
    events.sort_by_key(|m| m.start);
    let mut seen = HashSet::new();
    events.retain(|m| seen.insert(crate::calendar::dedupe_key(m)));
    Ok(crate::calendar::Fetched {
        href: first_href,
        events,
        report,
    })
}

fn remember_id(order: &mut VecDeque<String>, set: &mut HashSet<String>, id: &str) {
    if set.insert(id.to_string()) {
        order.push_back(id.to_string());
        if order.len() > 200 {
            if let Some(old) = order.pop_front() {
                set.remove(&old);
            }
        }
    }
}

pub fn mail_worker(
    account: crate::mail::Account,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    interval: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ev: Sender<crate::mail::Event>,
) {
    use std::sync::atomic::Ordering;
    let sleep_stop = |secs: u64| -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while !stop.load(Ordering::Relaxed) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            std::thread::sleep(left.min(Duration::from_millis(500)));
        }
        true
    };

    let mut seen_order: VecDeque<String> = VecDeque::new();
    let mut seen_set: HashSet<String> = HashSet::new();
    let mut initialized = false;
    let mut failures: u32 = 0;

    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let _ = ev.send(crate::mail::Event::Status {
            account: account.email.clone(),
            status: crate::mail::Status::Connecting,
        });

        match auth_status() {
            Ok(Some(st)) if st.token_valid && st.has_gmail => {}
            Ok(Some(st)) if !st.token_valid => {
                let _ = ev.send(crate::mail::Event::Status {
                    account: account.email.clone(),
                    status: crate::mail::Status::Auth("請完成 Google Workspace CLI 登入".into()),
                });
                let delay = 30u64.saturating_mul(2u64.pow(failures.min(5))).min(300);
                failures = failures.saturating_add(1);
                if sleep_stop(delay) {
                    return;
                }
                continue;
            }
            Ok(Some(_)) => {
                let _ = ev.send(crate::mail::Event::Status {
                    account: account.email.clone(),
                    status: crate::mail::Status::Auth("缺少 Gmail 權限，請重新登入授權".into()),
                });
                let delay = 30u64.saturating_mul(2u64.pow(failures.min(5))).min(300);
                failures = failures.saturating_add(1);
                if sleep_stop(delay) {
                    return;
                }
                continue;
            }
            Ok(None) => {
                let _ = ev.send(crate::mail::Event::Status {
                    account: account.email.clone(),
                    status: crate::mail::Status::Failed("找不到 Google Workspace CLI（gws）".into()),
                });
                if sleep_stop(60) {
                    return;
                }
                continue;
            }
            Err(e) => {
                let _ = ev.send(crate::mail::Event::Status {
                    account: account.email.clone(),
                    status: crate::mail::Status::Failed(e),
                });
                let delay = 30u64.saturating_mul(2u64.pow(failures.min(5))).min(300);
                failures = failures.saturating_add(1);
                if sleep_stop(delay) {
                    return;
                }
                continue;
            }
        }

        if !initialized {
            match list_messages("INBOX", false, 10) {
                Ok(mut ids) => {
                    ids.reverse();
                    for id in ids {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(Some((item, _))) = fetch_mail_item(&account.email, &id) {
                            let _ = ev.send(crate::mail::Event::Mail { item, new: false });
                        }
                        remember_id(&mut seen_order, &mut seen_set, &id);
                    }
                    initialized = true;
                    failures = 0;
                    let _ = ev.send(crate::mail::Event::Status {
                        account: account.email.clone(),
                        status: crate::mail::Status::Ready,
                    });
                }
                Err(e) => {
                    let _ = ev.send(crate::mail::Event::Status {
                        account: account.email.clone(),
                        status: crate::mail::Status::Failed(e),
                    });
                    let delay = 30u64.saturating_mul(2u64.pow(failures.min(5))).min(300);
                    failures = failures.saturating_add(1);
                    if sleep_stop(delay) {
                        return;
                    }
                    continue;
                }
            }
        } else {
            let _ = ev.send(crate::mail::Event::Status {
                account: account.email.clone(),
                status: crate::mail::Status::Ready,
            });
        }

        loop {
            let secs = interval.load(Ordering::Relaxed).clamp(15, 900);
            if sleep_stop(secs) {
                return;
            }
            match list_messages("INBOX", false, 10) {
                Ok(ids) => {
                    failures = 0;
                    let mut unseen: Vec<String> = ids
                        .into_iter()
                        .filter(|id| !seen_set.contains(id))
                        .take(5)
                        .collect();
                    unseen.reverse();
                    for id in unseen {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(Some((item, _))) = fetch_mail_item(&account.email, &id) {
                            let _ = ev.send(crate::mail::Event::Mail { item, new: true });
                        }
                        remember_id(&mut seen_order, &mut seen_set, &id);
                    }
                }
                Err(e) => {
                    let _ = ev.send(crate::mail::Event::Status {
                        account: account.email.clone(),
                        status: crate::mail::Status::Failed(e),
                    });
                    let delay = 30u64.saturating_mul(2u64.pow(failures.min(5))).min(300);
                    failures = failures.saturating_add(1);
                    if sleep_stop(delay) {
                        return;
                    }
                    break;
                }
            }
        }
    }
}

enum ServiceCmd {
    Check { auto_login: bool, auto_bind: bool },
    Login,
    Setup,
}

pub struct Service {
    cmd: Sender<ServiceCmd>,
    ev: Receiver<Event>,
}

impl Service {
    pub fn start(ctx: egui::Context) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let (ev_tx, ev_rx) = channel();
        let _ = std::thread::Builder::new()
            .name("daozai-gws".into())
            .spawn(move || service_loop(cmd_rx, ev_tx, ctx));
        Self {
            cmd: cmd_tx,
            ev: ev_rx,
        }
    }

    pub fn check(&self, auto_login: bool, auto_bind: bool) {
        let _ = self.cmd.send(ServiceCmd::Check {
            auto_login,
            auto_bind,
        });
    }

    pub fn login(&self) {
        let _ = self.cmd.send(ServiceCmd::Login);
    }

    pub fn setup(&self) {
        let _ = self.cmd.send(ServiceCmd::Setup);
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.ev.try_recv().ok()
    }
}

fn send_event(ev: &Sender<Event>, ctx: &egui::Context, phase: Phase, auto_bind: bool) {
    let _ = ev.send(Event { phase, auto_bind });
    ctx.request_repaint();
}

fn service_loop(cmd_rx: Receiver<ServiceCmd>, ev: Sender<Event>, ctx: egui::Context) {
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            ServiceCmd::Check {
                auto_login,
                auto_bind,
            } => match auth_status() {
                Ok(None) => send_event(&ev, &ctx, Phase::NotInstalled, false),
                Ok(Some(st))
                    if st.token_valid
                        && (st.has_gmail || st.has_calendar)
                        && !st.user.is_empty() =>
                {
                    send_event(&ev, &ctx, Phase::Connected(st), auto_bind);
                }
                Ok(Some(st)) if !st.client_config_exists => {
                    send_event(&ev, &ctx, Phase::NeedsSetup, false);
                }
                Ok(Some(_)) if auto_login => {
                    send_event(&ev, &ctx, Phase::LoggingIn, false);
                    match login() {
                        Ok(st) => send_event(&ev, &ctx, Phase::Connected(st), true),
                        Err(err) => send_event(&ev, &ctx, Phase::Failed(err), false),
                    }
                }
                Ok(Some(_)) => send_event(&ev, &ctx, Phase::NotLoggedIn, false),
                Err(err) => send_event(&ev, &ctx, Phase::Failed(err), false),
            },
            ServiceCmd::Login => match auth_status() {
                Ok(None) => send_event(&ev, &ctx, Phase::NotInstalled, false),
                Ok(Some(st))
                    if st.token_valid
                        && (st.has_gmail || st.has_calendar)
                        && !st.user.is_empty() =>
                {
                    send_event(&ev, &ctx, Phase::Connected(st), true);
                }
                Ok(Some(st)) if !st.client_config_exists => {
                    send_event(&ev, &ctx, Phase::NeedsSetup, false);
                }
                Ok(Some(_)) => {
                    send_event(&ev, &ctx, Phase::LoggingIn, false);
                    match login() {
                        Ok(st) => send_event(&ev, &ctx, Phase::Connected(st), true),
                        Err(err) => send_event(&ev, &ctx, Phase::Failed(err), false),
                    }
                }
                Err(err) => send_event(&ev, &ctx, Phase::Failed(err), false),
            },
            ServiceCmd::Setup => {
                if let Err(err) = launch_setup_terminal() {
                    send_event(&ev, &ctx, Phase::Failed(err), false);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_status_parses_with_preamble_and_detects_scopes() {
        let sample = r#"Using keyring backend: keyring
{
  "auth_method": "oauth2",
  "client_config_exists": true,
  "token_valid": true,
  "user": "alice@gmail.com",
  "scopes": [
    "email",
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/gmail.readonly"
  ]
}"#;
        let st = parse_auth_status(sample).unwrap();
        assert!(st.installed && st.client_config_exists && st.token_valid);
        assert_eq!(st.user, "alice@gmail.com");
        assert!(st.has_gmail && st.has_calendar);
    }

    #[test]
    fn gmail_list_and_message_parse_otp_html_and_ics() {
        let list_raw = r#"{"messages":[{"id":"msg-1"},{"id":"msg-2"}]}"#;
        assert_eq!(parse_gmail_list(list_raw).unwrap(), vec!["msg-1", "msg-2"]);
        assert!(parse_gmail_list("{}").unwrap().is_empty());

        let mime = concat!(
            "From: Service <noreply@example.com>\r\n",
            "Subject: Verification\r\n",
            "Content-Type: multipart/mixed; boundary=\"bnd\"\r\n\r\n",
            "--bnd\r\nContent-Type: text/html; charset=utf-8\r\n\r\n",
            "<p>Code: <b>482910</b></p><a href=\"https://example.com/verify\">Verify</a>\r\n",
            "--bnd\r\nContent-Type: text/calendar; charset=utf-8\r\n\r\n",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:meet-1\r\nSUMMARY:Review\r\nDTSTART:20261119T080000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            "--bnd--\r\n"
        );
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mime.as_bytes());
        let msg_raw = serde_json::json!({
            "id": "msg-1",
            "internalDate": "1763539200000",
            "labelIds": ["INBOX", "UNREAD"],
            "raw": b64,
        })
        .to_string();
        let (item, seen) = parse_gmail_message(&msg_raw, "alice@gmail.com")
            .unwrap()
            .unwrap();
        assert!(!seen);
        assert_eq!(item.uid, gmail_uid("msg-1"));
        assert_ne!(item.uid, 0);
        assert_eq!(item.code.as_deref(), Some("482910"));
        assert_eq!(item.links[0].url, "https://example.com/verify");
        assert_eq!(item.meeting.unwrap().title, "Review");
    }

    #[test]
    fn calendar_list_puts_primary_first_and_skips_deleted() {
        let raw = r#"{
          "items": [
            {"id": "team@group.calendar.google.com", "summary": "Team"},
            {"id": "deleted@group.calendar.google.com", "summary": "Old", "deleted": true},
            {"id": "alice@gmail.com", "summary": "alice@gmail.com", "summaryOverride": "Personal", "primary": true}
          ]
        }"#;
        let list = parse_calendar_list(raw).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].href, "alice@gmail.com");
        assert_eq!(list[0].name, "Personal");
        assert_eq!(list[1].href, "team@group.calendar.google.com");
        assert_eq!(list[1].name, "Team");
    }

    #[test]
    fn calendar_events_handle_timed_all_day_cancelled_and_dedupe() {
        let lo = chrono::DateTime::parse_from_rfc3339("2026-11-18T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let hi = chrono::DateTime::parse_from_rfc3339("2026-11-22T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let raw = r#"{
          "items": [
            {
              "id": "ev1",
              "iCalUID": "shared-uid",
              "status": "confirmed",
              "summary": "Standup",
              "start": {"dateTime": "2026-11-19T10:00:00+08:00"},
              "end": {"dateTime": "2026-11-19T10:30:00+08:00"}
            },
            {
              "id": "ev1-dup",
              "iCalUID": "shared-uid",
              "status": "confirmed",
              "summary": "Standup",
              "start": {"dateTime": "2026-11-19T10:00:00+08:00"},
              "end": {"dateTime": "2026-11-19T10:30:00+08:00"}
            },
            {
              "id": "ev2",
              "status": "confirmed",
              "summary": "Holiday",
              "start": {"date": "2026-11-20"},
              "end": {"date": "2026-11-21"}
            },
            {
              "id": "ev3",
              "status": "cancelled",
              "summary": "Cancelled",
              "start": {"dateTime": "2026-11-19T12:00:00Z"}
            }
          ]
        }"#;
        let parsed = parse_calendar_events(raw, lo, hi).unwrap();
        assert_eq!(parsed.report.cancelled, 1);
        assert_eq!(parsed.events.len(), 2);
        assert_eq!(parsed.events[0].title, "Standup");
        assert!(!parsed.events[0].all_day);
        assert_eq!(parsed.events[1].title, "Holiday");
        assert!(parsed.events[1].all_day);
        assert!(parsed.events[1].at.contains("全天"));
    }
}
