//! 歌词：按播放器与歌曲信息匹配 QQ 音乐、网易云及 LRCLIB，同步歌词成功后缓存。

const MAX_TEXT:usize=1024*1024;
const MAX_LINES:usize=5000;
const MAX_LINE_TEXT:usize=8192;
fn rejected_lyrics()->Lyrics{crate::resources::REJECTED_DATA.fetch_add(1,std::sync::atomic::Ordering::Relaxed);Lyrics::default()}

/// 确认「所有来源都没有同步歌词」后记住 7 天，期间不再联网；网络失败不会写入。
const NEGATIVE_TTL:std::time::Duration=std::time::Duration::from_secs(7*24*3600);
const TRACE_LIMIT:usize=24;

thread_local!{static TRACE:std::cell::RefCell<Vec<String>>=const{std::cell::RefCell::new(Vec::new())};}
/// 记录本次查词经过的步骤，只写本机诊断日志，便于定位「为什么没词」。
fn trace(step:impl FnOnce()->String){
    TRACE.with(|trace|{let mut trace=trace.borrow_mut();if trace.len()<TRACE_LIMIT{trace.push(step());}});
}
fn take_trace()->Vec<String>{TRACE.with(|trace|std::mem::take(&mut *trace.borrow_mut()))}
fn outcome(result:&Result<Lyrics,FetchError>)->String{
    match result {
        Ok(lyrics) if lyrics.synced=>format!("同步{}行",lyrics.lines.len()),
        Ok(lyrics)=>format!("文本{}行",lyrics.lines.len()),
        Err(error)=>match error {FetchError::NotFound=>"未找到".into(),FetchError::IncompleteMetadata=>"信息不完整".into(),FetchError::Unavailable(_)=>"网络失败".into(),FetchError::InvalidResponse(_)=>"响应异常".into()},
    }
}

fn is_native_script(c:char)->bool {
    // 韩文音节与字母、日文假名：这类歌名对应的歌词正文应以同一种文字为主。
    matches!(c,'\u{AC00}'..='\u{D7A3}'|'\u{1100}'..='\u{11FF}'|'\u{3130}'..='\u{318F}'|'\u{3040}'..='\u{30FF}')
}

/// 歌名是韩文或日文假名，歌词正文却几乎全是拉丁字母：判定为罗马音（音译）版本。
/// 只看歌名不看歌手：韩国歌手唱英文歌（歌名是英文）时歌词本来就是英文，不能误判。
pub(crate) fn romanized(track:&str,lyrics:&Lyrics)->bool {
    if !track.chars().any(is_native_script){return false;}
    let (mut native,mut latin)=(0usize,0usize);
    for line in lyrics.lines.iter().filter(|line|!is_credit(&line.text)) {
        for c in line.text.chars() {
            if is_native_script(c){native+=1;} else if c.is_ascii_alphabetic(){latin+=1;}
        }
    }
    latin>=40 && native*20<native+latin
}

fn negative_path(path:&std::path::Path)->std::path::PathBuf {path.with_extension("none")}
thread_local!{static SKIP_NEGATIVE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
/// 用户手动点「重试」时，本线程的这次查询跳过「近期已确认没有歌词」的记录，真正重新联网。
/// 查询结束（含提前返回）后自动恢复，不影响之后的自动查询。
struct SkipNegative(bool);
impl SkipNegative {fn set(on:bool)->Self {Self(SKIP_NEGATIVE.with(|skip|skip.replace(on)))}}
impl Drop for SkipNegative {fn drop(&mut self){SKIP_NEGATIVE.with(|skip|skip.set(self.0));}}

fn read_negative(path:&std::path::Path)->Option<Result<Lyrics,FetchError>> {
    use std::io::Read;
    if SKIP_NEGATIVE.with(|skip|skip.get()){trace(||"手动重试:跳过无歌词记录".into());return None;}
    let path=negative_path(path);
    let file=std::fs::File::open(&path).ok()?;
    let meta=file.metadata().ok()?;
    if meta.len()>MAX_TEXT as u64 || meta.modified().ok()?.elapsed().map_or(true,|age|age>NEGATIVE_TTL){return None;}
    let mut raw=String::new();file.take(MAX_TEXT as u64).read_to_string(&mut raw).ok()?;
    let lyrics=from_plain(&raw);
    Some(if lyrics.lines.is_empty(){Err(FetchError::NotFound)}else{Ok(lyrics)})
}
fn write_negative(path:&std::path::Path,plain:Option<&Lyrics>) {
    if let Some(parent)=path.parent(){if std::fs::create_dir_all(parent).is_err(){return;}}
    let raw=plain.map(|lyrics|lyrics.lines.iter().map(|line|line.text.as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
    let _=crate::model::atomic_write(&negative_path(path),raw.as_bytes());
}

#[derive(Clone, Debug)]
pub struct LyricsLine {
    pub t: f32,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricsLine>,
    pub synced: bool,
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub enum FetchError {
    Unavailable(String),
    NotFound,
    InvalidResponse(String),
    IncompleteMetadata,
}
impl FetchError {
    pub fn label(&self)->&'static str {
        match self {
            Self::Unavailable(_)=>"暂未获取歌词，请重试",
            Self::NotFound=>"未找到匹配歌词",
            Self::InvalidResponse(_)=>"歌词响应异常",
            Self::IncompleteMetadata=>"播放器未提供完整歌曲信息",
        }
    }
}

impl Lyrics {
    /// 返回当前时间点应显示的行号。
    pub fn index_at(&self, secs: f32) -> Option<usize> {
        self.lines.iter().rposition(|line|line.t<=secs)
    }
}

fn parse_time(tag: &str) -> Option<f32> {
    // mm:ss.xx 或 mm:ss:xx
    let mut parts = tag.split(':');
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let rest = parts.next()?.trim();
    let seconds=match parts.next() {Some(fraction)=>format!("{rest}.{fraction}"),None=>rest.replace(',', ".")};
    if parts.next().is_some(){return None;}
    let s: f32 = seconds.parse().ok()?;
    if !s.is_finite() || !(0.0..60.0).contains(&s){return None;}
    Some(m as f32 * 60.0 + s)
}

pub fn parse_lrc(raw: &str) -> Lyrics {
    if raw.len()>MAX_TEXT{return rejected_lyrics();}
    let mut lines = Vec::new();
    let mut copied=0usize;
    for line in raw.lines() {
        let mut times = Vec::new();
        let mut text = line;
        while text.starts_with('[') {
            let start=0;
            let Some(end_rel) = text[start..].find(']') else { break };
            let end = start + end_rel;
            let tag = &text[start + 1..end];
            if let Some(t) = parse_time(tag) {
                times.push(t);
            } else if !times.is_empty() { break; }
            text = &text[end + 1..];
        }
        let body = text.trim();
        copied=copied.saturating_add(body.len().saturating_mul(times.len()));
        if body.len()>MAX_LINE_TEXT || copied>2*MAX_TEXT || lines.len()+times.len()>MAX_LINES{return rejected_lyrics();}
        if body.is_empty() && times.is_empty() {
            continue;
        }
        for t in times {
            lines.push(LyricsLine { t, text: body.to_string() });
        }
    }
    lines.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    let synced = !lines.is_empty();
    Lyrics { lines, synced }
}

#[test]
fn quality_lyrics_keep_brackets_in_the_actual_text() {
    let parsed=parse_lrc("[00:01.00]唱到[副歌]这里\n[00:02.00][00:03.00]下一句");
    assert_eq!(parsed.lines[0].text,"唱到[副歌]这里");
    assert_eq!(parsed.lines.len(),3);
}

/// 纯文本没有同步时间，保留正文但不伪造逐行时间戳。
pub fn from_plain(raw: &str) -> Lyrics {
    if raw.len()>MAX_TEXT || raw.lines().count()>MAX_LINES || raw.lines().any(|line|line.len()>MAX_LINE_TEXT){return rejected_lyrics();}
    let lines: Vec<LyricsLine> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| LyricsLine { t: 0.0, text: l.trim().to_string() })
        .collect();
    Lyrics { lines, synced: false }
}

/// 歌词只来自 LRCLIB；网易云、QQ 音乐、酷狗、汽水的私有歌词接口与本机歌词缓存已移除。
/// `manual`：用户手动重试，跳过近期「没有歌词」的记录。网络失败仍不会被记成没有歌词。
pub fn fetch_for_display_with(source:&str,track:&str,artist:&str,duration:f64,song_id:Option<u64>,manual:bool)->Result<Lyrics,FetchError>{
    take_trace();
    let _skip=SkipNegative::set(manual);
    let started=std::time::Instant::now();
    let result=fetch_for_source(source,track,artist,duration,song_id);
    let summary=outcome(&result);
    let id=song_id.map(|id|format!(" ID={id}")).unwrap_or_default();
    let line=format!("{} {source} · {track} - {artist}{id} → {} → 结果:{summary} 用时{}ms\n",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        take_trace().join(" → "),started.elapsed().as_millis());
    // 测试时不写，免得把模拟数据混进用户的真实诊断日志。
    if !cfg!(test) {crate::resources::append_log(&crate::model::config_dir().join("diagnostics"),"lyrics.log",&line);}
    result
}

/// YouTube Music 與全域曲目標題清洗
pub fn clean_ytmusic_title(track:&str)->String {
    static SUFFIX:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let suffix=SUFFIX.get_or_init(||regex::Regex::new(r"(?i)\s*(?:-\s*Topic|[\(\[（【]\s*(?:official(?:\s+(?:music\s+|lyric\s+)?(?:video|audio|mv|lyrics?))?|lyrics?|lyric\s+video|visualizer|audio|mv|hd|4k)\s*[\)\]）】])").unwrap());
    suffix.replace_all(track,"").split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn clean_ytmusic_artist(artist:&str)->String {
    artist.trim_end_matches(" - Topic").trim().to_string()
}

/// 主力來源：YouTube Music + LRCLIB 動態歌詞與本地快取
pub fn fetch_for_source(_source:&str,track:&str,artist:&str,duration:f64,_song_id:Option<u64>)->Result<Lyrics,FetchError> {
    let clean_track = clean_ytmusic_title(track);
    let clean_art = clean_ytmusic_artist(artist);
    if clean_track.trim().is_empty() { return Err(FetchError::NotFound); }
    let path = cache_path(&clean_track, &clean_art, duration);
    if let Some(lyrics) = read_cache(&path).filter(|lyrics| !romanized(&clean_track, lyrics)) {
        trace(|| "本地快取:命中".into());
        return Ok(lyrics);
    }
    if let Some(remembered) = read_negative(&path) {
        trace(|| "近期已確認無同步歌詞".into());
        return remembered;
    }
    let (res,_failed) = fetch_lrclib_with_fallbacks(LRCLIB_BASE, &clean_track, &clean_art, duration);
    match &res {
        Ok(lyrics) if lyrics.synced => {
            if !romanized(&clean_track, lyrics) {
                write_cache(&path, lyrics);
            }
        }
        Ok(lyrics) => write_negative(&path, Some(lyrics)),
        Err(FetchError::NotFound) => write_negative(&path, None),
        _ => {}
    }
    res
}



/// LRCLIB 默认查询地址。
const LRCLIB_BASE:&str="https://lrclib.net";

/// LRCLIB 要求带可识别的 User-Agent。
const LRCLIB_USER_AGENT:&str=concat!("daozai-tw/",env!("CARGO_PKG_VERSION")," (https://github.com/binghao9312/daozai-tw)");

/// 单次 LRCLIB 查询（/api/get → /api/search）。第二个返回值：是否因网络或响应异常失败。
fn fetch_lrclib_once(base:&str,track:&str,artist:&str,duration:f64)->(Result<Lyrics,FetchError>,bool) {
    match fetch_from_lrclib(base,track,artist,duration) {
        Ok(lyrics)=>(Ok(lyrics),false),
        Err(FetchError::NotFound)=>(Err(FetchError::NotFound),false),
        Err(error)=>(Err(error),true),
    }
}

/// 歌手为空时从标题拆出歌手（两种顺序），再去掉节目 / 版本标注（只在有时长时，靠时长校验避免套错版本）。
/// 有来源断网或响应异常时立即停止，免得一首歌放大成多倍请求。
fn fetch_lrclib_with_fallbacks(base:&str,track:&str,artist:&str,duration:f64)->(Result<Lyrics,FetchError>,bool) {
    let mut attempts=vec![(track.to_owned(),artist.to_owned())];
    if artist.trim().is_empty() {
        if let Some((left,right))=split_title_artist(track) {attempts.push((left.clone(),right.clone()));attempts.push((right,left));}
    }
    let bare=bare_title(track);
    if duration>0.0 && bare!=track.trim() {attempts.push((bare,artist.to_owned()));}
    let mut best:Option<Result<Lyrics,FetchError>>=None;
    for (index,(title,singer)) in attempts.into_iter().enumerate() {
        if index>0 {trace(||format!("兜底:{title} - {}",if singer.is_empty(){"（无歌手）"}else{&singer}));}
        let (result,failed)=fetch_lrclib_once(base,&title,&singer,duration);
        if failed || result.as_ref().is_ok_and(|lyrics|lyrics.synced) {return (result,failed);}
        // 纯文本先留着，继续找同步版本；都没有时用第一份纯文本。
        if best.as_ref().is_none_or(|kept|kept.is_err() && result.is_ok()) {best=Some(result);}
    }
    (best.unwrap_or(Err(FetchError::NotFound)),false)
}




fn cache_path(track:&str,artist:&str,duration:f64)->std::path::PathBuf {
    let key=format!("{}|{}|{duration:.0}",normalized(track),normalized(artist));
    let hash=key.bytes().fold(14695981039346656037u64,|h,b|(h^b as u64).wrapping_mul(1099511628211));
    crate::model::config_dir().join("lyrics").join(format!("{hash:016x}.lrc"))
}

fn read_cache(path:&std::path::Path)->Option<Lyrics> {
    use std::io::Read;
    let file=std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len()>MAX_TEXT as u64{rejected_lyrics();return None;}
    let mut raw=String::new();file.take(MAX_TEXT as u64+1).read_to_string(&mut raw).ok()?;
    let lyrics=parse_lrc(&raw);
    (lyrics.synced && lyrics.lines.iter().any(|l|!l.text.is_empty())).then_some(lyrics)
}

fn write_cache(path:&std::path::Path,lyrics:&Lyrics) {
    if let Some(parent)=path.parent(){if std::fs::create_dir_all(parent).is_err(){return;}}
    let raw=lyrics.lines.iter().map(|line|format!("[{:02}:{:06.3}]{}\n",(line.t/60.0) as u32,line.t%60.0,line.text)).collect::<String>();
    let _=crate::model::atomic_write(path,raw.as_bytes());
}


/// 电台、DJ、部分网页播放器把「歌名 - 歌手」整个放进标题、歌手栏留空：拆成两半，只拆唯一的一个分隔符。
pub(crate) fn split_title_artist(title:&str)->Option<(String,String)> {
    for separator in [" - "," – "," — "] {
        if title.matches(separator).count()!=1 {continue;}
        let (left,right)=title.split_once(separator)?;
        let (left,right)=(left.trim(),right.trim());
        if !left.is_empty() && !right.is_empty() {return Some((left.to_owned(),right.to_owned()));}
    }
    None
}

/// 去掉开头的节目标注（【电台】、[DJ]）和结尾的版本标注（(DJ版)、（伴奏）、【现场】），只留歌名本体。
pub(crate) fn bare_title(title:&str)->String {
    static LEADING:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    static TRAILING:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let leading=LEADING.get_or_init(||regex::Regex::new(r"^\s*(?:【[^】]*】|\[[^\]]*\])\s*").unwrap());
    let trailing=TRAILING.get_or_init(||regex::Regex::new(r"\s*(?:\([^()]*\)|（[^（）]*）|\[[^\]]*\]|【[^】]*】)\s*$").unwrap());
    let mut text=title.trim().to_owned();
    loop {
        let next=trailing.replace(&leading.replace(&text,""),"").trim().to_owned();
        if next==text || next.is_empty() {return text;}
        text=next;
    }
}



/// 播放器两路来源（SMTC 与网易云日志）的歌名、歌手写法可能不同：统一规范化后再比较是否同一首。
pub(crate) fn same_song(title:&str,artist:&str,other_title:&str,other_artist:&str)->bool {
    let (a,b)=(song_identity(title,artist),song_identity(other_title,other_artist));
    !a.0.is_empty() && a.0==b.0 && (a.1.is_empty() || b.1.is_empty() || a.1==b.1)
}

/// 同一会话里前后两次歌曲信息的关系：`Some(true)` 同一首（歌手相同或一方为空），
/// `Some(false)` 同一首但署名有补全（如单人署名补全为合唱），`None` 不同歌曲。
pub(crate) fn song_relation(title:&str,artist:&str,other_title:&str,other_artist:&str)->Option<bool> {
    if same_song(title,artist,other_title,other_artist){return Some(true);}
    let (a,b)=(song_identity(title,artist),song_identity(other_title,other_artist));
    if a.0.is_empty() || a.0!=b.0 {return None;}
    let within=|x:&[String],y:&[String]|x.iter().all(|s|y.contains(s));
    (within(&a.1,&b.1) || within(&b.1,&a.1)).then_some(false)
}

/// 歌词中的署名行（作词、作曲等），不计入正文。
fn is_credit(text:&str)->bool {
    static CREDIT:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    CREDIT.get_or_init(||regex::Regex::new(r"^\s*(?:作词|作曲|编曲|词|曲|演唱|原唱|制作人|监制|混音|母带|和声|吉他|贝斯|鼓|弦乐|录音|出品|发行|OP|SP|Lyrics?|Lyricist|Composer|Arranger|Producer)\s*[:：]").unwrap()).is_match(text)
}

fn clean_title(track:&str)->String {
    static SUFFIX:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let suffix=SUFFIX.get_or_init(||regex::Regex::new(r"(?i)\s*[\(\[（【]\s*(?:official(?:\s+music)?\s+(?:video|audio)|lyrics?|lyric\s+video)\s*[\)\]）】]").unwrap());
    suffix.replace_all(track,"").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalized(text:&str)->String {
    text.chars().flat_map(char::to_lowercase).filter(|c|c.is_alphanumeric()).collect()
}

pub(super) fn artist_set(artist:&str)->Vec<String>{
    let mut parts=artist.split(['/', '、', ',', '，', '&', ';', '；']).map(normalized).filter(|s|!s.is_empty()).collect::<Vec<_>>();
    parts.sort();parts.dedup();parts
}

fn split_feature(text:&str)->(&str,&str) {
    static FEATURE:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let pattern=FEATURE.get_or_init(||regex::Regex::new(r"(?i)\b(?:feat(?:uring)?|ft)(?:\.\s*|\s+)([^()（）\[\]]+)[)）\]]?\s*$").unwrap());
    match pattern.captures(text) {
        Some(c)=>(text[..c.get(0).unwrap().start()].trim_end_matches([' ','(','（','[']),c.get(1).unwrap().as_str().trim()),
        None=>(text.trim(),""),
    }
}

/// 合唱署名可能位于歌名或歌手字段；只移动明确的署名，保留 Live、Remix 等版本信息。
fn song_identity(title:&str,artist:&str)->(String,Vec<String>) {
    let cleaned=clean_title(title);
    let (title,guests)=split_feature(&cleaned);
    let (lead,featured)=split_feature(artist);
    let artists=if artist.trim().is_empty(){Vec::new()}else{artist_set(&format!("{lead}/{featured}/{guests}"))};
    (normalized(title),artists)
}

fn search_terms(track:&str,artist:&str)->(String,String) {
    (split_feature(&clean_title(track)).0.to_owned(),split_feature(artist).0.to_owned())
}












fn fetch_from_lrclib(base:&str,track:&str,artist:&str,duration_secs:f64)->Result<Lyrics,FetchError> {
    if track.trim().is_empty() {
        return Err(FetchError::NotFound);
    }
    let clean_track = clean_ytmusic_title(track);
    let clean_art = clean_ytmusic_artist(artist);

    // 1. 優先精確查詢 /api/get
    let d = if duration_secs > 0.0 { format!("&duration={:.0}", duration_secs) } else { String::new() };
    let get_url = format!("{base}/api/get?track_name={}&artist_name={}{d}", urlencode(&clean_track), urlencode(&clean_art));
    if let Ok(val) = request_json(&get_url) {
        if let Some(synced) = val.get("syncedLyrics").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
            let parsed = parse_lrc(synced);
            if !parsed.lines.is_empty() { return Ok(parsed); }
        }
        if let Some(plain) = val.get("plainLyrics").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
            let parsed = from_plain(plain);
            if !parsed.lines.is_empty() { return Ok(parsed); }
        }
    }

    // 2. 模糊查詢 /api/search?q=
    let query_str = format!("{} {}", clean_track, clean_art);
    let search_q_url = format!("{base}/api/search?q={}", urlencode(query_str.trim()));
    if let Ok(val) = request_json(&search_q_url) {
        if let Some(items) = val.as_array() {
            if let Ok(lyrics) = lyrics_from_items(items, track, duration_secs) {
                return Ok(lyrics);
            }
        }
    }

    // 3. Fallback: 原始精準比對搜尋
    let identity=song_identity(track,artist);
    let (query_title,query_artist)=search_terms(&clean_track,&clean_art);
    let items=search(base,&query_title,&query_artist)?;
    let items=items.into_iter().filter(|item|{
        let name=item.get("trackName").and_then(|v|v.as_str()).unwrap_or_default();
        let singer=item.get("artistName").and_then(|v|v.as_str()).unwrap_or_default();
        !identity.1.is_empty() && song_identity(name,singer)==identity
    }).collect::<Vec<_>>();
    lyrics_from_items(&items,track,duration_secs)
}

fn request_json(url:&str)->Result<serde_json::Value,FetchError> {
    let response=match ureq::get(url)
        .set("User-Agent",LRCLIB_USER_AGENT)
        .set("Accept","application/json")
        .timeout(std::time::Duration::from_secs(5))
        .call() {
        Ok(response)=>response,
        Err(ureq::Error::Status(404,_))=>return Err(FetchError::NotFound),
        Err(error)=>return Err(FetchError::Unavailable(error.to_string())),
    };
    use std::io::Read;
    let mut body=String::new();
    response.into_reader().take((2*MAX_TEXT+1) as u64).read_to_string(&mut body).map_err(|e|FetchError::InvalidResponse(e.to_string()))?;
    if body.len()>2*MAX_TEXT{rejected_lyrics();return Err(FetchError::InvalidResponse("歌词响应超过大小上限".into()));}
    serde_json::from_str(&body).map_err(|e|FetchError::InvalidResponse(e.to_string()))
}

fn search(base:&str,track:&str,artist:&str)->Result<Vec<serde_json::Value>,FetchError> {
    let url = format!(
        "{base}/api/search?track_name={}&artist_name={}",
        urlencode(track),
        urlencode(artist)
    );
    let arr=request_json(&url)?;
    arr.as_array().cloned().ok_or_else(||FetchError::InvalidResponse("预期歌词列表".into()))
}

fn lyrics_from_items(items:&[serde_json::Value],track:&str,duration_secs:f64)->Result<Lyrics,FetchError> {
    // LRCLIB 同一首韩文、日文歌常同时有原文和罗马音版本：原文优先，其次时长更接近。
    let mut synced:Option<(Lyrics,(bool,f64))>=None;
    let mut plain:Option<(Lyrics,f64)>=None;
    for it in items {
        let d = it.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let score=if duration_secs>0.0{(d-duration_secs).abs()}else{0.0};
        if duration_secs>0.0 && d>0.0 && score>(duration_secs*0.03).max(10.0){continue;}
        if let Some(raw)=it.get("syncedLyrics").and_then(|v|v.as_str()) {
            let parsed=parse_lrc(raw);
            let rank=(romanized(track,&parsed),score);
            if !parsed.lines.is_empty() && synced.as_ref().is_none_or(|(_,s)|rank.0<s.0 || rank.0==s.0 && rank.1<s.1){synced=Some((parsed,rank));}
        }
        if let Some(raw)=it.get("plainLyrics").and_then(|v|v.as_str()).filter(|s|!s.trim().is_empty()) {
            if plain.as_ref().is_none_or(|(_,s)|score<*s){plain=Some((from_plain(raw),score));}
        }
    }
    synced.map(|(lyrics,_)|lyrics).or(plain.map(|(lyrics,_)|lyrics)).ok_or(FetchError::NotFound)
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn radio_style_titles_are_split_and_stripped_conservatively(){
        assert_eq!(split_title_artist("晴天 - 周杰伦"),Some(("晴天".into(),"周杰伦".into())));
        assert_eq!(split_title_artist("周杰伦 – 晴天"),Some(("周杰伦".into(),"晴天".into())));
        assert_eq!(split_title_artist("A - B - C"),None,"多个分隔符时不猜");
        assert_eq!(split_title_artist("Re-Make"),None,"连字符不是分隔符");
        assert_eq!(bare_title("【周末电台】晴天 (DJ版)"),"晴天");
        assert_eq!(bare_title("遥远的她（Live）【现场】"),"遥远的她");
        assert_eq!(bare_title("(没有歌名)"),"(没有歌名)","去掉后为空时保留原样");
        assert_eq!(bare_title("晴天"),"晴天");
    }
    #[test]
    fn resource_network_and_disk_lyrics_have_input_limits(){
        let oversized="x".repeat(2*1024*1024+1);
        let (base,worker)=serve(vec![("200 OK",oversized)]);
        assert!(matches!(request_json(&base),Err(FetchError::InvalidResponse(_))));worker.join().unwrap();
        let path=std::env::temp_dir().join(format!("daozai-large-lyrics-{}.lrc",std::process::id()));
        std::fs::write(&path,format!("[00:01]{}","x".repeat(MAX_TEXT))).unwrap();
        assert!(read_cache(&path).is_none());std::fs::remove_file(path).unwrap();
        assert!(from_plain(&"x\n".repeat(MAX_LINES+1)).lines.is_empty());
    }
    #[test]
    fn resource_lyrics_reject_oversized_input_and_timestamp_amplification(){
        assert!(parse_lrc(&format!("[00:01]{}","x".repeat(MAX_LINE_TEXT+1))).lines.is_empty());
        assert!(parse_lrc(&format!("[00:01]{}","字".repeat(400_000))).lines.is_empty());
        let raw=format!("{}{}","[00:01]".repeat(64),"x".repeat(64*1024));
        let lyrics=parse_lrc(&raw);
        assert!(lyrics.lines.iter().map(|line|line.text.len()).sum::<usize>()<=2*1024*1024,"重复时间戳不能成倍复制大段歌词");
    }










    use super::*;





    #[test]
    #[ignore="需要实际访问外部歌词服务"]
    fn live_reference_track() {
        let result=fetch_for_source("",
            "Never Gonna Give You Up","Rick Astley",213.0,None);
        println!("reference: {:?}",result.as_ref().map(|l|(l.synced,l.lines.len())));
        assert!(result.is_ok());
    }

    #[test]
    #[ignore="需要实际访问外部歌词服务"]
    fn live_web_player_track() {
        let result=fetch_for_source("",
            "スパークル (movie ver.)","RADWIMPS",534.0,None);
        println!("web-player: {:?}",result.as_ref().map(|l|(l.synced,l.lines.len())));
        assert!(result.unwrap().synced);
    }



    fn serve(responses:Vec<(&'static str,String)>)->(String,std::thread::JoinHandle<()>) {
        use std::{io::{Read,Write},net::TcpListener};
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let worker=std::thread::spawn(move||for (status,body) in responses {
            let (mut stream,_)=listener.accept().unwrap();
            let mut buf=[0;4096];let _=stream.read(&mut buf).unwrap();
            write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        });
        (base,worker)
    }




    #[test]
    fn cached_lyrics_preserve_timing_and_invalid_caches_are_ignored() {
        let path=std::env::temp_dir().join(format!("daozai-lrc-cache-{}-{:?}.lrc",std::process::id(),std::thread::current().id()));
        let expected=parse_lrc("[00:01.234]first\n[00:12.567]second\n[01:03.891]third");
        write_cache(&path,&expected);
        let cached=read_cache(&path).unwrap();
        assert_eq!(cached.lines.len(),expected.lines.len());
        for (a,b) in cached.lines.iter().zip(&expected.lines) {
            assert_eq!(a.text,b.text);assert!((a.t-b.t).abs()<0.002);
        }
        std::fs::write(&path,"broken cache").unwrap();
        assert!(read_cache(&path).is_none());
        let _=std::fs::remove_file(path);
    }

    #[test]
    fn parser_keeps_fractional_colon_times_and_rejects_invalid_timing() {
        let lyrics=parse_lrc("[00:12:35]first\n[00:NaN]invalid\n[00:61.00]invalid\n[00:12:35:10]invalid");
        assert_eq!(lyrics.lines.len(),1);assert!((lyrics.lines[0].t-12.35).abs()<0.001);
    }

    #[test]
    fn parses_and_sorts_lrc() {
        let raw = "[00:12.50]second\n[00:01.00]first\n[00:20:00]third";
        let l = parse_lrc(raw);
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[0].text, "first");
        assert_eq!(l.lines[1].text, "second");
        assert!((l.lines[2].t - 20.0).abs() < 1e-3);
        assert!(l.synced);
    }

    #[test]
    fn index_at_tracks_position() {
        let l = parse_lrc("[00:01.00]a\n[00:05.00]b\n[00:09.00]c");
        assert_eq!(l.index_at(0.0), None);
        assert_eq!(l.index_at(6.0), Some(1));
        assert_eq!(l.index_at(100.0), Some(2));
    }

    #[test]
    fn plain_lyrics_have_no_invented_timing() {
        let l = from_plain("one\ntwo\n\nthree");
        assert_eq!(l.lines.len(), 3);
        assert!(!l.synced);
        assert!(l.lines.iter().all(|line|line.t==0.0));
    }

    #[test]
    fn title_fallback_keeps_meaningful_versions() {
        assert_eq!(clean_title("スパークル (movie ver.)"),"スパークル (movie ver.)");
        assert_eq!(clean_title("Track (Official Music Video)"),"Track");
        assert_eq!(clean_title("Track (Live)"),"Track (Live)");
    }

    #[test]
    fn matching_prefers_synced_lyrics_and_rejects_other_versions() {
        let items=serde_json::json!([
            {"duration":534,"plainLyrics":"plain"},
            {"duration":535,"syncedLyrics":"[00:01.00]first\n[00:05.00]second"},
            {"duration":200,"syncedLyrics":"[00:01.00]wrong version"}
        ]);
        let result=lyrics_from_items(items.as_array().unwrap(),"Track",534.0).unwrap();
        assert!(result.synced);assert_eq!(result.lines[0].text,"first");
        assert_eq!(lyrics_from_items(&[items[2].clone()],"Track",534.0).unwrap_err(),FetchError::NotFound);
    }

    #[test]
    fn http_search_and_network_errors_are_distinct_from_missing_lyrics() {
        // 命中 /api/get 的同步歌词：只需一次请求。
        let (base,worker)=serve(vec![("200 OK",r#"{"trackName":"Track Name","artistName":"Artist","duration":120,"syncedLyrics":"[00:01.00]first"}"#.into())]);
        assert!(fetch_from_lrclib(&base,"Track Name","Artist",120.0).unwrap().synced);worker.join().unwrap();
        // 三个接口都确认没有：视为没有歌词，可以缓存。
        let (base,worker)=serve(vec![("404 Not Found","{}".into()),("200 OK","[]".into()),("200 OK","[]".into())]);
        assert_eq!(fetch_from_lrclib(&base,"Track Name","Artist",120.0).unwrap_err(),FetchError::NotFound);worker.join().unwrap();
        // 服务器错误不是没有歌词，不能被当作确认缺失。
        let (base,worker)=serve(vec![("502 Bad Gateway","".into())]);
        let (result,failed)=fetch_lrclib_with_fallbacks(&base,"Track Name","Artist",120.0);
        worker.join().unwrap();
        assert!(matches!(result,Err(FetchError::Unavailable(_))));assert!(failed);
    }

    #[test]
    fn same_song_ignores_separator_and_feature_spelling_but_not_other_tracks(){
        assert!(same_song("花见渡月","何佩欣","花见渡月","何佩欣"));
        assert!(same_song("Track","甲 / 乙","Track","乙/甲"));
        assert!(same_song("Track (feat. 乙)","甲","Track","甲、乙"));
        assert!(same_song("Track","","Track","甲"));
        assert!(!same_song("Track (Live)","甲","Track","甲"));
        assert!(!same_song("Track","甲","Track","丙"));
    }

    #[test]
    fn song_relation_separates_fill_ins_from_other_songs(){
        assert_eq!(song_relation("Track","甲","Track","甲"),Some(true));
        assert_eq!(song_relation("Track","","Track","甲"),Some(true),"歌手从空补全仍是同一首");
        assert_eq!(song_relation("Track","甲 / 乙","Track","乙、甲"),Some(true));
        assert_eq!(song_relation("有点甜","汪苏泷","有点甜","汪苏泷、BY2"),Some(false),"单人署名补全为合唱");
        assert_eq!(song_relation("有点甜","汪苏泷、BY2","有点甜","汪苏泷"),Some(false));
        assert_eq!(song_relation("Track","甲","Track","丙"),None);
        assert_eq!(song_relation("Track","甲、乙","Track","甲、丙"),None,"部分重叠不算补全");
        assert_eq!(song_relation("Track (Live)","甲","Track","甲"),None,"不同版本是不同歌曲");
        assert_eq!(song_relation("","甲","","甲"),None);
    }


    #[test]
    fn manual_retry_skips_remembered_missing_lyrics_only_during_that_query(){
        let path=std::env::temp_dir().join(format!("daozai-manual-{}-{:?}.lrc",std::process::id(),std::thread::current().id()));
        write_negative(&path,None);
        assert!(read_negative(&path).is_some());
        {
            let _skip=SkipNegative::set(true);
            assert!(read_negative(&path).is_none(),"手动重试应跳过无歌词记录");
        }
        assert!(read_negative(&path).is_some(),"查询结束后恢复使用无歌词记录");
        std::fs::remove_file(negative_path(&path)).unwrap();
    }



    #[test]
    fn korean_and_japanese_romanization_is_recognized_without_flagging_real_lyrics(){
        let roman=parse_lrc("[00:01.00]Hoksi ireon na ttaeme himdeureonni\n[00:04.00]Amu daedap eomneun neo\n[00:07.00]Babocheoreom wae\n[00:10.00]Neoreul jiuji mothae");
        let hangul=parse_lrc("[00:01.00]혹시 이런 나 땜에 힘들었니\n[00:04.00]아무 대답 없는 너\n[00:07.00]바보처럼 왜\n[00:10.00]너를 지우지 못해");
        let mixed=parse_lrc("[00:01.00]Baby you know 너를 지우지 못해\n[00:04.00]I just can't let you go 바보처럼 왜\n[00:07.00]Oh oh oh yeah yeah\n[00:10.00]아무 대답 없는 너");
        assert!(romanized("눈,코,입 (EYES,NOSE,LIPS)",&roman));
        assert!(!romanized("눈,코,입 (EYES,NOSE,LIPS)",&hangul));
        assert!(!romanized("눈,코,입",&mixed),"夹杂英文的韩文歌词仍是原文");
        assert!(!romanized("Dynamite",&roman),"英文歌名的歌曲不做判断，避免把英文歌当成音译");
        assert!(romanized("さくらんぼ",&parse_lrc("[00:01.00]Ai shiteru no hibiki dake de\n[00:05.00]Tsuyoku nareru ki ga shita yo\n[00:09.00]Sasayaka na shiawase")));
        assert!(!romanized("눈,코,입",&parse_lrc("[00:01.00]Eyes")),"字数太少不判断");
    }

    #[test]
    fn romanized_lyrics_are_only_a_fallback_and_never_cached(){
        let title="눈,코,입 (EYES,NOSE,LIPS)";
        let roman=r#"[{"trackName":"눈,코,입 (EYES,NOSE,LIPS)","artistName":"TAEYANG","duration":229,"syncedLyrics":"[00:01.00]Hoksi ireon na ttaeme himdeureonni\n[00:04.00]Amu daedap eomneun neo\n[00:07.00]Babocheoreom wae neoreul jiuji mothae"}]"#;
        let both=r#"[{"trackName":"눈,코,입 (EYES,NOSE,LIPS)","artistName":"TAEYANG","duration":229,"syncedLyrics":"[00:01.00]Hoksi ireon na ttaeme himdeureonni\n[00:04.00]Amu daedap eomneun neo\n[00:07.00]Babocheoreom wae neoreul jiuji mothae"},
            {"trackName":"눈,코,입 (EYES,NOSE,LIPS)","artistName":"TAEYANG","duration":235,"syncedLyrics":"[00:01.00]혹시 이런 나 땜에 힘들었니\n[00:04.00]아무 대답 없는 너"}]"#;
        // 同一来源里原文优先，即使罗马音版本时长更接近。
        let (base,worker)=serve(vec![("200 OK",both.into()),("200 OK",both.into())]);
        assert_eq!(fetch_from_lrclib(&base,title,"TAEYANG",229.0).unwrap().lines[0].text,"혹시 이런 나 땜에 힘들었니");worker.join().unwrap();
        // 只有罗马音时仍可显示，作为最后的备选。
        let (base,worker)=serve(vec![("200 OK",roman.into()),("200 OK",roman.into())]);
        let (result,_)=fetch_lrclib_with_fallbacks(&base,title,"TAEYANG",229.0);
        assert!(romanized(title,&result.unwrap()));worker.join().unwrap();
        // 已缓存的罗马音版本会被忽略。
        let path=std::env::temp_dir().join(format!("daozai-roman-{}.lrc",std::process::id()));
        write_cache(&path,&parse_lrc("[00:01.00]Hoksi ireon na ttaeme himdeureonni\n[00:04.00]Amu daedap eomneun neo\n[00:07.00]Babocheoreom wae neoreul jiuji mothae"));
        assert!(read_cache(&path).filter(|lyrics|!romanized(title,lyrics)).is_none());std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn urlencode_escapes_spaces_and_symbols() {
        assert_eq!(urlencode("a b&c"), "a%20b%26c");
    }

    fn capture_then_serve(status:&'static str,body:&'static str)->(String,std::sync::mpsc::Receiver<String>,std::thread::JoinHandle<()>) {
        use std::{io::{Read,Write},net::TcpListener};
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let (tx,rx)=std::sync::mpsc::channel();
        let worker=std::thread::spawn(move||{
            let (mut stream,_)=listener.accept().unwrap();
            let mut buf=[0;4096];let n=stream.read(&mut buf).unwrap();
            tx.send(String::from_utf8_lossy(&buf[..n]).into_owned()).unwrap();
            write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        });
        (base,rx,worker)
    }

    #[test]
    fn lrclib_get_prefers_synced_lyrics_and_sends_a_user_agent() {
        let (base,requests,worker)=capture_then_serve("200 OK",r#"{"trackName":"Track","artistName":"Artist","duration":120,"plainLyrics":"plain","syncedLyrics":"[00:01.00]first\n[00:05.00]second"}"#);
        let lyrics=fetch_from_lrclib(&base,"Track","Artist",120.0).unwrap();
        worker.join().unwrap();
        let request=requests.recv().unwrap();
        assert!(request.starts_with("GET /api/get?track_name=Track&artist_name=Artist&duration=120"),"{request}");
        assert!(request.to_ascii_lowercase().contains("user-agent: daozai-tw/"),"{request}");
        assert!(lyrics.synced);assert_eq!(lyrics.lines[0].text,"first");assert_eq!(lyrics.lines.len(),2);
    }

    #[test]
    fn lrclib_falls_back_to_search_when_get_misses() {
        let (base,worker)={
            use std::{io::{Read,Write},net::TcpListener};
            let listener=TcpListener::bind("127.0.0.1:0").unwrap();
            let base=format!("http://{}",listener.local_addr().unwrap());
            let worker=std::thread::spawn(move||{
                let responses=[("404 Not Found","{}".to_string()),("200 OK",r#"[{"trackName":"Track (Live)","artistName":"Artist","duration":200,"syncedLyrics":"[00:01.00]wrong"},{"trackName":"Track","artistName":"Artist","duration":120,"syncedLyrics":"[00:02.00]found"}]"#.to_string())];
                for (status,body) in responses {
                    let (mut stream,_)=listener.accept().unwrap();
                    let mut buf=[0;4096];let _=stream.read(&mut buf).unwrap();
                    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                }
            });
            (base,worker)
        };
        let lyrics=fetch_from_lrclib(&base,"Track","Artist",120.0).unwrap();
        worker.join().unwrap();
        assert!(lyrics.synced);assert_eq!(lyrics.lines[0].text,"found","时长不符的版本不采用");
    }

    #[test]
    fn lrclib_unreachable_service_is_a_failure_not_missing_lyrics() {
        let (result,failed)=fetch_lrclib_with_fallbacks("http://127.0.0.1:1","Track","Artist",120.0);
        assert!(result.is_err());assert!(failed,"断网时不能确认没有歌词");
    }

    #[test]
    fn lrclib_empty_answer_is_confirmed_missing_lyrics() {
        let (base,worker)=serve(vec![("200 OK","[]".into()),("200 OK","[]".into()),("200 OK","[]".into())]);
        let (result,failed)=fetch_lrclib_with_fallbacks(&base,"Track","Artist",120.0);
        worker.join().unwrap();
        assert_eq!(result.unwrap_err(),FetchError::NotFound);assert!(!failed);
    }

    #[test]
    fn ytmusic_titles_and_artists_are_cleaned_for_lookup() {
        assert_eq!(clean_ytmusic_title("Song (Official Video)"),"Song");
        assert_eq!(clean_ytmusic_title("Song (Official Audio)"),"Song");
        assert_eq!(clean_ytmusic_title("Song [Official MV]"),"Song");
        assert_eq!(clean_ytmusic_title("Song (Lyrics)"),"Song");
        assert_eq!(clean_ytmusic_title("Song (Live)"),"Song (Live)","有意义的版本标注保留");
        assert_eq!(clean_ytmusic_artist("Artist - Topic"),"Artist");
        assert_eq!(clean_ytmusic_artist("Artist"),"Artist");
    }

    #[test]
    fn negative_lyric_cache_expires_after_a_week() {
        let path=std::env::temp_dir().join(format!("daozai-negative-{}-{:?}.lrc",std::process::id(),std::thread::current().id()));
        let _=std::fs::remove_file(negative_path(&path));
        write_negative(&path,None);
        assert_eq!(read_negative(&path).unwrap().unwrap_err(),FetchError::NotFound);
        let file=std::fs::OpenOptions::new().write(true).open(negative_path(&path)).unwrap();
        file.set_modified(std::time::SystemTime::now()-NEGATIVE_TTL-std::time::Duration::from_secs(60)).unwrap();drop(file);
        assert!(read_negative(&path).is_none());
        std::fs::remove_file(negative_path(&path)).unwrap();
    }
}
