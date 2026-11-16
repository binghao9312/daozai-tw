//! 播放器程序身份核實：只用“查詢有限訊息”許可權確認程序名，
//! 確認是目標播放器後，呼叫方才申請記憶體讀取許可權，避免接觸其他軟體。
use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot,Process32FirstW,Process32NextW,PROCESSENTRY32W,TH32CS_SNAPPROCESS},
        Threading::{OpenProcess,QueryFullProcessImageNameW,PROCESS_NAME_WIN32,PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

fn exe_name(path:&str)->&str {path.rsplit(['\\','/']).next().unwrap_or(path)}

/// 從程序快照按可執行檔名篩選程序號；快照不需要開啟任何程序。
pub fn pids_named(exe:&str)->Vec<u32> {
    let mut pids=Vec::new();
    unsafe {
        let Ok(snapshot)=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0) else {return pids};
        let mut entry=PROCESSENTRY32W{dwSize:std::mem::size_of::<PROCESSENTRY32W>() as u32,..Default::default()};
        let mut ok=Process32FirstW(snapshot,&mut entry).is_ok();
        while ok {
            let len=entry.szExeFile.iter().position(|&c|c==0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(exe){pids.push(entry.th32ProcessID);}
            ok=Process32NextW(snapshot,&mut entry).is_ok();
        }
        let _=CloseHandle(snapshot);
    }
    pids
}

/// 僅以有限查詢許可權核對程序映像名（程序號可能被複用，每次使用前都要核對）。
pub fn image_is(pid:u32,exe:&str)->bool {
    image_path(pid).is_some_and(|path|exe_name(&path).eq_ignore_ascii_case(exe))
}

/// 用“查詢有限訊息”許可權讀取程序的可執行檔案路徑；只用於核對身份和讀取版本號。
pub fn image_path(pid:u32)->Option<String> {
    if pid==0 {return None;}
    unsafe {
        let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid).ok()?;
        let mut path=[0u16;1024];let mut length=path.len() as u32;
        let result=QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,windows::core::PWSTR(path.as_mut_ptr()),&mut length);
        let _=CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(&path[..length as usize]))
    }
}

#[test]
fn exe_name_takes_file_part(){
    assert_eq!(exe_name(r"C:\Program Files\KuGou\KuGou.exe"),"KuGou.exe");
    assert_eq!(exe_name("cloudmusic.exe"),"cloudmusic.exe");
}

#[test]
fn snapshot_finds_own_process(){
    let me=std::env::current_exe().unwrap();
    let exe=me.file_name().unwrap().to_str().unwrap();
    let pid=std::process::id();
    assert!(pids_named(exe).contains(&pid));
    assert!(image_is(pid,exe));
    assert!(!image_is(pid,"KuGou.exe"));
}
