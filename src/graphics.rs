//! OpenGL 初始化失敗時使用獨立的圖形後端。
//!
//! 典型場景：筆記本接擴充套件塢 / DisplayLink 外接屏、遠端桌面、顯示卡驅動缺失時，視窗所在的顯示裝置
//! 只有 Windows 自帶的 OpenGL 1.1（GDI Generic），egui_glow 報「requires OpenGL 2.0+」。
//! 這時改用 Direct3D 12 重新啟動，不需要使用者拔擴充套件塢。

/// 相容圖形後端（低功耗顯示卡優先）：指定 Direct3D 12 或 Vulkan。
fn compatible(mut options:eframe::NativeOptions,backends:eframe::wgpu::Backends)->eframe::NativeOptions {
    options.renderer=eframe::Renderer::Wgpu;
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup)=&mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends=backends;
        setup.power_preference=eframe::wgpu::PowerPreference::LowPower;
        // 預設的 Performance 讓 DX12 分配器按 256MB 視訊記憶體塊 + 64MB 上傳塊預留。核顯沒有獨立視訊記憶體，
        // 這些都算進島仔程序記憶體（實測 60MB → 530MB）。島仔的紋理只有幾 MB，按小塊分配即可。
        let base=setup.device_descriptor.clone();
        setup.device_descriptor=std::sync::Arc::new(move|adapter|small_memory_blocks(base(adapter)));
    }
    options
}
/// 島仔同時存在的紋理只有幾十張。DX12 會按繫結上限預建整張描述符表：
/// 預設 100 萬項約 61MB，65536 項約 4MB，仍有上千倍餘量。
const MAX_BINDINGS:u32=65_536;
fn small_memory_blocks(descriptor:eframe::wgpu::DeviceDescriptor<'static>)->eframe::wgpu::DeviceDescriptor<'static>{
    let required_limits=eframe::wgpu::Limits{max_non_sampler_bindings:MAX_BINDINGS,..descriptor.required_limits.clone()};
    eframe::wgpu::DeviceDescriptor{memory_hints:eframe::wgpu::MemoryHints::MemoryUsage,required_limits,..descriptor}
}

/// 相容渲染優先用 Direct3D 12（Windows 原生、睡眠喚醒與換屏處理最穩）；
/// 裝置不支援時再退到 Vulkan。兩者都不經過顯示卡的 OpenGL 驅動。
fn launch_compatible(options:eframe::NativeOptions,launch:&mut impl FnMut(eframe::NativeOptions)->eframe::Result<()>)->eframe::Result<()> {
    match launch(compatible(options.clone(),eframe::wgpu::Backends::DX12)) {
        Err(eframe::Error::Wgpu(_))=>launch(compatible(options,eframe::wgpu::Backends::VULKAN)),
        result=>result,
    }
}

/// 驗收與排障：設定環境變數 `DAOZAI_RENDERER=wgpu` 直接使用相容圖形後端，
/// 在正常電腦上也能檢查 OpenGL 失敗後使用者會看到的畫面。
fn forced_compatible()->bool {
    std::env::var("DAOZAI_RENDERER").is_ok_and(|value|value.trim().eq_ignore_ascii_case("wgpu"))
}

pub fn run(options:eframe::NativeOptions,launch:impl FnMut(eframe::NativeOptions)->eframe::Result<()>)->eframe::Result<()> {
    // 上次崩在顯示卡 OpenGL 驅動裡時，崩潰處理留下的標記會在這裡轉成設定。
    let _=daozai::renderer::apply_pending_fallback();
    run_with(forced_compatible() || daozai::renderer::compat_requested(),options,launch)
}

fn opengl(mut options:eframe::NativeOptions)->eframe::NativeOptions { options.renderer=eframe::Renderer::Glow; options }

fn run_with(compat_first:bool,options:eframe::NativeOptions,mut launch:impl FnMut(eframe::NativeOptions)->eframe::Result<()>)->eframe::Result<()> {
    if compat_first {
        // 相容渲染是被要求的（設定、崩潰標記或環境變數），但這臺電腦可能根本沒有 DX12 / Vulkan 裝置
        // （遠端桌面、虛擬機器、只有基本顯示介面卡、老顯示卡）。這時退回 OpenGL，不讓使用者每次都打不開。
        daozai::renderer::set_compat_active(true);
        let error=match launch_compatible(options.clone(),&mut launch) {
            Err(error @ eframe::Error::Wgpu(_))=>error,
            result=>return result,
        };
        daozai::renderer::set_compat_active(false);
        return launch(opengl(options)).map_err(|fallback|eframe::Error::AppCreation(format!(
            "相容圖形後端無法啟動：{error}\nOpenGL 也無法啟動：{fallback}\n請安裝電腦廠商提供的顯示卡驅動後重試。"
        ).into()));
    }
    let options=opengl(options);
    let fallback_options=options.clone();
    let error=match launch(options) {
        Ok(())=>return Ok(()),
        Err(error @ (eframe::Error::OpenGL(_)|eframe::Error::Glutin(_)|eframe::Error::NoGlutinConfigs(..)))=>error,
        Err(error)=>return Err(error),
    };
    daozai::renderer::set_compat_active(true);
    launch_compatible(fallback_options,&mut launch).map_err(|fallback|eframe::Error::AppCreation(format!(
        "OpenGL 初始化失敗：{error}\n相容圖形後端也無法啟動：{fallback}\n請安裝電腦廠商提供的顯示卡驅動後重試。"
    ).into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opengl_20_failure_retries_without_opengl(){
        let mut attempts=0;
        let result=run_with(false,eframe::NativeOptions::default(),|options|{
            attempts+=1;
            if attempts==1 {
                assert_eq!(options.renderer,eframe::Renderer::Glow);
                return Err(eframe::Error::OpenGL("egui_glow requires OpenGL 2.0+.".to_owned().into()));
            }
            assert_eq!(options.renderer,eframe::Renderer::Wgpu);
            let eframe::egui_wgpu::WgpuSetup::CreateNew(setup)=options.wgpu_options.wgpu_setup else{panic!("應建立相容圖形裝置")};
            assert!(!setup.instance_descriptor.backends.contains(eframe::wgpu::Backends::GL));
            Ok(())
        });
        assert!(result.is_ok());assert_eq!(attempts,2);
    }

    #[test]
    fn compatible_rendering_prefers_direct3d_12_then_vulkan(){
        let mut backends=Vec::new();
        let result=launch_compatible(eframe::NativeOptions::default(),&mut |options|{
            let eframe::egui_wgpu::WgpuSetup::CreateNew(setup)=options.wgpu_options.wgpu_setup else{panic!("應建立相容圖形裝置")};
            backends.push(setup.instance_descriptor.backends);
            if backends.len()==1 {Err(eframe::Error::Wgpu(eframe::egui_wgpu::WgpuError::NoSuitableAdapterFound("測試".into())))} else {Ok(())}
        });
        assert!(result.is_ok());
        assert_eq!(backends,[eframe::wgpu::Backends::DX12,eframe::wgpu::Backends::VULKAN],"先 Direct3D 12，失敗再 Vulkan");
    }

    #[test]
    fn compatible_rendering_uses_small_gpu_memory_blocks(){
        let base=eframe::wgpu::DeviceDescriptor{label:Some("egui wgpu device"),required_limits:eframe::wgpu::Limits{max_texture_dimension_2d:8192,..Default::default()},..Default::default()};
        let descriptor=small_memory_blocks(base);
        assert!(matches!(descriptor.memory_hints,eframe::wgpu::MemoryHints::MemoryUsage),"核顯上預設分塊會讓程式多佔約 300MB");
        assert_eq!(descriptor.required_limits.max_texture_dimension_2d,8192,"保留 egui 的其他裝置要求");
        assert_eq!(descriptor.required_limits.max_non_sampler_bindings,MAX_BINDINGS,"預設 100 萬項的描述符表會多佔約 61MB");
    }

    #[test]
    fn application_errors_do_not_restart_the_application(){
        let mut attempts=0;
        assert!(run_with(false,eframe::NativeOptions::default(),|_|{
            attempts+=1;Err(eframe::Error::AppCreation("應用初始化失敗".into()))
        }).is_err());
        assert_eq!(attempts,1);
    }

    #[test]
    fn successful_opengl_run_is_not_restarted(){
        let mut attempts=0;
        run_with(false,eframe::NativeOptions::default(),|options|{
            attempts+=1;assert_eq!(options.renderer,eframe::Renderer::Glow);Ok(())
        }).unwrap();
        assert_eq!(attempts,1);
    }

    fn no_adapter()->eframe::Error { eframe::Error::Wgpu(eframe::egui_wgpu::WgpuError::NoSuitableAdapterFound("測試".into())) }

    #[test]
    fn requested_compat_without_any_adapter_falls_back_to_opengl(){
        let mut renderers=Vec::new();
        let result=run_with(true,eframe::NativeOptions::default(),|options|{
            renderers.push(options.renderer);
            if options.renderer==eframe::Renderer::Wgpu {Err(no_adapter())} else {Ok(())}
        });
        assert!(result.is_ok());
        assert_eq!(renderers,[eframe::Renderer::Wgpu,eframe::Renderer::Wgpu,eframe::Renderer::Glow],"DX12、Vulkan 都沒有裝置時退回 OpenGL");
    }

    #[test]
    fn requested_compat_that_works_does_not_try_opengl(){
        let mut renderers=Vec::new();
        run_with(true,eframe::NativeOptions::default(),|options|{renderers.push(options.renderer);Ok(())}).unwrap();
        assert_eq!(renderers,[eframe::Renderer::Wgpu]);
    }

    #[test]
    fn requested_compat_app_errors_do_not_try_opengl(){
        let mut attempts=0;
        assert!(run_with(true,eframe::NativeOptions::default(),|_|{attempts+=1;Err(eframe::Error::AppCreation("應用初始化失敗".into()))}).is_err());
        assert_eq!(attempts,1);
    }

    #[test]
    fn requested_compat_and_opengl_failures_are_both_reported(){
        let error=run_with(true,eframe::NativeOptions::default(),|options|{
            if options.renderer==eframe::Renderer::Wgpu {Err(no_adapter())} else {Err(eframe::Error::OpenGL("OpenGL 2.0".to_owned().into()))}
        }).unwrap_err().to_string();
        assert!(error.contains("測試") && error.contains("OpenGL 2.0"),"{error}");
    }

    #[test]
    fn both_failures_are_preserved_in_the_report(){
        let mut attempts=0;
        let error=run_with(false,eframe::NativeOptions::default(),|_|{
            attempts+=1;
            if attempts==1{Err(eframe::Error::OpenGL("OpenGL 2.0".to_owned().into()))}
            else{Err(eframe::Error::AppCreation("沒有可用的圖形裝置".into()))}
        }).unwrap_err().to_string();
        assert!(error.contains("OpenGL 2.0") && error.contains("沒有可用的圖形裝置"));
        assert_eq!(attempts,2);
    }
}
