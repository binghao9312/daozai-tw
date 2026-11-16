//! 不需要交換標準輸入輸出的外部程式啟動。
use std::{io,process::{Child,Command}};

pub fn spawn(command:&mut Command)->io::Result<Child>{
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
    command.spawn()
}

#[cfg(all(test,windows))]
mod tests {
    use super::*;
    #[test]
    fn lightweight_child_stdio_does_not_keep_parent_pipes_open(){
        if let Some(path)=std::env::var_os("DAOZAI_STDIO_PROBE"){
            #[link(name="kernel32")]
            extern "system"{fn GetStdHandle(kind:u32)->*mut std::ffi::c_void;fn GetFileType(handle:*mut std::ffi::c_void)->u32;}
            let kinds=unsafe{[-10i32,-11,-12].map(|kind|GetFileType(GetStdHandle(kind as u32)))};
            std::fs::write(path,serde_json::to_vec(&kinds).unwrap()).unwrap();return;
        }
        let path=std::env::temp_dir().join(format!("daozai-stdio-{}.json",std::process::id()));
        let make=||{let mut command=Command::new(std::env::current_exe().unwrap());
            command.args(["--exact","child_process::tests::lightweight_child_stdio_does_not_keep_parent_pipes_open","--test-threads=1"])
                .env("DAOZAI_STDIO_PROBE",&path).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
            command};
        let control=make().output().unwrap();assert!(control.status.success());
        let inherited:Vec<u32>=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let mut detached=spawn(&mut make()).unwrap();let piped=detached.stdin.is_some()||detached.stdout.is_some()||detached.stderr.is_some();
        let exited=detached.wait().unwrap();let actual:Vec<u32>=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();assert!(exited.success());
        assert_eq!(inherited,[3,3,3],"對照程序應持有管道");
        assert!(!piped,"分離啟動不能保留父程序的標準流管道");assert_eq!(actual,[2,2,2],"子程序三條標準流應連線到空裝置");
    }
}
