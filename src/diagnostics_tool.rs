//! 收集診斷：診斷指令碼內嵌在程式裡，`daozai.exe --collect-diagnostics` 直接執行，不依賴安裝目錄裡的附帶檔案。
//!
//! 兩個入口共用這裡：
//! - 「設定 → 關於 → 收集診斷」：島仔能開啟時用；
//! - 開始選單「收集島仔診斷」（免安裝包裡是同名的 cmd）：島仔啟動即崩潰、卡死、找不到視窗時用。
//!   這條路在單例項檢查、建立視窗和初始化顯示卡之前就處理，顯示卡驅動出問題也能收集。
#[cfg(windows)]
use std::path::{Path,PathBuf};

pub const ARG:&str="--collect-diagnostics";
const SCRIPT:&[u8]=include_bytes!("../scripts/collect-diagnostics.ps1");

/// 命令列裡是否要求收集診斷。
pub fn requested(mut args:impl Iterator<Item=String>)->bool {args.any(|arg|arg==ARG)}

/// 命令列入口：釋放指令碼、在新控制檯裡收集、結束後在資源管理器裡選中診斷包。
#[cfg(windows)]
pub fn run(data_dir:&Path)->Result<(),String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE:u32=0x0000_0010;
    let work=std::env::temp_dir().join(format!("daozai-diagnostics-tool-{}",std::process::id()));
    std::fs::create_dir_all(&work).map_err(|e|format!("無法建立臨時目錄：{e}"))?;
    let script=work.join("collect-diagnostics.ps1");
    let result=(||{
        // 指令碼自帶 UTF-8 BOM，Windows PowerShell 5.1 才能正確讀中文。
        std::fs::write(&script,SCRIPT).map_err(|e|format!("無法寫入診斷指令碼：{e}"))?;
        let output=dirs_desktop().unwrap_or_else(||data_dir.to_path_buf());
        let status=std::process::Command::new("powershell.exe")
            .args(["-NoProfile","-ExecutionPolicy","Bypass","-File"]).arg(&script)
            .arg("-DataDirectory").arg(data_dir).arg("-OutputDirectory").arg(output).arg("-RevealArchive")
            .creation_flags(CREATE_NEW_CONSOLE).status().map_err(|e|format!("無法啟動 PowerShell：{e}"))?;
        if !status.success(){return Err("診斷收集未完成".into());}
        Ok(())
    })();
    let _=std::fs::remove_file(&script);let _=std::fs::remove_dir(&work);
    result
}

/// 當前使用者的桌面（診斷包預設放這裡，方便使用者找到併傳送）。
#[cfg(windows)]
fn dirs_desktop()->Option<PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Desktop,SHGetKnownFolderPath,KF_FLAG_DEFAULT};
    unsafe {
        let path=SHGetKnownFolderPath(&FOLDERID_Desktop,KF_FLAG_DEFAULT,None).ok()?;
        let text=path.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

/// 應用內按鈕：另起一個 `daozai.exe --collect-diagnostics`，介面不等待，也不受當前視窗狀態影響。
pub fn launch()->std::io::Result<()> {
    let exe=std::env::current_exe()?;
    std::process::Command::new(exe).arg(ARG).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map(|_|())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_explicit_switch_starts_collection(){
        assert!(requested(["daozai.exe".to_string(),ARG.to_string()].into_iter()));
        assert!(!requested(["daozai.exe".to_string(),"--settings".to_string()].into_iter()));
    }
    #[test]
    fn embedded_script_keeps_bom_and_collects_music_gaps(){
        assert!(SCRIPT.starts_with(&[0xEF,0xBB,0xBF]),"Windows PowerShell 5.1 需要 BOM 才能讀中文");
        let text=String::from_utf8_lossy(SCRIPT);
        assert!(text.contains("music-gaps.log") && text.contains("param("));
    }
}
