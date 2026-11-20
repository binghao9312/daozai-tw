//! 一鍵下載 Google Workspace CLI（`gws`）。
//!
//! 使用者機器上沒有 `gws` 時，由設定頁的按鈕觸發：從官方 GitHub 發行頁
//! （googleworkspace/cli，Apache-2.0）下載 Windows 版壓縮檔，核對寫死在程式裡的 SHA256 之後，
//! 只把 `gws.exe` 放進 Daozai 自己的資料夾（`%LOCALAPPDATA%\Daozai\gws`）。
//! 不需要 Node／npm，不改系統 PATH，也不需要系統管理員權限。
//!
//! 版本與雜湊都釘死：即使下載來源被換掉，雜湊不符就直接拒絕，不會執行不明檔案。
//! 要升級時改 [`VERSION`]、[`URL`]、[`SHA256`]（雜湊取自官方發行頁的 `.sha256` 檔）。
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 釘住的 gws 版本（與 `gws --version` 回報一致）。
pub const VERSION: &str = "v0.22.5";
const URL: &str = "https://github.com/googleworkspace/cli/releases/download/v0.22.5/google-workspace-cli-x86_64-pc-windows-msvc.zip";
const SHA256: &str = "407705d695dc83d48b1c5f50d71b5aa64095bf6f17d5b439b2e9a373bbe67ec2";
/// 壓縮檔約 6 MB；放寬到 24 MB 擋掉異常大的回應。
const MAX_ZIP: u64 = 24 * 1024 * 1024;
/// `gws.exe` 約 16 MB。
const MAX_EXE: usize = 64 * 1024 * 1024;
const EXE_NAME: &str = "gws.exe";
const BAD_ZIP: &str = "壓縮檔格式不正確";

/// 自己安裝的 gws 放哪：測試可用 `DAOZAI_DATA_DIR` 改到暫存資料夾。
pub fn install_dir() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("DAOZAI_DATA_DIR") {
        return Some(PathBuf::from(root).join("gws"));
    }
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("Daozai").join("gws"))
}

/// 已經由島仔安裝的 `gws.exe`（存在才返回）。
pub fn installed_exe() -> Option<PathBuf> {
    let exe = install_dir()?.join(EXE_NAME);
    exe.is_file().then_some(exe)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn verify_sha256(data: &[u8], expected: &str) -> Result<(), String> {
    let actual = hex(ring::digest::digest(&ring::digest::SHA256, data).as_ref());
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err("下載的檔案雜湊與預期不符，已拒絕安裝（可能是網路中途被改動或下載不完整）".into())
    }
}

fn le16(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?) as usize)
}

fn le32(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?) as usize)
}

/// 從 zip 取出名稱完全相同的一個檔案（只認最上層檔名，不處理目錄，所以不會有路徑穿越）。
/// 支援「儲存」與「deflate」兩種壓縮，並核對長度與 CRC32。
fn extract_entry(zip: &[u8], name: &str, limit: usize) -> Result<Vec<u8>, String> {
    let bad = || BAD_ZIP.to_string();
    let last = zip.len().checked_sub(22).ok_or_else(bad)?;
    let first = last.saturating_sub(65_535);
    let eocd = (first..=last)
        .rev()
        .find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or_else(bad)?;
    let total = le16(zip, eocd + 10).ok_or_else(bad)?;
    let mut pos = le32(zip, eocd + 16).ok_or_else(bad)?;
    for _ in 0..total {
        if zip.get(pos..pos.checked_add(4).ok_or_else(bad)?) != Some(&[0x50, 0x4b, 0x01, 0x02][..]) {
            return Err(bad());
        }
        let method = le16(zip, pos + 10).ok_or_else(bad)?;
        let crc = le32(zip, pos + 16).ok_or_else(bad)?;
        let compressed = le32(zip, pos + 20).ok_or_else(bad)?;
        let size = le32(zip, pos + 24).ok_or_else(bad)?;
        let name_len = le16(zip, pos + 28).ok_or_else(bad)?;
        let extra_len = le16(zip, pos + 30).ok_or_else(bad)?;
        let comment_len = le16(zip, pos + 32).ok_or_else(bad)?;
        let local = le32(zip, pos + 42).ok_or_else(bad)?;
        let name_at = pos + 46;
        let entry_name = zip.get(name_at..name_at.checked_add(name_len).ok_or_else(bad)?).ok_or_else(bad)?;
        pos = name_at
            .checked_add(name_len)
            .and_then(|p| p.checked_add(extra_len))
            .and_then(|p| p.checked_add(comment_len))
            .ok_or_else(bad)?;
        if entry_name != name.as_bytes() {
            continue;
        }
        // 0xFFFFFFFF 表示 zip64；官方檔案不會用到，不支援就明確拒絕。
        if compressed == 0xFFFF_FFFF || size == 0xFFFF_FFFF || local == 0xFFFF_FFFF {
            return Err("不支援 zip64 格式的壓縮檔".into());
        }
        if size > limit {
            return Err("壓縮檔內的檔案大小異常，已拒絕".into());
        }
        if zip.get(local..local.checked_add(4).ok_or_else(bad)?) != Some(&[0x50, 0x4b, 0x03, 0x04][..]) {
            return Err(bad());
        }
        let data_at = local
            .checked_add(30)
            .and_then(|p| p.checked_add(le16(zip, local + 26)?))
            .and_then(|p| p.checked_add(le16(zip, local + 28)?))
            .ok_or_else(bad)?;
        let packed = zip.get(data_at..data_at.checked_add(compressed).ok_or_else(bad)?).ok_or_else(bad)?;
        let data = match method {
            0 => packed.to_vec(),
            8 => {
                let mut out = Vec::with_capacity(size);
                flate2::read::DeflateDecoder::new(packed)
                    .take(size as u64 + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| format!("解壓縮失敗：{e}"))?;
                out
            }
            _ => return Err("不支援的壓縮方式".into()),
        };
        if data.len() != size {
            return Err(bad());
        }
        let mut check = flate2::Crc::new();
        check.update(&data);
        if check.sum() as usize != crc {
            return Err("壓縮檔內容校驗失敗".into());
        }
        return Ok(data);
    }
    Err(format!("壓縮檔裡找不到 {name}"))
}

/// 驗證雜湊、取出 `gws.exe`，寫進 `dir`（先寫暫存檔再換名，失敗不會留下半個檔案）。
fn install_into(zip: &[u8], expected_sha256: &str, dir: &Path) -> Result<PathBuf, String> {
    verify_sha256(zip, expected_sha256)?;
    let exe = extract_entry(zip, EXE_NAME, MAX_EXE)?;
    if !exe.starts_with(b"MZ") {
        return Err("取出的 gws.exe 不是 Windows 可執行檔，已拒絕".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("無法建立安裝資料夾：{e}"))?;
    let target = dir.join(EXE_NAME);
    let temp = dir.join("gws.exe.download");
    std::fs::write(&temp, &exe).map_err(|e| format!("無法寫入安裝檔：{e}"))?;
    if target.exists() {
        if let Err(e) = std::fs::remove_file(&target) {
            let _ = std::fs::remove_file(&temp);
            return Err(format!("舊的 gws.exe 無法取代（可能正在執行，請稍後再試）：{e}"));
        }
    }
    if let Err(e) = std::fs::rename(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("無法完成安裝：{e}"));
    }
    Ok(target)
}

fn download() -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .user_agent("Daozai")
        .build();
    let response = agent.get(URL).call().map_err(|e| format!("下載失敗：{e}"))?;
    let mut zip = Vec::new();
    response
        .into_reader()
        .take(MAX_ZIP + 1)
        .read_to_end(&mut zip)
        .map_err(|e| format!("下載中斷：{e}"))?;
    if zip.len() as u64 > MAX_ZIP {
        return Err("下載的檔案大小異常，已拒絕".into());
    }
    Ok(zip)
}

/// 下載、驗證並安裝；返回安裝後的 `gws.exe` 路徑。在背景執行緒呼叫。
pub fn install() -> Result<PathBuf, String> {
    let dir = install_dir().ok_or_else(|| "找不到本機應用程式資料夾（LOCALAPPDATA）".to_string())?;
    let zip = download()?;
    install_into(&zip, SHA256, &dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 組一個最小 zip（儲存或 deflate），供測試不依賴網路。
    fn make_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data, deflate) in entries {
            let mut crc = flate2::Crc::new();
            crc.update(data);
            let packed: Vec<u8> = if *deflate {
                let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                enc.write_all(data).unwrap();
                enc.finish().unwrap()
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend([0x50, 0x4b, 0x03, 0x04]);
            out.extend(20u16.to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(method.to_le_bytes());
            out.extend([0u8; 4]);
            out.extend(crc.sum().to_le_bytes());
            out.extend((packed.len() as u32).to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(name.as_bytes());
            out.extend(&packed);
            central.extend([0x50, 0x4b, 0x01, 0x02]);
            central.extend(20u16.to_le_bytes());
            central.extend(20u16.to_le_bytes());
            central.extend(0u16.to_le_bytes());
            central.extend(method.to_le_bytes());
            central.extend([0u8; 4]);
            central.extend(crc.sum().to_le_bytes());
            central.extend((packed.len() as u32).to_le_bytes());
            central.extend((data.len() as u32).to_le_bytes());
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0u8; 12]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let central_at = out.len() as u32;
        out.extend(&central);
        out.extend([0x50, 0x4b, 0x05, 0x06]);
        out.extend([0u8; 4]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(central_at.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn sha(data: &[u8]) -> String {
        hex(ring::digest::digest(&ring::digest::SHA256, data).as_ref())
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("daozai-gws-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn pinned_release_constants_are_consistent() {
        assert!(URL.contains(&format!("/download/{VERSION}/")), "網址與釘住的版本一致");
        assert!(URL.starts_with("https://github.com/googleworkspace/cli/releases/download/"), "只從官方發行頁下載");
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn extracts_stored_and_deflated_entries() {
        let big: Vec<u8> = (0..200_000u32).flat_map(|i| (i % 251) as u8..=(i % 251) as u8).collect();
        let zip = make_zip(&[("README.md", b"hello", false), ("gws.exe", &big, true), ("LICENSE", b"apache", false)]);
        assert_eq!(extract_entry(&zip, "gws.exe", MAX_EXE).unwrap(), big);
        assert_eq!(extract_entry(&zip, "README.md", MAX_EXE).unwrap(), b"hello");
        let stored = make_zip(&[("gws.exe", b"MZ-stored", false)]);
        assert_eq!(extract_entry(&stored, "gws.exe", MAX_EXE).unwrap(), b"MZ-stored");
    }

    #[test]
    fn rejects_missing_oversized_corrupt_and_truncated_archives() {
        let zip = make_zip(&[("gws.exe", b"MZdata", true)]);
        assert!(extract_entry(&zip, "other.exe", MAX_EXE).unwrap_err().contains("找不到"));
        assert!(extract_entry(&zip, "gws.exe", 3).unwrap_err().contains("大小異常"));
        // 不認帶路徑的名稱：不會把 ../gws.exe 當成 gws.exe。
        let sneaky = make_zip(&[("../gws.exe", b"MZ", false)]);
        assert!(extract_entry(&sneaky, "gws.exe", MAX_EXE).is_err());
        // 內容被改動：CRC32 對不上。
        let mut tampered = make_zip(&[("gws.exe", b"MZ-original", false)]);
        let at = tampered.windows(11).position(|w| w == b"MZ-original").unwrap();
        tampered[at + 3] ^= 0xff;
        assert!(extract_entry(&tampered, "gws.exe", MAX_EXE).unwrap_err().contains("校驗"));
        // 截斷、空檔、亂資料都只能是錯誤，不能 panic。
        for cut in [0usize, 5, 21, zip.len() / 2, zip.len() - 1] {
            assert!(extract_entry(&zip[..cut], "gws.exe", MAX_EXE).is_err(), "截到 {cut} 位元組");
        }
        assert!(extract_entry(&[0xAB; 4096], "gws.exe", MAX_EXE).is_err());
    }

    #[test]
    fn install_refuses_a_hash_mismatch_and_leaves_nothing_behind() {
        let dir = temp_dir("mismatch");
        let zip = make_zip(&[("gws.exe", b"MZ-evil", true)]);
        let err = install_into(&zip, SHA256, &dir).unwrap_err();
        assert!(err.contains("雜湊"), "{err}");
        assert!(!dir.join("gws.exe").exists() && !dir.join("gws.exe.download").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_writes_gws_exe_and_replaces_an_older_copy() {
        let dir = temp_dir("write");
        let first = make_zip(&[("gws.exe", b"MZ-first", true), ("README.md", b"x", false)]);
        let path = install_into(&first, &sha(&first), &dir).unwrap();
        assert_eq!(path, dir.join("gws.exe"));
        assert_eq!(std::fs::read(&path).unwrap(), b"MZ-first");
        let second = make_zip(&[("gws.exe", b"MZ-second", false)]);
        install_into(&second, &sha(&second), &dir).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"MZ-second", "重新安裝會換成新檔");
        assert!(!dir.join("gws.exe.download").exists(), "不留暫存檔");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_refuses_a_non_executable_payload() {
        let dir = temp_dir("notexe");
        let zip = make_zip(&[("gws.exe", b"#!/bin/sh\necho hi", false)]);
        assert!(install_into(&zip, &sha(&zip), &dir).unwrap_err().contains("可執行檔"));
        assert!(!dir.join("gws.exe").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 真的連官方發行頁：下載、核對釘住的雜湊、安裝，並執行 `gws --version`。
    /// 需要網路，預設不跑；升級 VERSION／SHA256 後用 `cargo test --lib -- --ignored real_release` 驗證。
    #[test]
    #[ignore]
    fn real_release_downloads_verifies_installs_and_runs() {
        let dir = temp_dir("real");
        let zip = download().expect("下載官方發行檔");
        let path = install_into(&zip, SHA256, &dir).expect("雜湊相符並安裝");
        let out = std::process::Command::new(&path).arg("--version").output().expect("執行 gws");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains(VERSION.trim_start_matches('v')), "gws --version 應回報 {VERSION}：{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
