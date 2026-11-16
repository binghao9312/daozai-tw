//! 渲染方式：預設 OpenGL；部分顯示卡的 OpenGL 驅動在長時間執行後會崩潰或畫面錯亂，
//! 這時改用 Direct3D 12（相容渲染）。崩潰處理只留一個標記檔案，下次啟動時再寫入設定，
//! 避免在崩潰現場改寫設定檔案。
use std::sync::atomic::{AtomicBool, Ordering};

static COMPAT_ACTIVE: AtomicBool = AtomicBool::new(false);

fn marker() -> std::path::PathBuf { crate::model::config_dir().join("diagnostics").join("renderer-fallback.txt") }

/// 崩潰模組是不是顯示卡的 OpenGL 驅動（Intel igxelpicd64 / ig9icd64、NVIDIA nvoglv64、AMD atioglxx 等，
/// 以及系統的 opengl32）。只看檔名，不含路徑。
pub fn is_gl_driver(module: &str) -> bool {
    let name = module.rsplit(['\\', '/']).next().unwrap_or(module).to_ascii_lowercase();
    let stem = name.strip_suffix(".dll").unwrap_or(&name);
    stem == "opengl32" || stem.contains("ogl") || (stem.contains("icd") && !stem.contains("vk"))
}

/// 本次執行實際使用的是不是相容渲染（崩潰處理據此判斷是否還需要切換）。
pub fn compat_active() -> bool { COMPAT_ACTIVE.load(Ordering::Relaxed) }
pub fn set_compat_active(active: bool) { COMPAT_ACTIVE.store(active, Ordering::Relaxed) }

/// 崩潰現場呼叫：只寫一個小標記檔案，記下崩潰的驅動模組。
pub fn request_fallback(module: &str) {
    let path = marker();
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    let _ = std::fs::write(path, module.chars().filter(|c| !c.is_control()).take(120).collect::<String>());
}

/// 啟動時呼叫：有標記就開啟相容渲染並記下原因，返回原因（寫入診斷用）。
pub fn apply_pending_fallback() -> Option<String> {
    let path = marker();
    let module = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let module = module.trim();
    let mut settings = crate::model::Settings::load();
    if settings.render_compat { return None; }
    let when = chrono::Local::now().format("%m-%d %H:%M");
    settings.render_compat = true;
    settings.render_compat_reason = format!("{when} 顯示卡的 OpenGL 驅動（{module}）崩潰，已自動切換");
    settings.save();
    Some(settings.render_compat_reason)
}

/// 本次啟動是否使用相容渲染。
pub fn compat_requested() -> bool { crate::model::Settings::load().render_compat }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opengl_drivers_are_recognized_and_others_are_not() {
        for module in ["igxelpicd64.dll", r"C:\Windows\System32\DriverStore\FileRepository\iigd.inf\ig9icd64.dll", "ig75icd64.dll", "nvoglv64.dll", "atioglxx.dll", "OPENGL32.dll"] {
            assert!(is_gl_driver(module), "{module}");
        }
        for module in ["daozai.exe", "ntdll.dll", "KERNELBASE.dll", "d3d12.dll", "igdumdim64.dll", "nvwgf2umx.dll", "amdxc64.dll", "igvk64.dll", "vulkan-1.dll", "ucrtbase.dll"] {
            assert!(!is_gl_driver(module), "{module}");
        }
    }
    #[test]
    fn fallback_marker_turns_on_compat_rendering_once() {
        let _g = crate::ui_tests::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("daozai-renderer-{}", std::process::id()));
        std::env::set_var("DAOZAI_DATA_DIR", &dir);
        assert!(apply_pending_fallback().is_none(), "沒有標記時不改動設定");
        request_fallback("igxelpicd64.dll");
        let reason = apply_pending_fallback().expect("有標記時開啟相容渲染");
        assert!(reason.contains("igxelpicd64.dll"));
        let settings = crate::model::Settings::load();
        assert!(settings.render_compat && settings.render_compat_reason == reason);
        assert!(apply_pending_fallback().is_none() && !marker().exists(), "標記只生效一次");
        std::env::remove_var("DAOZAI_DATA_DIR");
        let _ = std::fs::remove_dir_all(dir);
    }
}
