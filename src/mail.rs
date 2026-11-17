//! 郵件（v0.5.5）：六類信箱的帳號模型、IMAP 收取、正文解析。
//!
//! 輕量化約束：只用 Rust 原生 crate（`imap` + `native-tls` + `mailparse`），
//! 不引 Node/Go/Electron。憑據一律進 Windows 憑據管理器（`keyring`），
//! 帳號清單隻存「信箱 + 型別 + 是否監控」，不存密碼。
//!
//! 收信優先 `Session::idle()` 長連線；伺服器不支援 IDLE 時退回輪詢（60/120/300 秒）。
//! 正文只在記憶體裡解析，不落盤、不快取。

use std::collections::HashMap;
#[cfg(windows)]
#[path="mail_fetch.rs"]
pub(crate) mod fetch;

pub const SERVICE: &str = "Daozai.Mail";
pub const MICROSOFT_UNSUPPORTED: &str = "Outlook、Hotmail、Live 和 Microsoft 365 雲信箱需要 OAuth 授權，島仔暫不支援密碼直連。";

// ---------------------------------------------------------------- 信箱型別

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Qq,
    Mail163,
    Mail126,
    Exmail,
    ICloud,
    Gmail,
    Microsoft,
    Custom,
}

impl Default for Provider {
    fn default() -> Self {
        Provider::Gmail
    }
}

impl Provider {
    pub const ALL: [Provider; 3] = [
        Provider::Gmail,
        Provider::ICloud,
        Provider::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Provider::Qq => "qq",
            Provider::Mail163 => "163",
            Provider::Mail126 => "126",
            Provider::Exmail => "exmail",
            Provider::ICloud => "icloud",
            Provider::Gmail => "gmail",
            Provider::Microsoft => "microsoft",
            Provider::Custom => "custom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Provider::Qq => "QQ 信箱",
            Provider::Mail163 => "163 信箱",
            Provider::Mail126 => "126 信箱",
            Provider::Exmail => "騰訊企業郵",
            Provider::ICloud => "iCloud 信箱",
            Provider::Gmail => "Gmail",
            Provider::Microsoft => "Outlook / Hotmail（暫不支援）",
            Provider::Custom => "自定義 IMAP",
        }
    }

    pub fn host(self) -> &'static str {
        match self {
            Provider::Qq => "imap.qq.com",
            Provider::Mail163 => "imap.163.com",
            Provider::Mail126 => "imap.126.com",
            Provider::Exmail => "imap.exmail.qq.com",
            Provider::ICloud => "imap.mail.me.com",
            Provider::Gmail => "imap.gmail.com",
            Provider::Microsoft => "outlook.office365.com",
            Provider::Custom => "",
        }
    }

    pub fn port(self) -> u16 {
        993
    }

    /// 163 / 126 在 LOGIN 之前必須發 RFC 2971 的 `ID`，否則會被判 Unsafe Login。
    pub fn needs_id(self) -> bool {
        matches!(self, Provider::Mail163 | Provider::Mail126)
    }

    /// 設定介面的操作指引。
    pub fn hint(self) -> &'static str {
        match self {
            Provider::Microsoft => MICROSOFT_UNSUPPORTED,
            Provider::Custom => "向學校或單位管理員確認 IMAP 主機、TLS 埠及使用者名稱。僅支援連線即加密的 IMAP over TLS（通常 993）與 LOGIN/應用專用密碼。嚴格校驗證書；不支援明文、STARTTLS、OAuth2 或網頁登入/SSO。修改伺服器或使用者名稱必須重新輸入該目標的授權碼。",
            Provider::Qq => "1. 登入 QQ 信箱網頁版，點右上角「設定」→ 左側「帳號與安全」。\n2. 在新頁面選「安全設定」，向下找到「POP3/IMAP/SMTP/Exchange/CardDAV 服務」；未開啟時先按提示開啟。\n3. 點「生成授權碼」，完成驗證後複製到下方。這裡填寫授權碼，不是 QQ 密碼或信箱獨立密碼。",
            Provider::Mail163 | Provider::Mail126 => "1. 登入對應的網易信箱網頁版，點頂部「設定」→「POP3/SMTP/IMAP」。\n2. 確認「IMAP/SMTP 服務」已開啟；首次開啟時按提示完成驗證。\n3. 在「授權密碼管理」中點「新增授權密碼」，完成驗證後複製到下方。填寫客戶端授權碼，不是信箱登入密碼。",
            Provider::Exmail => "1. 登入騰訊企業郵網頁版，在「設定」→「收發信設定」檢查 IMAP/SMTP 服務。\n2. 常見網頁版在「設定」→「信箱繫結」→「客戶端專用密碼」→「生成新密碼」獲取密碼；啟用安全登入後須使用專用密碼。\n3. 企業版本和管理員策略可能不同。找不到入口或服務被禁用時，請讓企業管理員確認客戶端接入方式，不要照搬 QQ 信箱的路徑。",
            Provider::ICloud => "1. 登入 Apple 賬戶頁面，進入「登入和安全」→「App 專用密碼」。\n2. 選擇「生成 App 專用密碼」，按提示填寫名稱並完成驗證；賬戶需已開啟雙重認證。\n3. 信箱欄填寫完整的 iCloud 信箱地址，下方填寫剛生成的 App 專用密碼，不是 Apple 賬戶密碼。",
            Provider::Gmail => "1. 登入 Google 賬戶，進入「安全性與登入」→「您的 Google 帳號登入選項」→「開啟兩步驗證」，按提示完成設定。\n2. 開啟下方「應用專用密碼」頁面，輸入應用名稱（例如：島仔）並建立，複製生成的 16 位應用專用密碼。\n3. 信箱欄填完整 Gmail 地址，下方填應用專用密碼，不是 Google 登入密碼。若沒有建立入口，請檢查組織帳號限制、高階保護或僅使用安全金鑰的驗證設定。",
        }
    }

    pub fn setup_link(self) -> (&'static str, &'static str) {
        match self {
            Provider::Qq => ("開啟 QQ 信箱網頁版", "https://mail.qq.com/"),
            Provider::Mail163 => ("開啟 163 信箱網頁版", "https://mail.163.com/"),
            Provider::Mail126 => ("開啟 126 信箱網頁版", "https://mail.126.com/"),
            Provider::Exmail => ("開啟騰訊企業郵網頁版", "https://exmail.qq.com/"),
            Provider::ICloud => ("開啟 Apple 賬戶", "https://account.apple.com/"),
            Provider::Gmail => ("開啟應用專用密碼", "https://myaccount.google.com/apppasswords"),
            Provider::Microsoft => ("開啟 Outlook 網頁版", "https://outlook.live.com/mail/"),
            Provider::Custom => ("", ""),
        }
    }

    pub fn parse(key: &str) -> Self {
        match key {
            "163" => Provider::Mail163,
            "126" => Provider::Mail126,
            "exmail" => Provider::Exmail,
            "icloud" => Provider::ICloud,
            "gmail" => Provider::Gmail,
            "microsoft" => Provider::Microsoft,
            "custom" => Provider::Custom,
            "qq" => Provider::Qq,
            _ => Provider::Gmail,
        }
    }
}

// ---------------------------------------------------------------- 帳號與狀態

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub email: String,
    pub provider: Provider,
    pub custom: Option<CustomImap>,
    /// 是否監控這個帳號（設定裡可逐個開關）。
    pub enabled: bool,
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct CustomImap { pub host:String, pub port:u16, pub username:String }
impl Default for CustomImap {fn default()->Self{Self{host:String::new(),port:993,username:String::new()}}}
impl Account {
    pub fn endpoint(&self)->Result<(String,u16,String),String>{
        let email=self.email.trim().to_ascii_lowercase();
        let domain=email.rsplit_once('@').map(|(_,domain)|domain).unwrap_or("");
        if self.provider==Provider::Microsoft||matches!(domain,"outlook.com"|"hotmail.com"|"live.com"|"msn.com"){
            return Err(MICROSOFT_UNSUPPORTED.into());
        }
        if self.provider!=Provider::Custom {return Ok((self.provider.host().into(),self.provider.port(),self.email.clone()));}
        let c=self.custom.as_ref().ok_or("請填寫自定義 IMAP 連線配置")?;
        let host=c.host.trim().to_ascii_lowercase();
        if matches!(host.as_str(),"outlook.office365.com"|"imap-mail.outlook.com"|"outlook.live.com"|"imap.outlook.com"){
            return Err(MICROSOFT_UNSUPPORTED.into());
        }
        if host.is_empty() || host.len()>253 || !host.split('.').all(|label|!label.is_empty() && label.len()<=63 && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')) {
            return Err("IMAP 主機必須是域名或 IPv4 地址，不要填寫網址、埠、路徑或帳號".into());
        }
        if c.port==0 || c.port==143{return Err("僅支援 IMAP over TLS（通常 993），不支援 143/STARTTLS 或明文連線".into());}
        let user=if c.username.trim().is_empty(){self.email.trim()}else{c.username.trim()};
        if user.is_empty() || user.len()>256 || user.chars().any(char::is_control){return Err("IMAP 使用者名稱無效".into());}
        Ok((host,c.port,user.into()))
    }
    /// Custom secrets are bound to the TLS endpoint and exact login identity.
    pub fn credential_key(&self)->String{
        if self.provider!=Provider::Custom{return key(self);}
        let identity=match self.endpoint(){Ok((host,port,user))=>format!("{}\0{host}\0{port}\0{user}",key(self)),Err(_)=>"invalid-custom-endpoint".into()};
        let digest=ring::digest::digest(&ring::digest::SHA256,identity.as_bytes());
        format!("custom-{}",digest.as_ref().iter().map(|b|format!("{b:02x}")).collect::<String>())
    }
}

pub fn key(account: &Account) -> String {
    account.email.trim().to_ascii_lowercase()
}

fn accounts_path() -> std::path::PathBuf {
    crate::model::config_dir().join("mail-accounts.json")
}

pub fn load_accounts() -> Vec<Account> {
    match std::fs::read(accounts_path()) {
        Ok(raw) => decode_accounts(&raw),
        Err(_) => Vec::new(),
    }
}

/// 純解析：帳號清單裡只有信箱、型別、開關，沒有任何密碼欄位。
pub fn decode_accounts(raw: &[u8]) -> Vec<Account> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    json.as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let email = a.get("email")?.as_str()?.trim().to_string();
            if email.is_empty() {
                return None;
            }
            Some(Account {
                custom:a.get("custom").and_then(|c|Some(CustomImap{host:c.get("host")?.as_str()?.into(),port:u16::try_from(c.get("port")?.as_u64()?).ok()?,username:c.get("username").and_then(|u|u.as_str()).unwrap_or("").into()})),
                provider: Provider::parse(a.get("provider").and_then(|p| p.as_str()).unwrap_or("gmail")),
                enabled: a.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                email,
            })
        })
        .collect()
}

pub fn encode_accounts(items: &[Account]) -> Vec<u8> {
    let list: Vec<_> = items
        .iter()
        .map(|a| serde_json::json!({"email": a.email, "provider": a.provider.key(), "enabled": a.enabled,"custom":a.custom.as_ref().map(|c|serde_json::json!({"host":c.host,"port":c.port,"username":c.username}))}))
        .collect();
    serde_json::to_vec(&list).unwrap_or_default()
}

pub fn save_accounts(items: &[Account]) {
    let data = encode_accounts(items);
    if data.is_empty() {
        return;
    }
    let path = accounts_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = crate::model::atomic_write(&path, &data);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Connecting,
    Ready,
    /// 授權碼缺失或失效。
    Auth(String),
    Failed(String),
}

impl Status {
    pub fn label(&self) -> String {
        match self {
            Status::Idle => "未連線".into(),
            Status::Connecting => "連線中…".into(),
            Status::Ready => "已連線".into(),
            Status::Auth(msg) => format!("需要授權碼：{msg}"),
            Status::Failed(msg) => format!("連線失敗：{msg}"),
        }
    }
    pub fn ok(&self) -> bool {
        matches!(self, Status::Ready)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub account: String,
    pub from: String,
    pub subject: String,
    /// 本地時間 `MM-DD HH:MM`
    pub time: String,
    pub code: Option<String>,
    /// IMAP INTERNALDATE, used only to bound code display age.
    pub received_unix: Option<i64>,
    /// 完整純文字正文，保留分段，僅儲存在記憶體中。
    pub summary: std::sync::Arc<str>,
    /// HTML 原文（無 HTML 版本時為空）。排版層據此還原標題、列表、引用等結構；
    /// `summary` 只作為純文字郵件的兜底，兩者都只在記憶體中儲存。
    pub html: std::sync::Arc<str>,
    pub links: Vec<Link>,
    /// 會議邀請（來自 .ics / text/calendar），沒有就是 `None`。
    pub meeting: Option<Meeting>,
    pub uid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub label: String,
    pub url: String,
}

/// 僅把有效網頁連結交給預設瀏覽器。
pub fn web_url(raw:&str)->Option<String> {
    let parsed=url::Url::parse(raw.trim()).ok()?;
    if !matches!(parsed.scheme(),"https"|"http") || parsed.host_str().is_none(){return None;}
    Some(parsed.into())
}

pub fn links_of(parsed:&Parsed)->Vec<Link> {
    use std::sync::OnceLock;
    static ANCHOR:OnceLock<regex::Regex>=OnceLock::new();
    static URL:OnceLock<regex::Regex>=OnceLock::new();
    let anchor=ANCHOR.get_or_init(||regex::Regex::new(r#"(?is)<a\b[^>]*?\s+href\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))[^>]*>(.*?)</a\s*>"#).unwrap());
    let url=URL.get_or_init(||regex::Regex::new(r#"(?i)https?://[^\s<>"'，。；！？、]+"#).unwrap());
    let mut links=Vec::new();let mut seen=std::collections::HashSet::new();
    let mut add=|raw:&str,label:String| {
        let decoded=otp::html_to_text(raw);
        if let Some(url)=web_url(decoded.trim()) {
            if seen.insert(url.clone()) {links.push(Link{label:if label.trim().is_empty(){url.clone()}else{label.trim().into()},url});}
        }
    };
    for cap in anchor.captures_iter(&parsed.html) {
        if let Some(href)=cap.get(1).or_else(||cap.get(2)).or_else(||cap.get(3)) {
            add(href.as_str(),otp::html_to_text(&cap[4]));
        }
    }
    for text in [&parsed.text,&otp::html_to_text(&parsed.html)] {
        for hit in url.find_iter(text) {
            let mut target=hit.as_str().trim_end_matches(['.',',',';',':','!','，','。','；','！','？','）','】','》']);
            for (open,close) in [('(',')'),('[',']'),('{','}')] {
                while target.ends_with(close) && target.chars().filter(|c|*c==close).count()>target.chars().filter(|c|*c==open).count(){target=&target[..target.len()-1];}
            }
            add(target,target.into());
        }
    }
    links
}

/// 會議邀請裡介面和提醒要用的欄位。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meeting {
    pub all_day: bool,
    pub title: String,
    /// 開始時間（Unix 秒，本地時區解釋）。
    pub start: i64,
    /// 來源提供的結束時間；缺失或無效時不推測時長。
    pub end: Option<i64>,
    /// 本地時間 `MM-DD HH:MM`，直接顯示。
    pub at: String,
    /// iCalendar 的 `UID`；郵件和日曆兩條來源靠它 + 開始時間去重。
    pub uid: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Status { account: String, status: Status },
    Mail { item: Item, new: bool },
    Test { account: String, ok: bool, note: String },
    Managed(crate::mail_manage::Completed),
}

// ---------------------------------------------------------------- 正文解析

pub struct Parsed {
    pub from: String,
    pub subject: String,
    /// text/plain 正文（沒有則空）。
    pub text: String,
    /// text/html 正文轉成的純文字（沒有則空）。
    pub html: String,
    /// text/calendar 正文（.ics 原文，沒有則空）。
    pub calendar: String,
    pub emphasized: Vec<String>,
}

fn header(mail: &mailparse::ParsedMail, name: &str) -> String {
    mail.headers
        .iter()
        .find(|h| h.get_key().eq_ignore_ascii_case(name))
        .map(|h| h.get_value().trim().to_string())
        .unwrap_or_default()
}

fn display_addr(raw: &str) -> String {
    mailparse::addrparse(raw)
        .ok()
        .and_then(|list| {
            list.iter().find_map(|a| match a {
                mailparse::MailAddr::Single(s) => Some(s.display_name.clone().unwrap_or_else(|| s.addr.clone())),
                mailparse::MailAddr::Group(g) => g.addrs.first().map(|s| s.display_name.clone().unwrap_or_else(|| s.addr.clone())),
            })
        })
        .unwrap_or_else(|| raw.trim().trim_matches('"').to_string())
}

fn collect(mail: &mailparse::ParsedMail, plain: &mut String, html: &mut String, calendar: &mut String) {
    if mail.subparts.is_empty() {
        let ct = mail.ctype.mimetype.to_ascii_lowercase();
        if ct == "text/plain" {
            if let Ok(body) = mail.get_body() {
                plain.push_str(&body);
                plain.push('\n');
            }
        } else if ct == "text/html" {
            if let Ok(body) = mail.get_body() {
                html.push_str(&body);
                html.push('\n');
            }
        } else if ct == "text/calendar" || ct == "application/ics" {
            if let Ok(body) = mail.get_body() {
                calendar.push_str(&body);
                calendar.push('\n');
            }
        }
    } else {
        for part in &mail.subparts {
            collect(part, plain, html, calendar);
        }
    }
}

/// 解析一封原始郵件（頭部 + 正文），只保留介面上要用的欄位。
pub fn parse_message(raw: &[u8]) -> Parsed {
    let Ok(mail) = mailparse::parse_mail(raw) else {
        return Parsed {
            from: String::new(),
            subject: String::new(),
            text: String::from_utf8_lossy(raw).into_owned(),
            html: String::new(),
            calendar: String::new(),
            emphasized: Vec::new(),
        };
    };
    let subject = header(&mail, "Subject");
    let from = display_addr(&header(&mail, "From"));
    let (mut plain, mut html, mut calendar) = (String::new(), String::new(), String::new());
    collect(&mail, &mut plain, &mut html, &mut calendar);
    let emphasized = if plain.trim().is_empty() {
        otp::emphasized(&html)
    } else {
        Vec::new()
    };
    Parsed { from, subject, text: plain, html, calendar, emphasized }
}

/// 優先純文字；HTML 原文另外保留給閱讀介面排版。
pub fn body_of(parsed:&Parsed)->String {
    let body = if !parsed.text.trim().is_empty() {
        parsed.text.clone()
    } else if !parsed.html.trim().is_empty() {
        otp::html_to_text(&parsed.html)
    } else {
        String::new()
    };
    body.replace("\r\n","\n").trim().to_string()
}

/// 緊湊場景使用的短摘要。
pub fn summary_of(parsed: &Parsed) -> String {
    let body=body_of(parsed);
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = flat.chars().take(200).collect();
    if flat.chars().count() > 200 {
        out.push('…');
    }
    out
}

/// 從解析結果裡提驗證碼：先純文字，純文字沒有再看 HTML 文字。
pub fn code_of(parsed: &Parsed) -> Option<String> {
    if let Some(c) = otp::extract(&parsed.subject, &parsed.text, &parsed.emphasized) {
        return Some(c.text);
    }
    if !parsed.html.trim().is_empty() {
        let text = otp::html_to_text(&parsed.html);
        if let Some(c) = otp::extract(&parsed.subject, &text, &parsed.emphasized) {
            return Some(c.text);
        }
    }
    None
}

/// 從 .ics 文字里取第一個 VEVENT 的標題與開始時間（郵件正文用）。
pub fn meeting_of(calendar: &str) -> Option<Meeting> {
    meetings_of(calendar).into_iter().next()
}

/// 從一段 iCalendar 裡取出所有帶時刻的 VEVENT。
///
/// CalDAV 的 `calendar-query` 一次會返回多個事件，郵件裡的 `.ics` 通常只有一個；
/// 兩者共用這一套解析；支援帶時刻和全天日期，沒有有效 DTSTART 就跳過。
pub fn meetings_of(calendar: &str) -> Vec<Meeting> {
    // RFC 5545 折行：以空格/製表符開頭的行是上一行的續行。
    let mut lines: Vec<String> = Vec::new();
    for raw in calendar.lines() {
        let line = raw.trim_end_matches('\r');
        if (line.starts_with(' ') || line.starts_with('\t')) && !lines.is_empty() {
            if let Some(last) = lines.last_mut() {
                last.push_str(&line[1..]);
            }
        } else {
            lines.push(line.to_string());
        }
    }

    let mut out = Vec::new();
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut in_event = false;
    let mut cancelled = false;
    let mut nested = 0usize;
    for line in &lines {
        let up = line.to_ascii_uppercase();
        if up=="BEGIN:VCALENDAR" {cancelled=false;}
        if !in_event && up=="METHOD:CANCEL" {cancelled=true;}
        if up=="BEGIN:VEVENT" {
            in_event = true;
            nested=0;
            fields.clear();
            continue;
        }
        if up=="END:VEVENT" {
            if in_event && !cancelled {
                if let Some(m) = event_of(&fields) {
                    out.push(m);
                }
            }
            in_event = false;
            continue;
        }
        if !in_event {
            continue;
        }
        if up.starts_with("BEGIN:"){nested+=1;continue;}
        if up.starts_with("END:"){nested=nested.saturating_sub(1);continue;}
        if nested>0 {continue;}
        let Some((head, value)) = line.split_once(':') else { continue };
        let name = head.split(';').next().unwrap_or("").to_ascii_uppercase();
        if name == "DTSTART" || name == "DTEND" {
            if let Some(tz) = head.split(';').skip(1).find_map(|p| p.split_once('=').filter(|(k, _)| k.eq_ignore_ascii_case("TZID")).map(|(_, v)| v.trim_matches('"'))) {
                fields.push((format!("{name}-TZID"), tz.to_string()));
            }
        }
        fields.push((name, value.trim().to_string()));
    }
    out
}

fn event_of(fields: &[(String, String)]) -> Option<Meeting> {
    use chrono::TimeZone;
    let get = |name: &str| fields.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    if get("STATUS").is_some_and(|v|v.eq_ignore_ascii_case("CANCELLED")){return None;}
    let parse_time = |name: &str| -> Option<chrono::DateTime<chrono::Local>> {
        let value = get(name)?;
        Some(if let Some(s) = value.strip_suffix('Z') {
            chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?.and_utc().with_timezone(&chrono::Local)
        } else {
            if !value.contains('T') {
                return chrono::NaiveDate::parse_from_str(&value,"%Y%m%d").ok()?.and_hms_opt(0,0,0)?.and_local_timezone(chrono::Local).earliest();
            }
            let n = chrono::NaiveDateTime::parse_from_str(&value, "%Y%m%dT%H%M%S").ok()?;
            if let Some(tz) = get(&format!("{name}-TZID")) {
                let zone: chrono_tz::Tz = tz.parse().ok()?;
                zone.from_local_datetime(&n).single()?.with_timezone(&chrono::Local)
            } else {
                chrono::Local.from_local_datetime(&n).single()?
            }
        })
    };
    let naive = parse_time("DTSTART")?;
    let all_day=get("DTSTART").is_some_and(|value|!value.contains('T'));
    let end = parse_time("DTEND").map(|at|at.timestamp()).filter(|end|*end>naive.timestamp())
        .or_else(||all_day.then(||naive.date_naive().succ_opt()?.and_hms_opt(0,0,0)?.and_local_timezone(chrono::Local).earliest().map(|at|at.timestamp())).flatten());
    let title = get("SUMMARY").map(|v| ics_text(&v)).filter(|t| !t.is_empty()).unwrap_or_else(|| "會議".to_string());
    Some(Meeting {
        all_day,
        title,
        start: naive.timestamp(),
        end,
        at: naive.format(if all_day {"%m-%d 全天"}else{"%m-%d %H:%M"}).to_string(),
        uid: get("UID").unwrap_or_default(),
    })
}

/// iCalendar 文字轉義：`\n` / `\,` / `\;` / `\\`。
fn ics_text(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') | Some('N') => out.push(' '),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

use crate::otp;

// ---------------------------------------------------------------- 憑據

/// 密碼從網頁或文件複製時常帶換行與首尾空白，儲存和連線前一律去掉。
pub fn clean_secret(raw: &str) -> String {
    raw.chars().filter(|c| !matches!(c, '\r' | '\n')).collect::<String>().trim().to_string()
}

#[cfg(windows)]
pub mod secrets {
    /// 憑據只進 Windows 憑據管理器；鍵是信箱地址，值是授權碼。
    pub fn save(email: &str, password: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(super::SERVICE, &email.to_ascii_lowercase())
            .map_err(|e| format!("憑據管理器不可用：{e}"))?;
        let password = super::clean_secret(password);
        entry.set_password(&password).map_err(|e| format!("儲存授權碼失敗：{e}"))
    }

    pub fn load(email: &str) -> Result<String, String> {
        let entry = keyring::Entry::new(super::SERVICE, &email.to_ascii_lowercase())
            .map_err(|e| format!("憑據管理器不可用：{e}"))?;
        entry.get_password().map_err(|_| "沒有儲存授權碼".to_string())
    }

    pub fn delete(email: &str) {
        if let Ok(entry) = keyring::Entry::new(super::SERVICE, &email.to_ascii_lowercase()) {
            let _ = entry.delete_credential();
        }
    }
}

#[cfg(not(windows))]
pub mod secrets {
    pub fn save(_email: &str, _password: &str) -> Result<(), String> {
        Err("僅 Windows 支援憑據管理器".into())
    }
    pub fn load(_email: &str) -> Result<String, String> {
        Err("僅 Windows 支援憑據管理器".into())
    }
    pub fn delete(_email: &str) {}
}

// ---------------------------------------------------------------- 收信服務

#[cfg(windows)]
pub(crate) mod net {
    //! TLS 連線 + 163/126 的 pre-login `ID` 握手。
    //!
    //! `imap` 2.4 只在已登入的 `Session` 上暴露 `run_command_and_check_ok`，
    //! 而網易要求 greeting → ID → LOGIN 的順序，所以這裡手工讀問候、發 ID，
    //! 再把「已經讀掉的開頭位元組」原樣交回給 imap。

    use super::Account;
    use imap::extensions::idle::SetReadTimeout;
    use native_tls::{TlsConnector, TlsStream};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    pub type Stream = TlsStream<TcpStream>;

    /// 先吐出預先讀到的問候位元組，再透傳給底層流。
    pub struct Prefixed<S> {
        head: Vec<u8>,
        pos: usize,
        inner: S,
    }

    impl<S> Prefixed<S> {
        fn new(head: Vec<u8>, inner: S) -> Self {
            Self { head, pos: 0, inner }
        }
    }

    impl<S: Read> Read for Prefixed<S> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos < self.head.len() {
                let n = (self.head.len() - self.pos).min(buf.len());
                buf[..n].copy_from_slice(&self.head[self.pos..self.pos + n]);
                self.pos += n;
                Ok(n)
            } else {
                self.inner.read(buf)
            }
        }
    }

    impl<S: Write> Write for Prefixed<S> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.inner.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }

    impl SetReadTimeout for Prefixed<Stream> {
        fn set_read_timeout(&mut self, timeout: Option<Duration>) -> imap::Result<()> {
            self.inner
                .get_ref()
                // imap 結束 IDLE 時傳入 None；恢復普通命令的超時，避免無限阻塞。
                .set_read_timeout(timeout.or(Some(Duration::from_secs(20))))
                .map_err(imap::Error::Io)
        }
    }

    fn read_line(stream: &mut Stream, buf: &mut Vec<u8>) -> Result<(), String> {
        let mut byte = [0u8; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => return Err("伺服器關閉了連線".into()),
                Ok(_) => {
                    buf.push(byte[0]);
                    if buf.ends_with(b"\r\n") {
                        return Ok(());
                    }
                    if buf.len() > 8192 {
                        return Err("伺服器響應過長".into());
                    }
                }
                Err(e) => return Err(format!("讀取失敗：{e}")),
            }
        }
    }

    pub fn connect(account: &Account, password: &str) -> Result<imap::Session<Prefixed<Stream>>, String> {
        let (host,port,username)=account.endpoint()?;
        let tls = TlsConnector::builder()
            .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
            .build()
            .map_err(|e| format!("TLS 初始化失敗：{e}"))?;
        let tcp = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("無法連線 {host}：{e}"))?;
        let timeout = Some(Duration::from_secs(20));
        let _ = tcp.set_read_timeout(timeout);
        let _ = tcp.set_write_timeout(timeout);
        let mut stream = tls.connect(&host, tcp).map_err(|e| format!("TLS 握手失敗：{e}"))?;

        // 1) 讀問候，直到 "* OK" / "* PREAUTH"
        let mut greeting = Vec::new();
        loop {
            let mut line = Vec::new();
            read_line(&mut stream, &mut line)?;
            let upper = String::from_utf8_lossy(&line).to_ascii_uppercase();
            greeting.extend_from_slice(&line);
            if upper.starts_with("* BYE") {
                return Err("伺服器拒絕連線".into());
            }
            if upper.starts_with("* OK") || upper.starts_with("* PREAUTH") {
                break;
            }
            if greeting.len() > 8192 {
                return Err("伺服器問候異常".into());
            }
        }

        // 2) 網易系：LOGIN 之前先發 ID（RFC 2971）
        if account.provider.needs_id() {
            stream
                .write_all(b"A0001 ID (\"name\" \"daozai\" \"version\" \"0.5.5\" \"vendor\" \"daozai\")\r\n")
                .map_err(|e| format!("傳送 ID 失敗：{e}"))?;
            let _ = stream.flush();
            let mut ok = false;
            for _ in 0..20 {
                let mut line = Vec::new();
                read_line(&mut stream, &mut line)?;
                let text = String::from_utf8_lossy(&line).to_string();
                if text.starts_with("A0001 ") {
                    ok = text.to_ascii_uppercase().contains(" OK");
                    break;
                }
            }
            if !ok {
                return Err("伺服器不接受 ID 命令".into());
            }
        }

        // 3) 把問候交回 imap，再正常登入
        let mut client = imap::Client::new(Prefixed::new(greeting, stream));
        client.read_greeting().map_err(|e| format!("伺服器問候異常：{e}"))?;
        let session = client
            .login(username, password)
            .map_err(|(e, _)| login_error(e))?;
        Ok(session)
    }

    pub fn login_error(e: imap::Error) -> String {
        match e {
            imap::Error::No(_) | imap::Error::Bad(_) => "登入被拒絕，請檢查使用者名稱、授權碼和管理員的 IMAP/認證策略（不支援 OAuth-only）".into(),
            imap::Error::Io(_) => "登入時網路連線中斷，請重試".into(),
            _ => "IMAP 登入失敗，請核對伺服器及認證方式".into(),
        }
    }

    /// 測試連線：連上、登入、選 INBOX，然後退出。
    pub fn probe(account: &Account, password: &str) -> Result<(), String> {
        let mut session = connect(account, password)?;
        session.select("INBOX").map_err(|e| format!("無法開啟收件箱：{e}"))?;
        let _ = session.logout();
        Ok(())
    }
}

#[cfg(windows)]
mod service {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Arc;
    use std::time::{Duration,Instant};

    enum Cmd {
        Sync(Vec<Account>, u32),
        Test(Account, String),
    }

    pub struct Service {
        cmd: Sender<Cmd>,
        ev: Receiver<Event>,
        manage: Sender<crate::mail_manage::Request>,
        done: Receiver<crate::mail_manage::Completed>,
    }

    impl Service {
        pub fn start() -> Self {
            let (ev_tx, ev_rx) = channel();
            let (cmd_tx, cmd_rx) = channel();
            let _ = std::thread::Builder::new()
                .name("daozai-mail".into())
                .spawn(move || supervisor(cmd_rx, ev_tx));
            let (manage,done)=crate::mail_manage::start();
            Self { cmd: cmd_tx, ev: ev_rx,manage,done }
        }

        pub fn sync(&self, accounts: &[Account], interval_secs: u32) {
            let _ = self.cmd.send(Cmd::Sync(accounts.to_vec(), interval_secs));
        }

        pub fn test(&self, account: Account, password: String) {
            let _ = self.cmd.send(Cmd::Test(account, password));
        }

        pub fn manage(&self,request:crate::mail_manage::Request){let _=self.manage.send(request);}

        pub fn try_recv(&self) -> Option<Event> {
            self.done.try_recv().ok().map(Event::Managed).or_else(||self.ev.try_recv().ok())
        }
    }

    fn supervisor(cmd_rx: Receiver<Cmd>, ev: Sender<Event>) {
        let mut workers: HashMap<String, (Arc<AtomicBool>, std::thread::JoinHandle<()>,Account)> = HashMap::new();
        let interval = Arc::new(AtomicU64::new(120));
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                Cmd::Sync(accounts, secs) => {
                    interval.store(secs.clamp(30, 900) as u64, Ordering::Relaxed);
                    let wanted: HashMap<String, Account> = accounts
                        .into_iter()
                        .filter(|a| a.enabled)
                        .map(|a| (key(&a), a))
                        .collect();
                    let drop: Vec<String> = workers
                        .keys()
                        .filter(|k| wanted.get(*k)!=workers.get(*k).map(|worker|&worker.2))
                        .cloned()
                        .collect();
                    for k in drop {
                        if let Some((flag, _, _)) = workers.remove(&k) {
                            flag.store(true, Ordering::Relaxed);
                        }
                    }
                    for (k, account) in wanted {
                        if workers.contains_key(&k) {
                            continue;
                        }
                        let stop = Arc::new(AtomicBool::new(false));
                        let flag = stop.clone();
                        let ev = ev.clone();
                        let iv = interval.clone();
                        let name = "daozai-mail".to_string();
                        let running_account=account.clone();
                        if let Ok(handle) = std::thread::Builder::new()
                            .name(name)
                            .spawn(move || worker(account, flag, iv, ev))
                        {
                            workers.insert(k, (stop, handle,running_account));
                        }
                    }
                }
                Cmd::Test(account, password) => {
                    let ev = ev.clone();
                    let _ = std::thread::Builder::new()
                        .name("daozai-mail-test".into())
                        .spawn(move || {
                            let result = net::probe(&account, &password);
                            let _ = ev.send(Event::Test {
                                account: account.email,
                                ok: result.is_ok(),
                                note: result.err().unwrap_or_else(|| "連線成功".into()),
                            });
                        });
                }
            }
        }
        for (_, (flag, _, _)) in workers {
            flag.store(true, Ordering::Relaxed);
        }
    }

    fn sleep_stop(stop: &AtomicBool, secs: u64) -> bool {
        sleep_until_stop(stop,Instant::now()+Duration::from_secs(secs))
    }

    fn sleep_until_stop(stop:&AtomicBool,deadline:Instant)->bool{
        while !stop.load(Ordering::Relaxed){
            let left=deadline.saturating_duration_since(Instant::now());
            if left.is_zero(){return false;}
            std::thread::sleep(left.min(Duration::from_millis(500)));
        }
        true
    }

    // 只在輪詢分支保活。各類信箱都在同一個檢查截止點按 UID 查新郵件。
    fn wait_for_poll<S:std::io::Read+std::io::Write>(
        session:&mut imap::Session<S>,start:Instant,secs:u64,allow_noop:&mut bool,mut wait_until:impl FnMut(Instant)->bool,
    )->imap::Result<()>{
        const KEEPALIVE:Duration=Duration::from_secs(45);
        let deadline=start+Duration::from_secs(secs);
        let mut previous=start;
        loop{
            let wake=if *allow_noop{(previous+KEEPALIVE).min(deadline)}else{deadline};
            if wait_until(wake) || !*allow_noop{return Ok(());}
            // NOOP 可攜帶 EXISTS、EXPUNGE、FETCH 等通知；收信以隨後的 UID 查詢為準。
            match session.run_command_and_check_ok("NOOP"){
                Ok(())=>{},
                // 伺服器明確拒絕保活時，保留當前連線，按原間隔用 UID 查詢。
                Err(imap::Error::No(_)|imap::Error::Bad(_))=>*allow_noop=false,
                Err(error)=>return Err(error),
            }
            let now=Instant::now();
            if wake==deadline || now>=deadline{return Ok(());}
            previous=wake.max(now);
        }
    }

    fn next_retry_delay(base:u64,failures:&mut u32)->u64 {
        let multiplier=2u64.pow((*failures).min(6));
        let delay=base.saturating_mul(multiplier).min(60);
        *failures=failures.saturating_add(1);
        delay
    }

    fn wait_for_change<T: std::io::Read + std::io::Write + imap::extensions::idle::SetReadTimeout>(session: &mut imap::Session<T>) -> imap::Result<()> {
        // 優先響應伺服器推送；定期補查，避免服務端漏推送時長時間沒有提醒。
        session.idle()?.wait_with_timeout(Duration::from_secs(10)).map(|_| ())
    }

    #[test]
    fn idle_without_push_returns_for_a_fallback_check() {
        use std::io::{BufRead,Write};
        for push in [true,false] {
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address=listener.local_addr().unwrap();
        let server=std::thread::spawn(move||{
            let (mut socket,_)=listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(14))).unwrap();
            let mut reader=std::io::BufReader::new(socket.try_clone().unwrap());
            write!(socket,"* OK ready\r\n").unwrap();
            let mut line=String::new();reader.read_line(&mut line).unwrap();
            write!(socket,"{} OK logged in\r\n",line.split_whitespace().next().unwrap()).unwrap();
            line.clear();reader.read_line(&mut line).unwrap();
            let tag=line.split_whitespace().next().unwrap().to_string();
            assert!(line.contains("IDLE"));write!(socket,"+ idling\r\n").unwrap();
            if push {write!(socket,"* 1 EXISTS\r\n").unwrap();}
            line.clear();
            if reader.read_line(&mut line).is_err(){return false;}
            assert_eq!(line,"DONE\r\n");
            write!(socket,"{tag} OK idle ended\r\n").unwrap();true
        });
        let mut client=imap::Client::new(std::net::TcpStream::connect(address).unwrap());
        client.read_greeting().unwrap();
        let mut session=client.login("test","test").unwrap();
        let start=std::time::Instant::now();
        let result=wait_for_change(&mut session);
        assert!(server.join().unwrap(),"服務端未推送時，必須在 14 秒內主動結束等待並補查");
        result.unwrap();
        if push {assert!(start.elapsed()<Duration::from_secs(2),"有推送時必須立即處理，不能等待檢查間隔");}
        }
    }

    fn worker(account: Account, stop: Arc<AtomicBool>, interval: Arc<AtomicU64>, ev: Sender<Event>) {
        if let Err(error)=account.endpoint(){
            let _=ev.send(Event::Status{account:account.email,status:Status::Failed(error)});return;
        }
        let mut last_uid = None;
        let mut last_validity=None;
        let mut failures=0;
        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let _ = ev.send(Event::Status {
                account: account.email.clone(),
                status: Status::Connecting,
            });
            let password = match secrets::load(&account.credential_key()) {
                Ok(p) => p,
                Err(e) => {
                    let _ = ev.send(Event::Status {
                        account: account.email.clone(),
                        status: Status::Auth(e),
                    });
                    if sleep_stop(&stop, next_retry_delay(60,&mut failures)) {
                        return;
                    }
                    continue;
                }
            };
            let mut session = match net::connect(&account, &password) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ev.send(Event::Status {
                        account: account.email.clone(),
                        status: Status::Failed(e),
                    });
                    if sleep_stop(&stop, next_retry_delay(30,&mut failures)) {
                        return;
                    }
                    continue;
                }
            };
            if stop.load(Ordering::Relaxed){let _=session.logout();return;}
            let Ok(mailbox)=session.select("INBOX") else {
                let _ = ev.send(Event::Status {
                    account: account.email.clone(),
                    status: Status::Failed("無法開啟收件箱".into()),
                });
                if sleep_stop(&stop, next_retry_delay(30,&mut failures)) {
                    return;
                }
                continue;
            };
            if mailbox.uid_validity.is_none() || mailbox.uid_validity!=last_validity{last_uid=None;}
            last_validity=mailbox.uid_validity;
            let connected_at=std::time::Instant::now();
            match monitor(&mut session,&account,&mut last_uid,&stop,&interval,&ev) {
                Ok(())=>return,
                Err(note)=>{let _=ev.send(Event::Status{account:account.email.clone(),status:Status::Failed(note)});}
            }
            if connected_at.elapsed()>=Duration::from_secs(60){failures=0;}
            if sleep_stop(&stop, next_retry_delay(10,&mut failures)) {
                return;
            }
        }
    }

    fn monitor<S:std::io::Read+std::io::Write+imap::extensions::idle::SetReadTimeout>(
        session:&mut imap::Session<S>,account:&Account,last_uid:&mut Option<u32>,
        stop:&AtomicBool,interval:&AtomicU64,ev:&Sender<Event>,
    )->Result<(),String> {
        match last_uid {
            Some(last)=>fetch_new(session,account,last,ev).map_err(|_|"讀取新郵件失敗，正在重連")?,
            None=>{*last_uid=Some(recent(session,account,ev).map_err(|_|"讀取最近郵件失敗，正在重連")?);}
        }
        let mut allow_noop=true;
        let mut idle_supported=session.capabilities().map_err(|_|"讀取收信能力失敗，正在重連")?.has_str("IDLE");
        let _=ev.send(Event::Status{account:account.email.clone(),status:Status::Ready});
        loop {
            if stop.load(Ordering::Relaxed){let _=session.logout();return Ok(());}
            let previous=*last_uid;
            let cycle_started=std::time::Instant::now();
            if idle_supported {
                match wait_for_change(session) {
                    Ok(())=>{},
                    // 伺服器明確拒絕推送時，本次連線改為輪詢；重連後重新檢查推送能力。
                    Err(imap::Error::No(_)|imap::Error::Bad(_))=>idle_supported=false,
                    Err(_)=>return Err("郵件推送連線中斷，正在重連".into()),
                }
            }else{
                wait_for_poll(session,cycle_started,interval.load(Ordering::Relaxed),&mut allow_noop,|at|sleep_until_stop(stop,at))
                    .map_err(|_|"郵件連線中斷，正在重連")?;
            }
            if stop.load(Ordering::Relaxed){let _=session.logout();return Ok(());}
            fetch_new(session,account,last_uid.as_mut().unwrap(),ev).map_err(|_|"讀取新郵件失敗，正在重連")?;
            if idle_supported && *last_uid==previous && cycle_started.elapsed()<Duration::from_secs(5) {
                if sleep_stop(stop,5){let _=session.logout();return Ok(());}
                // 冷卻後補讀新郵件，並消費等待期間到達的通知，再進入下一輪推送等待。
                fetch_new(session,account,last_uid.as_mut().unwrap(),ev).map_err(|_|"讀取新郵件失敗，正在重連")?;
            }
        }
    }

    #[cfg(test)]
    mod receiving_tests {
        use super::*;
        use std::{io::{BufRead,Write},net::{TcpListener,TcpStream}};

        #[derive(Clone,Copy)]
        enum Scenario {
            RejectedIdle, SupportedIdle, EmptyIdle, CooldownMail,
            InitialMailbox, DisconnectedIdle, BrokenHistory, Burst, PollKeepalive, BrokenNoop, RefusedNoop,
        }

        fn inbox(scenario:Scenario)->(imap::Session<TcpStream>,Arc<AtomicBool>,std::thread::JoinHandle<Vec<String>>) {
            let listener=TcpListener::bind("127.0.0.1:0").unwrap();
            let address=listener.local_addr().unwrap();
            let stop=Arc::new(AtomicBool::new(false));let server_stop=stop.clone();
            let server=std::thread::spawn(move || {
                let (mut socket,_)=listener.accept().unwrap();
                let timeout=if matches!(scenario,Scenario::EmptyIdle|Scenario::CooldownMail){8}else{2};
                socket.set_read_timeout(Some(Duration::from_secs(timeout))).unwrap();
                let mut reader=std::io::BufReader::new(socket.try_clone().unwrap());
                let mut commands=Vec::new();let mut idle_tag=String::new();let mut idle_started=false;let mut fetches=0;let mut searches=0;
                write!(socket,"* OK ready\r\n").unwrap();
                loop {
                    let mut line=String::new();
                    match reader.read_line(&mut line) {
                        Ok(0)=>break,
                        Ok(_)=>{},
                        Err(error) if error.kind()==std::io::ErrorKind::ConnectionReset=>break,
                        Err(error)=>panic!("模擬收信連線讀取失敗：{error}"),
                    }
                    let line=line.trim_end();commands.push(line.to_string());
                    if line=="DONE" {
                        if matches!(scenario,Scenario::SupportedIdle|Scenario::EmptyIdle|Scenario::CooldownMail){write!(socket,"{idle_tag} OK idle ended\r\n").unwrap();}
                        else{write!(socket,"{idle_tag} BAD not idling\r\n").unwrap();}
                        continue;
                    }
                    let (tag,command)=line.split_once(' ').unwrap();
                    if command.starts_with("LOGIN ") {
                        write!(socket,"{tag} OK login\r\n").unwrap();
                    }else if command.starts_with("UID SEARCH ") {
                        searches+=1;
                        let uids=match scenario {
                            Scenario::EmptyIdle=>"",
                            Scenario::CooldownMail if searches<3=>"",
                            Scenario::RejectedIdle|Scenario::SupportedIdle if !idle_started=>"",
                            Scenario::BrokenHistory|Scenario::InitialMailbox=>"42",
                            Scenario::Burst=>"1 2 3 4 5 6 7",
                            _=>"41",
                        };
                        if uids.is_empty(){write!(socket,"* SEARCH\r\n{tag} OK search\r\n").unwrap();}
                        else{write!(socket,"* SEARCH {uids}\r\n{tag} OK search\r\n").unwrap();}
                    }else if command.starts_with("UID FETCH ") {
                        if matches!(scenario,Scenario::BrokenHistory){write!(socket,"* BYE connection lost\r\n").unwrap();break;}
                        let uid=command.split_whitespace().nth(2).unwrap();
                        let header=format!("From: sender@example.com\r\nSubject: mail-{uid}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n");
                        let body="ordinary message";
                        write!(socket,"* 1 FETCH (UID {uid} BODY[HEADER] {{{}}}\r\n{header} BODY[TEXT] {{{}}}\r\n{body})\r\n{tag} OK fetch\r\n",header.len(),body.len()).unwrap();
                        fetches+=1;
                        if matches!(scenario,Scenario::SupportedIdle|Scenario::CooldownMail) && fetches==2 {server_stop.store(true,Ordering::Relaxed);}
                    }else if command=="CAPABILITY" {
                        let capabilities=if matches!(scenario,Scenario::InitialMailbox){"IMAP4rev1"}else{"IMAP4rev1 IDLE"};
                        write!(socket,"* CAPABILITY {capabilities}\r\n{tag} OK capabilities\r\n").unwrap();
                    }else if command=="IDLE" {
                        if matches!(scenario,Scenario::EmptyIdle) && idle_started {server_stop.store(true,Ordering::Relaxed);}
                        idle_started=true;idle_tag=tag.to_string();
                        if matches!(scenario,Scenario::RejectedIdle){
                            write!(socket,"{tag} NO IDLE unavailable\r\n").unwrap();
                        }else if matches!(scenario,Scenario::SupportedIdle|Scenario::EmptyIdle|Scenario::CooldownMail){
                            write!(socket,"+ idling\r\n* 1 EXISTS\r\n").unwrap();
                        }else{write!(socket,"* BYE connection lost\r\n").unwrap();break;}
                    }else if command=="NOOP" {
                        if matches!(scenario,Scenario::RefusedNoop){write!(socket,"{tag} BAD unavailable\r\n").unwrap();continue;}
                        if matches!(scenario,Scenario::BrokenNoop){write!(socket,"* BYE closing\r\n").unwrap();break;}
                        if matches!(scenario,Scenario::PollKeepalive){
                            write!(socket,"* 1 EXISTS\r\n* 1 FETCH (FLAGS (\\Seen))\r\n").unwrap();
                        }else{server_stop.store(true,Ordering::Relaxed);}
                        write!(socket,"{tag} OK noop\r\n").unwrap();
                    }else if command=="LOGOUT" {
                        write!(socket,"* BYE logout\r\n{tag} OK logout\r\n").unwrap();break;
                    }else{panic!("意外的收信命令：{command}");}
                }
                commands
            });
            let socket=TcpStream::connect(address).unwrap();socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut client=imap::Client::new(socket);client.read_greeting().unwrap();
            (client.login("synthetic","synthetic").unwrap(),stop,server)
        }

        fn account()->Account {Account{provider:Provider::Mail163,email:"test@163.com".into(),enabled:true,custom:None}}

        #[test]
        fn polling_keepalives_do_not_fetch_until_the_configured_deadline(){
            for (secs,steps) in [(60,vec![45,60]),(120,vec![45,90,120]),(300,vec![45,90,135,180,225,270,300])] {
                let (mut session,_,server)=inbox(Scenario::PollKeepalive);
                let start=Instant::now();
                let mut wakes=Vec::new();
                wait_for_poll(&mut session,start,secs,&mut true,|at|{wakes.push((at-start).as_secs());false}).unwrap();
                let (tx,_)=channel();let mut last=40;fetch_new(&mut session,&account(),&mut last,&tx).unwrap();
                session.logout().unwrap();drop(session);
                let commands=server.join().unwrap();assert_eq!(wakes,steps);
                let before_search=commands.iter().skip(1).take_while(|s|!s.contains("UID SEARCH")).collect::<Vec<_>>();
                assert_eq!(before_search.len(),steps.len());assert!(before_search.iter().all(|s|s.ends_with(" NOOP")));
                assert_eq!(commands.iter().filter(|s|s.contains("UID SEARCH")).count(),1);
                assert_eq!(last,41);
            }
        }
        #[test]
        fn rejected_keepalive_keeps_polling_without_repeated_noops(){
            let (mut session,_,server)=inbox(Scenario::RefusedNoop);
            let mut allow_noop=true;
            let (tx,_)=channel();let mut last=40;
            for steps in [vec![45,120],vec![120]]{
                let start=Instant::now();let mut wakes=Vec::new();
                wait_for_poll(&mut session,start,120,&mut allow_noop,|at|{wakes.push((at-start).as_secs());false}).unwrap();
                assert_eq!(wakes,steps);assert!(!allow_noop);
                fetch_new(&mut session,&account(),&mut last,&tx).unwrap();
            }
            session.logout().unwrap();drop(session);let commands=server.join().unwrap();
            assert_eq!(commands.iter().filter(|s|s.ends_with(" NOOP")).count(),1);
            assert_eq!(commands.iter().filter(|s|s.contains("UID SEARCH")).count(),2);assert_eq!(last,41);
        }

        #[test]
        fn polling_stops_before_sending_or_after_a_broken_keepalive(){
            for cancel in [true,false]{
                let (mut session,_,server)=inbox(Scenario::BrokenNoop);
                let result=wait_for_poll(&mut session,Instant::now(),120,&mut true,|_|cancel);
                assert_eq!(result.is_ok(),cancel);
                drop(session);let commands=server.join().unwrap();
                assert_eq!(commands.iter().filter(|s|s.ends_with(" NOOP")).count(),usize::from(!cancel));
                assert!(!commands.iter().any(|s|s.contains("UID SEARCH")));
            }
        }

        #[test]
        fn reconnect_backoff_is_bounded_and_can_reset() {
            let mut failures=0;
            let delays:Vec<_>=(0..7).map(|_|next_retry_delay(10,&mut failures)).collect();
            assert_eq!(delays,vec![10,20,40,60,60,60,60]);
            failures=0;
            assert_eq!(next_retry_delay(30,&mut failures),30);
            assert_eq!(next_retry_delay(30,&mut failures),60);
            assert_eq!(next_retry_delay(60,&mut failures),60);
        }

        #[test]
        fn cooldown_wait_can_be_cancelled() {
            let stop=Arc::new(AtomicBool::new(false));let worker_stop=stop.clone();
            let started=std::time::Instant::now();
            let worker=std::thread::spawn(move || sleep_stop(&worker_stop,30));
            std::thread::sleep(Duration::from_millis(50));stop.store(true,Ordering::Relaxed);
            assert!(worker.join().unwrap());
            assert!(started.elapsed()<Duration::from_secs(2),"停止監聽應打斷後臺等待");
        }

        #[test]
        fn repeated_empty_pushes_are_throttled() {
            let (mut session,stop,server)=inbox(Scenario::EmptyIdle);
            let (ev,rx)=channel();let mut last=Some(40);
            let started=std::time::Instant::now();
            let result=monitor(&mut session,&account(),&mut last,&stop,&AtomicU64::new(120),&ev);
            let elapsed=started.elapsed();drop(session);let commands=server.join().unwrap();
            assert!(result.is_ok(),"{result:?}");assert_eq!(last,Some(40));
            assert!(elapsed>=Duration::from_secs(5),"兩次空推送之間應降頻，實際耗時 {elapsed:?}");
            assert_eq!(commands.iter().filter(|command|command.ends_with(" IDLE")).count(),2);
            assert!(!rx.try_iter().any(|event|matches!(event,Event::Mail{..})));
        }

        #[test]
        fn new_mail_arriving_during_cooldown_is_read_before_idling_again() {
            let (mut session,stop,server)=inbox(Scenario::CooldownMail);
            let (ev,rx)=channel();let mut last=Some(40);
            let result=monitor(&mut session,&account(),&mut last,&stop,&AtomicU64::new(120),&ev);
            drop(session);let commands=server.join().unwrap();
            assert!(result.is_ok(),"{result:?}");assert_eq!(last,Some(41));
            assert!(rx.try_iter().any(|event|matches!(event,Event::Mail{item,new:true} if item.uid==41)));
            assert_eq!(commands.iter().filter(|command|command.ends_with(" IDLE")).count(),1,"冷卻結束後應先檢查期間到達的新郵件");
        }

        #[test]
        fn supported_idle_still_delivers_mail_without_polling() {
            let (mut session,stop,server)=inbox(Scenario::SupportedIdle);
            let (ev,rx)=channel();let mut last=Some(40);
            let result=monitor(&mut session,&account(),&mut last,&stop,&AtomicU64::new(0),&ev);
            drop(session);let commands=server.join().unwrap();
            assert!(result.is_ok(),"{result:?}");assert_eq!(last,Some(41));
            assert!(rx.try_iter().any(|event|matches!(event,Event::Mail{item,new:true} if item.uid==41)));
            assert_eq!(commands.iter().filter(|command|command.as_str()=="DONE").count(),1);
            assert!(!commands.iter().any(|command|command.ends_with(" NOOP")));
        }

        #[test]
        fn initial_mail_is_silent_and_ready_follows_successful_reading() {
            for provider in Provider::ALL {
                let (mut session,stop,server)=inbox(Scenario::InitialMailbox);
                let (ev,rx)=channel();let mut last=None;let account=Account{provider,..account()};
                let result=monitor(&mut session,&account,&mut last,&stop,&AtomicU64::new(0),&ev);
                drop(session);server.join().unwrap();
                assert!(result.is_ok(),"{result:?}");assert_eq!(last,Some(42));
                let events:Vec<_>=rx.try_iter().collect();
                let mail=events.iter().position(|event|matches!(event,Event::Mail{item,new:false} if item.uid==42)).unwrap();
                let ready=events.iter().position(|event|matches!(event,Event::Status{status:Status::Ready,..})).unwrap();
                assert!(mail<ready,"成功讀取後再顯示已連線");
                assert!(!events.iter().any(|event|matches!(event,Event::Mail{new:true,..})),"首次載入應保持靜默");
            }
        }

        #[test]
        fn rejected_idle_still_receives_new_163_mail() {
            let (mut session,stop,server)=inbox(Scenario::RejectedIdle);
            let (ev,rx)=channel();let mut last=Some(40);
            let result=monitor(&mut session,&account(),&mut last,&stop,&AtomicU64::new(0),&ev);
            drop(session);let commands=server.join().unwrap();
            assert!(result.is_ok(),"推送被拒絕後應繼續輪詢收信：{result:?} commands={commands:?}");
            assert_eq!(last,Some(41));
            assert!(rx.try_iter().any(|event|matches!(event,Event::Mail{item,new:true} if item.uid==41)));
            assert!(!commands.iter().any(|command|command=="DONE"),"尚未進入推送狀態時不能傳送 DONE");
            assert!(commands.iter().any(|command|command.ends_with(" NOOP")));
        }

        #[test]
        fn reconnect_checks_new_mail_before_waiting_for_push() {
            let (mut session,stop,server)=inbox(Scenario::DisconnectedIdle);
            let (ev,rx)=channel();let mut last=Some(40);
            assert!(monitor(&mut session,&account(),&mut last,&stop,&AtomicU64::new(0),&ev).is_err());
            drop(session);server.join().unwrap();
            assert_eq!(last,Some(41),"重連後應先補讀斷線期間到達的郵件");
            assert!(rx.try_iter().any(|event|matches!(event,Event::Mail{item,new:true} if item.uid==41)));
        }

        #[test]
        fn interrupted_initial_fetch_keeps_history_retryable() {
            let (mut session,stop,server)=inbox(Scenario::BrokenHistory);
            let (ev,rx)=channel();let mut last=None;
            assert!(monitor(&mut session,&account(),&mut last,&stop,&AtomicU64::new(0),&ev).is_err());
            drop(session);server.join().unwrap();
            assert_eq!(last,None,"最近郵件讀取失敗後不能提交最大 UID");
            assert!(!rx.try_iter().any(|event|matches!(event,Event::Status{status:Status::Ready,..})));
        }

        #[test]
        fn bursts_do_not_skip_messages_between_batches() {
            let (mut session,_,server)=inbox(Scenario::Burst);
            let (ev,rx)=channel();let mut last=0;
            fetch_new(&mut session,&account(),&mut last,&ev).unwrap();
            fetch_new(&mut session,&account(),&mut last,&ev).unwrap();
            drop(session);server.join().unwrap();
            let uids:Vec<_>=rx.try_iter().filter_map(|event|match event {Event::Mail{item,new:true}=>Some(item.uid),_=>None}).collect();
            assert_eq!(uids,vec![1,2,3,4,5,6,7],"批次讀取不能跳過前面的新郵件");
            assert_eq!(last,7);
        }
    }

    /// 首次連線：把最近 20 封讀進面板，但不當作新郵件提示。
    fn recent<S:std::io::Read+std::io::Write>(
        session: &mut imap::Session<S>,
        account: &Account,
        ev: &Sender<Event>,
    ) -> Result<u32, ()> {
        let all = session.uid_search("ALL").map_err(|_| ())?;
        let mut uids: Vec<u32> = all.into_iter().collect();
        uids.sort_unstable();
        let max = uids.last().copied().unwrap_or(0);
        let tail: Vec<u32> = uids.iter().rev().take(20).rev().copied().collect();
        for uid in tail {
            match super::fetch::item(session,uid,account){
                Ok(Some(item))=>{let _=ev.send(Event::Mail{item,new:false});},
                Ok(None)=>{},
                Err(())=>return Err(()),
            }
        }
        Ok(max)
    }

    #[test]
    #[ignore = "需要真實 QQ 信箱及新測試郵件，驗證後臺監聽"]
    fn qq_live_notification_worker() {
        let accounts=load_accounts().into_iter().filter(|a|a.provider==Provider::Qq&&a.enabled).collect::<Vec<_>>();
        assert!(!accounts.is_empty());
        let service=Service::start();service.sync(&accounts,120);
        let start=std::time::Instant::now();
        while start.elapsed()<Duration::from_secs(160) {
            if let Ok(event)=service.ev.recv_timeout(Duration::from_secs(1)) {
                match event {
                    Event::Status{status,..}=>println!("worker_status={status:?}"),
                    Event::Mail{new,item}=>{
                        println!("worker_mail new={new} test_message={}",item.subject.starts_with("島仔"));
                        if new && item.subject=="島仔後臺監聽實測" {
                            service.sync(&[],120);return;
                        }
                    },
                    _=>{}
                }
            }
        }
        service.sync(&[],120);panic!("160 秒內未收到新郵件事件");
    }

    #[test]
    #[ignore="需要本輪已傳送的日曆邀請和普通日期郵件"]
    fn qq_live_calendar_mail_probe() {
        let account=load_accounts().into_iter().find(|a|a.provider==Provider::Qq&&a.enabled).unwrap();
        let password=secrets::load(&account.credential_key()).unwrap();
        let mut session=net::connect(&account,&password).unwrap();session.examine("INBOX").unwrap();
        let (tx,rx)=std::sync::mpsc::channel();recent(&mut session,&account,&tx).unwrap();
        let items=rx.try_iter().filter_map(|ev|if let Event::Mail{item,..}=ev{Some(item)}else{None}).collect::<Vec<_>>();
        let invite=items.iter().find(|m|m.subject=="【島仔測試】郵件日程邀請：收件箱合併驗證").expect("測試邀請未到達");
        assert_eq!(invite.meeting.as_ref().unwrap().uid,"daozai-mail-test-20261103-1500");
        let plain=items.iter().find(|m|m.subject=="DAOZAI-DATE-TEST-20261103").expect("普通日期郵件未到達");
        assert!(plain.meeting.is_none());session.logout().unwrap();
        println!("invitation_recognized=true plain_date_rejected=true");
    }

    /// 顯式執行的真實信箱只讀檢查；不輸出地址、憑據或郵件內容。
    #[test]
    fn exmail_capability_trailing_spaces_preserve_following_response() {
        let response=b"* CAPABILITY IMAP4 IMAP4rev1 XLIST MOVE IDLE XAPPLEPUSHSERVICE NAMESPACE CHILDREN ID UIDPLUS \r\na2 OK capabilities\r\n";
        let (rest,parsed)=imap_proto::parse_response(response).unwrap();
        assert!(matches!(parsed,imap_proto::Response::Capabilities(caps) if caps.iter().any(|c|matches!(c,imap_proto::Capability::Atom("IDLE")))));
        assert_eq!(rest,b"a2 OK capabilities\r\n");
        assert!(imap_proto::parse_response(b"* CAPABILITY IDLE \r\n").is_err(),"仍需校驗 IMAP4rev1");
    }

    #[test]
    #[ignore="本機騰訊企業信箱只讀連線診斷；不輸出憑據或郵件內容"]
    fn exmail_live_capability_probe() {
        let account=load_accounts().into_iter().find(|a|a.enabled && a.custom.as_ref().is_some_and(|c|c.host=="imap.exmail.qq.com")).expect("未配置騰訊企業信箱");
        let password=secrets::load(&account.credential_key()).expect("無法讀取本機憑據");
        let mut session=net::connect(&account,&password).expect("登入失敗");
        session.examine("INBOX").expect("只讀開啟失敗");
        let (tx,rx)=std::sync::mpsc::channel();
        recent(&mut session,&account,&tx).expect("最近郵件讀取失敗");
        drop(rx);
        match session.capabilities(){
            Ok(caps)=>println!("capabilities=ok idle={}",caps.has_str("IDLE")),
            Err(error)=>panic!("capabilities_error={error:?}"),
        }
        let _=session.logout();
    }

    #[test]
    #[ignore = "需要本機已連線的 QQ 信箱，顯式執行只讀實測"]
    fn qq_live_read_only_probe() {
        let accounts=load_accounts();
        let account=accounts.iter().find(|a|a.provider==Provider::Qq&&a.enabled).expect("未找到已啟用 QQ 信箱");
        let password=secrets::load(&account.credential_key()).expect("讀取信箱憑據失敗");
        let mut session=net::connect(account,&password).expect("真實登入失敗");
        session.examine("INBOX").expect("只讀開啟收件箱失敗");
        println!("idle_supported={}",session.capabilities().expect("讀取能力失敗").has_str("IDLE"));
        let before=session.uid_search("UNSEEN").expect("讀取未讀狀態失敗");
        let (tx,rx)=std::sync::mpsc::channel();
        let mut last=recent(&mut session,account,&tx).expect("最近郵件讀取失敗");
        let initial=rx.try_iter().collect::<Vec<_>>();
        assert!(initial.iter().all(|e|matches!(e,Event::Mail{new:false,..})));
        println!("login=ok readonly_inbox=ok recent_parsed={}",initial.len());
        for event in &initial {if let Event::Mail{item,..}=event{
            println!("subject_chars={} sender_chars={} body_bytes={} links={} max_link_chars={}",item.subject.chars().count(),item.from.chars().count(),item.summary.len(),item.links.len(),item.links.iter().map(|l|l.label.chars().count()).max().unwrap_or(0));
        }}
        let received=initial.iter().any(|e|matches!(e,Event::Mail{item,..} if item.subject=="島仔收信測試"&&item.code.as_deref()==Some("482731")&&!item.links.is_empty()));
        println!("test_message_with_code_and_link={received}");
        fetch_new(&mut session,account,&mut last,&tx).expect("增量收取失敗");
        let added=rx.try_iter().count();
        let after=session.uid_search("UNSEEN").expect("檢查未讀狀態失敗");
        println!("incremental_fetch=ok new_messages={} unseen_unchanged={}",added,before==after);
        session.logout().expect("退出失敗");
    }

    fn fetch_new<S:std::io::Read+std::io::Write>(
        session: &mut imap::Session<S>,
        account: &Account,
        last: &mut u32,
        ev: &Sender<Event>,
    ) -> Result<(), ()> {
        let set = session
            .uid_search(format!("UID {}:*", last.saturating_add(1)))
            .map_err(|_| ())?;
        let mut uids: Vec<u32> = set.into_iter().filter(|u| *u > *last).collect();
        uids.sort_unstable();
        uids.truncate(5);
        for uid in uids {
            if let Some(item)=super::fetch::item(session,uid,account)? {
                let _=ev.send(Event::Mail{item,new:true});
            }
            *last = (*last).max(uid);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod service {
    use super::*;

    pub struct Service;

    impl Service {
        pub fn start() -> Self {
            Self
        }
        pub fn sync(&self, _accounts: &[Account], _interval_secs: u32) {}
        pub fn test(&self, _account: Account, _password: String) {}
        pub fn manage(&self,_request:crate::mail_manage::Request){}
        pub fn try_recv(&self) -> Option<Event> {
            None
        }
    }
}

pub use service::Service;

#[cfg(test)]
mod tests {

    /// multipart/alternative 郵件必須同時保留 text/plain 與 text/html 原文，
    /// 否則排版層只能拿到被壓平的純文字。
    #[test]
    fn multipart_message_keeps_html_original(){
        let raw=b"From: Team <team@example.com>\r\nSubject: Weekly\r\n\
Content-Type: multipart/alternative; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nWeekly  Flat text\r\n\
--b\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<h1>Weekly</h1><ul><li>Flat text</li></ul>\r\n--b--\r\n";
        let parsed=super::parse_message(raw);
        assert!(!parsed.text.trim().is_empty(),"text/plain 仍要保留作摘要與兜底");
        assert!(parsed.html.contains("<h1>"),"HTML 原文必須保留");
        let lines=crate::mail_format::lines(&parsed.html,&parsed.text);
        assert_eq!(lines[0].kind,crate::mail_format::Kind::Title,"真實郵件應走 HTML 排版");
        assert!(lines.iter().any(|l|l.kind==crate::mail_format::Kind::Bullet));
    }
    #[test]
    fn quality_cancelled_meetings_are_not_active_events() {
        let event="BEGIN:VEVENT\nUID:cancelled\nDTSTART:20261101T060000Z\nSTATUS:CANCELLED\nEND:VEVENT";
        assert!(super::meetings_of(event).is_empty());
        let message=format!("BEGIN:VCALENDAR\nMETHOD:CANCEL\n{}\nEND:VCALENDAR",event.replace("STATUS:CANCELLED\n",""));
        assert!(super::meetings_of(&message).is_empty());
    }

    #[test]
    fn quality_alarm_fields_do_not_replace_meeting_fields() {
        let event="BEGIN:VEVENT\nDTSTART:20261101T060000Z\nBEGIN:VALARM\nSUMMARY:鬧鐘文案\nEND:VALARM\nSUMMARY:正式會議\nEND:VEVENT";
        assert_eq!(super::meetings_of(event)[0].title,"正式會議");
    }

    #[test]
    fn quality_ics_unfolding_preserves_content_spaces() {
        let event="BEGIN:VEVENT\nDTSTART:20261101T060000Z\nSUMMARY:專案\n  評審\nEND:VEVENT";
        assert_eq!(super::meetings_of(event)[0].title,"專案 評審");
    }
    #[test]
    fn email_links_preserve_targets_labels_and_query_parameters() {
        let parsed=super::parse_message(b"Content-Type: text/html; charset=utf-8\r\n\r\n<a href=\"https://example.com/review?a=1&amp;b=2\"><b>Review</b></a><a href='https://example.com/review?a=1&amp;b=2'>Again</a><a href=javascript:alert(1)>Unsafe</a>");
        assert_eq!(super::links_of(&parsed),vec![super::Link{label:"Review".into(),url:"https://example.com/review?a=1&b=2".into()}]);
        let plain=super::parse_message(b"Content-Type: text/plain\r\n\r\nRead https://example.com/docs. Then https://example.com/help");
        let links=super::links_of(&plain);
        assert_eq!(links.len(),2);assert_eq!(links[0].url,"https://example.com/docs");
        assert!(super::web_url("file:///C:/Windows/notepad.exe").is_none());
        assert!(super::web_url("https://").is_none());
        let parens=super::parse_message(b"Content-Type: text/plain\r\n\r\nhttps://example.com/Function_(mathematics). (https://example.com/help)");
        let targets=super::links_of(&parens);
        assert_eq!(targets[0].url,"https://example.com/Function_(mathematics)");
        assert_eq!(targets[1].url,"https://example.com/help");
    }
    use super::*;
    use chrono::TimeZone;

    fn raw(subject: &str, body: &str) -> Vec<u8> {
        format!("From: =?UTF-8?B?5bCP54mb?= <hi@example.com>\r\nSubject: {subject}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}").into_bytes()
    }

    #[test]
    fn parses_subject_sender_and_code() {
        let parsed = parse_message(&raw("驗證碼", "您的驗證碼是：834921，10分鐘內有效。"));
        assert_eq!(parsed.subject, "驗證碼");
        assert_eq!(parsed.from, "小牛");
        assert_eq!(code_of(&parsed).as_deref(), Some("834921"));
    }

    #[test]
    fn html_only_mail_still_finds_code() {
        let html = "<html><body><p>您正在登入</p><p>驗證碼：<b>906213</b></p></body></html>";
        let raw = format!("From: a@b.c\r\nSubject: 登入\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{html}");
        let parsed = parse_message(raw.as_bytes());
        assert_eq!(code_of(&parsed).as_deref(), Some("906213"));
    }

    #[test]
    fn accounts_roundtrip_without_secrets() {
        let items = vec![
            Account { custom: None, email: "a@qq.com".into(), provider: Provider::Qq, enabled: true },
            Account { custom: None, email: "b@163.com".into(), provider: Provider::Mail163, enabled: false },
        ];
        let bytes = encode_accounts(&items);
        assert_eq!(decode_accounts(&bytes), items);
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("password") && !text.contains("auth"));
        assert_eq!(decode_accounts(b"not json"), Vec::new());
    }

    #[test]
    fn provider_presets_are_gmail_icloud_custom_with_gmail_default() {
        assert_eq!(Provider::ALL, [Provider::Gmail, Provider::ICloud, Provider::Custom]);
        assert_eq!(Provider::default(), Provider::Gmail);
        assert_eq!(Provider::parse("qq"), Provider::Qq, "已保存的 QQ 帳號仍可讀取，但不再出現在預設列表");
        assert_eq!(Provider::parse("gmail"), Provider::Gmail);
        assert_eq!(Provider::Gmail.host(), "imap.gmail.com");
        assert_eq!(Provider::Gmail.port(), 993);
        let gmail=Account {custom: None, email:"demo@gmail.com".into(),provider:Provider::Gmail,enabled:true};
        assert_eq!(decode_accounts(&encode_accounts(&[gmail.clone()])),vec![gmail]);
        assert!(Provider::Mail163.needs_id() && Provider::Mail126.needs_id());
        assert!(!Provider::Qq.needs_id());
        assert_eq!(Provider::parse("exmail").host(), "imap.exmail.qq.com");
    }

    #[test]
    fn summary_flattens_body_and_caps_length() {
        let parsed = parse_message(&raw("週會", "第一行\n\n  第二行   有空格\r\n第三行"));
        assert_eq!(summary_of(&parsed), "第一行 第二行 有空格 第三行");
        let long = "字".repeat(400);
        let parsed = parse_message(&raw("長", &long));
        let s = summary_of(&parsed);
        assert_eq!(s.chars().count(), 201, "200 字 + 省略號");
        assert!(s.ends_with('…'));
        // 沒有正文時摘要為空，介面只顯示主題
        assert_eq!(summary_of(&parse_message(&raw("空", ""))), "");
    }

    #[test]
    fn full_body_keeps_paragraphs_and_content_beyond_preview_and_old_fetch_limit() {
        let text=format!("第一段\n\n{}\n\n最後一段：完整正文結束", "正文內容".repeat(10000));
        let parsed=parse_message(&raw("長郵件",&text));
        assert_eq!(body_of(&parsed),text);
        let html=parse_message(b"Content-Type: text/html; charset=utf-8\r\n\r\n<p>First paragraph</p><p>Last paragraph</p>");
        assert_eq!(body_of(&html),"First paragraph\nLast paragraph");
    }

    const ICS: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:demo-1@example.com\r\n\
SUMMARY:產品評審會\r\n\
DTSTART;TZID=Asia/Shanghai:20261030T143000\r\n\
DTEND;TZID=Asia/Shanghai:20261030T153000\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn ics_meeting_title_and_start() {
        let m = meeting_of(ICS).expect("應當識別出會議");
        assert_eq!(m.title, "產品評審會");
        let start = chrono::Utc
            .with_ymd_and_hms(2026, 10, 30, 6, 30, 0)
            .earliest()
            .unwrap();
        assert_eq!(m.at, start.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string());
        let expect = start.timestamp();
        assert_eq!(m.start, expect);
        assert_eq!(m.end,Some(expect+3600));
    }

    #[test]
    fn ics_end_time_keeps_its_timezone_and_never_invents_a_duration() {
        let ics="BEGIN:VEVENT\nDTSTART;TZID=Asia/Shanghai:20261030T143000\nDTEND:20261030T073000Z\nEND:VEVENT";
        let meeting=meeting_of(ics).unwrap();
        assert_eq!(meeting.end,Some(meeting.start+3600));
        let ics="BEGIN:VEVENT\nDTSTART:20261030T063000Z\nDTEND;TZID=Asia/Shanghai:20261030T153000\nEND:VEVENT";
        let meeting=meeting_of(ics).unwrap();
        assert_eq!(meeting.end,Some(meeting.start+3600));
        for end in ["","DTEND:20261030T060000Z\n","DTEND:invalid\n","DTEND;TZID=Unknown/Zone:20261030T153000\n"] {
            let meeting=meeting_of(&format!("BEGIN:VEVENT\nDTSTART:20261030T063000Z\n{end}END:VEVENT")).unwrap();
            assert!(meeting.end.is_none(),"缺失或無效的結束時間應保留為空");
        }
    }

    #[test]
    fn ics_utc_and_folded_lines() {
        // 折行：CRLF + 一個空格被刪掉，"Design s" + "ync" = "Design sync"
        let utc = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Design s\n ync\nDTSTART:20261030T063000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        let m = meeting_of(utc).expect("Z 時間應能解析");
        assert_eq!(m.title, "Design sync");
        let local = chrono::DateTime::from_timestamp(m.start, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%m-%d %H:%M")
            .to_string();
        assert_eq!(m.at, local);
    }

    #[test]
    fn ics_named_time_zone_uses_its_own_offset() {
        let ics = "BEGIN:VEVENT\nSUMMARY:跨時區會議\nDTSTART;TZID=America/New_York:20261030T090000\nEND:VEVENT\n";
        let m = meeting_of(ics).unwrap();
        assert_eq!(m.start, chrono::DateTime::parse_from_rfc3339("2026-10-30T13:00:00Z").unwrap().timestamp());
        let unknown = "BEGIN:VEVENT\nDTSTART;TZID=Unknown/Zone:20261030T090000\nEND:VEVENT\n";
        assert!(meeting_of(unknown).is_none());
    }

    #[test]
    fn all_day_calendar_event_has_exclusive_end() {
        let event=meeting_of("BEGIN:VEVENT\nUID:all-day\nSUMMARY:全天\nDTSTART;VALUE=DATE:20261103\nDTEND;VALUE=DATE:20261105\nEND:VEVENT").expect("全天日程應可展示");
        assert_eq!(event.end.unwrap()-event.start,2*86400);
    }

    #[test]
    fn no_ics_means_no_meeting() {
        // 普通郵件沒有日曆部分，不從正文猜測日期。
        let all_day = "BEGIN:VEVENT\nSUMMARY:放假\nDTSTART;VALUE=DATE:20261030\nEND:VEVENT\n";
        assert!(meeting_of(all_day).unwrap().all_day);
        assert!(meeting_of("").is_none());
        assert!(meeting_of("會議時間：10月30日 14:30").is_none());
    }

    #[test]
    fn calendar_part_is_collected_from_mime() {
        let raw = format!(
            "From: a@b.c\r\nSubject: 邀請\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=XYZ\r\n\r\n--XYZ\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n會議邀請見附件\r\n--XYZ\r\nContent-Type: text/calendar; charset=utf-8; method=REQUEST\r\n\r\n{ICS}\r\n--XYZ--\r\n"
        );
        let parsed = parse_message(raw.as_bytes());
        assert!(parsed.calendar.contains("產品評審會"));
        let m = meeting_of(&parsed.calendar).unwrap();
        assert_eq!(m.title, "產品評審會");
        assert_eq!(summary_of(&parsed), "會議邀請見附件");
    }
}

#[cfg(test)]
mod custom_endpoint_tests {
 #[test] fn microsoft_cloud_accounts_report_unsupported_before_connecting(){
  let a=Account{email:"test@hotmail.com".into(),provider:Provider::Microsoft,custom:None,enabled:true};
  let error=a.endpoint().expect_err("尚未支援的微軟信箱不能發起連線");
  assert!(error.contains("OAuth")&&error.contains("暫不支援"));
  assert!(Provider::Microsoft.label().contains("暫不支援"));
  for host in ["outlook.office365.com"," IMAP-MAIL.OUTLOOK.COM ","outlook.live.com","imap.outlook.com"]{
   let b=Account{email:"student@example.edu".into(),provider:Provider::Custom,custom:Some(CustomImap{host:host.into(),port:993,username:String::new()}),..a.clone()};
   assert_eq!(b.endpoint().unwrap_err(),error,"微軟雲主機應顯示同一提示");
  }
  for email in ["user@outlook.com","user@hotmail.com","user@live.com"," USER@OUTLOOK.COM "]{
   let b=Account{email:email.into(),provider:Provider::Qq,custom:None,enabled:true};
   assert_eq!(b.endpoint().unwrap_err(),error,"誤選服務商也不能引導使用者嘗試密碼");
  }
 }
 use super::*;
 fn account()->Account{Account{email:"student@example.edu".into(),provider:Provider::Custom,enabled:true,custom:Some(CustomImap{host:"imap.example.edu".into(),port:993,username:"student-id".into()})}}
 #[test] fn custom_requires_tls_hostname_and_clean_identity(){
  let a=account();assert_eq!(a.endpoint().unwrap(),("imap.example.edu".into(),993,"student-id".into()));
  for host in ["https://imap.example.edu","a/b","a:993","user@host","bad\\host","-bad.example",""]{let mut b=a.clone();b.custom.as_mut().unwrap().host=host.into();assert!(b.endpoint().is_err());}
  for port in [0,143]{let mut b=a.clone();b.custom.as_mut().unwrap().port=port;assert!(b.endpoint().is_err());}
  let mut b=a.clone();b.custom.as_mut().unwrap().username="user\r\nLOGIN".into();assert!(b.endpoint().is_err());
 }
 #[test] fn credentials_are_bound_to_target_and_login(){
  let a=account();let k=a.credential_key();
  let mut b=a.clone();b.custom.as_mut().unwrap().host="other.example.edu".into();assert_ne!(k,b.credential_key());assert_ne!(a,b);
  let mut b=a.clone();b.custom.as_mut().unwrap().username="other-user".into();assert_ne!(k,b.credential_key());
  let mut b=a.clone();b.custom.as_mut().unwrap().port=1993;assert_ne!(k,b.credential_key());
  let mut b=a.clone();b.custom.as_mut().unwrap().host="IMAP.EXAMPLE.EDU".into();assert_eq!(k,b.credential_key());
  assert!(!k.contains("student"));
 }
 #[test] fn custom_config_roundtrips_without_a_secret(){
  let a=account();let raw=encode_accounts(&[a.clone()]);assert_eq!(decode_accounts(&raw),vec![a]);
  let text=String::from_utf8(raw).unwrap();assert!(!text.contains("password"));assert!(!text.contains("secret"));
 }
}
