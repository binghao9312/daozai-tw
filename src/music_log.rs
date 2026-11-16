//! 音樂相容診斷：把播放器版本、進度讀取和跳轉是否適配寫進歌詞診斷日誌。
//! 只在狀態變化時寫一行，用來分清「播放器版本不認識」和「我們的邏輯有問題」。
use std::{collections::HashMap,sync::Mutex};

/// 系統媒體會話的來源 → 播放器可執行檔名。
pub(crate) fn player_exe(source:&str)->Option<&'static str> {
    Some(match source.to_ascii_lowercase().as_str() {
        "kugou"|"kugou.exe"=>"KuGou.exe",
        "cloudmusic.exe"=>"cloudmusic.exe",
        "qqmusic"|"qqmusic.exe"|"tencent.qqmusic"=>"QQMusic.exe",
        "汽水音乐"|"sodamusic.exe"=>"SodaMusic.exe",
        "spotify"|"spotify.exe"=>"Spotify.exe",
        "kwmusic.exe"=>"kwmusic.exe",
        "bodian_pc.exe"=>"bodian_pc.exe",
        _ if super::apple_music::is_source(source)=>"AppleMusic.exe",
        _=>return None,
    })
}

/// 同一主題（如「酷狗進度」）只在內容變化時返回真；第一次出現也算變化。
fn changed(topic:&str,state:&str)->bool {
    static LAST:Mutex<Option<HashMap<String,String>>>=Mutex::new(None);
    let mut last=LAST.lock().unwrap_or_else(|e|e.into_inner());
    let map=last.get_or_insert_with(HashMap::new);
    if map.get(topic).is_some_and(|old|old==state){return false;}
    map.insert(topic.to_owned(),state.to_owned());true
}

fn write(line:&str) {
    if cfg!(test) {return;}
    let line=format!("{} {line}\n",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
    crate::resources::append_log(&crate::model::config_dir().join("diagnostics"),"lyrics.log",&line);
}

/// 記一條狀態；與上次相同則不寫。
pub(crate) fn state(topic:&str,state:&str) {
    if changed(topic,state) {write(&format!("{topic} → {state}"));}
}

/// 檔案版本號（如 20.1.61.28014）；讀不到時返回「未知」。
pub(crate) fn file_version(path:&str)->String {
    use windows::{core::{HSTRING,w},Win32::Storage::FileSystem::{GetFileVersionInfoSizeW,GetFileVersionInfoW,VerQueryValueW,VS_FIXEDFILEINFO}};
    let path=HSTRING::from(path);
    unsafe {
        let size=GetFileVersionInfoSizeW(&path,None);
        if size==0 || size>1<<20 {return "未知".into();}
        let mut data=vec![0u8;size as usize];
        if GetFileVersionInfoW(&path,0,size,data.as_mut_ptr().cast()).is_err() {return "未知".into();}
        let mut info=std::ptr::null_mut();let mut length=0;
        if !VerQueryValueW(data.as_ptr().cast(),w!("\\"),&mut info,&mut length).as_bool() || info.is_null()
            || (length as usize)<std::mem::size_of::<VS_FIXEDFILEINFO>() {return "未知".into();}
        let info=&*(info as *const VS_FIXEDFILEINFO);
        format!("{}.{}.{}.{}",info.dwFileVersionMS>>16,info.dwFileVersionMS&0xffff,info.dwFileVersionLS>>16,info.dwFileVersionLS&0xffff)
    }
}

/// 當前播放器的版本：每個播放器程序只記一次（播放器重啟或升級後會再記）。
pub(crate) fn player(source:&str) {
    // 來源沒變時不重複列舉程序；每次輪詢只比較一個字串。
    static LAST:Mutex<String>=Mutex::new(String::new());
    {
        let mut last=LAST.lock().unwrap_or_else(|e|e.into_inner());
        if *last==source {return;}
        source.clone_into(&mut last);
    }
    let Some(exe)=player_exe(source) else{return;};
    for pid in super::player_process::pids_named(exe) {
        let Some(path)=super::player_process::image_path(pid) else{continue;};
        let version=file_version(&path);
        if changed(&format!("播放器 · {exe} pid{pid}"),&version) {
            write(&format!("播放器 · {source} → {exe} 版本{version}"));
        }
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn states_are_written_only_when_they_change(){
        assert!(changed("测试主题","已读取"));
        assert!(!changed("测试主题","已读取"));
        assert!(changed("测试主题","组件版本不匹配"));
        assert!(changed("测试主题","已读取"));
        assert!(changed("另一主题","已读取"));
    }
    #[test]
    fn known_sources_map_to_player_executables(){
        assert_eq!(player_exe("kugou"),Some("KuGou.exe"));
        assert_eq!(player_exe("汽水音乐"),Some("SodaMusic.exe"));
        assert_eq!(player_exe("CloudMusic.exe"),Some("cloudmusic.exe"));
        assert_eq!(player_exe("chrome.exe"),None);
    }
    #[test]
    fn reads_the_version_of_a_system_file(){
        let system=std::env::var("SystemRoot").unwrap_or_else(|_|r"C:\Windows".into());
        let version=file_version(&format!(r"{system}\System32\kernel32.dll"));
        assert!(version.split('.').count()==4 && version.split('.').all(|part|part.parse::<u32>().is_ok()),"{version}");
        assert_eq!(file_version(r"C:\不存在\nothing.dll"),"未知");
    }
}
