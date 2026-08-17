use std::path::PathBuf;

#[cfg(windows)]
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System, UpdateKind};

use crate::error::{app_err, AppResult};

pub const WINDOWS_CODEX_AUMIDS: &[&str] = &[
    "OpenAI.Codex_2p2nqsd0c76g0!App",
    "OpenAI.CodexBeta_2p2nqsd0c76g0!App",
    "OpenAI.ChatGPT-Desktop_2p2nqsd0c76g0!App",
];

pub fn find_process_ids(manual_path: Option<&str>) -> Vec<u32> {
    let system = process_system();
    system
        .processes()
        .iter()
        .filter(|(_, process)| is_codex_desktop_process(process, manual_path))
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

pub fn terminate_process_ids(ids: &[u32]) {
    let system = process_system();
    for id in ids {
        if let Some(process) = system.process(Pid::from_u32(*id)) {
            let _ = process.kill();
        }
    }
}

pub fn running_process_ids(ids: &[u32]) -> Vec<u32> {
    let system = process_system();
    ids.iter()
        .copied()
        .filter(|id| system.process(Pid::from_u32(*id)).is_some())
        .collect()
}

fn process_system() -> System {
    System::new_with_specifics(
        // 只需要进程可执行文件路径判断 Codex 桌面进程；不刷新 CPU/内存/磁盘等无关数据，
        // 避免每 3 秒轮询和每次 get_state 全量扫描系统进程产生无谓开销
        RefreshKind::nothing()
            .with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
    )
}

pub fn wait_for_exit_with<F, S>(
    ids: &[u32],
    timeout_ms: u64,
    interval_ms: u64,
    mut running: F,
    mut sleep: S,
) -> bool
where
    F: FnMut(&[u32]) -> Vec<u32>,
    S: FnMut(Duration),
{
    if ids.is_empty() {
        return true;
    }
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if running(ids).is_empty() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(interval_ms));
    }
}

pub fn wait_for_exit(ids: &[u32], timeout_ms: u64, interval_ms: u64) -> bool {
    wait_for_exit_with(
        ids,
        timeout_ms,
        interval_ms,
        running_process_ids,
        std::thread::sleep,
    )
}

pub fn launch_codex(manual_path: Option<&str>) -> AppResult<()> {
    #[cfg(windows)]
    {
        if let Some(executable) = windows_standalone_executable(manual_path).filter(|path| {
            !path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("\\windowsapps\\")
        }) {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            Command::new(&executable)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|error| app_err!("无法启动 Codex: {error}"))?;
            return Ok(());
        }

        if activate_windows_package(manual_path).is_ok() {
            return Ok(());
        }
        Err(app_err!("未找到可启动的 Codex/ChatGPT 桌面应用"))
    }

    #[cfg(not(windows))]
    {
        let app = manual_path
            .map(PathBuf::from)
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("app"))
            .or_else(macos_app_candidate)
            .ok_or_else(|| app_err!("未找到可启动的 Codex/ChatGPT 桌面应用"))?;
        Command::new("open")
            .arg("-a")
            .arg(&app)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| app_err!("无法启动 Codex: {error}"))?;
        Ok(())
    }
}

#[cfg(windows)]
fn activate_windows_package(manual_path: Option<&str>) -> AppResult<()> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        ApplicationActivationManager, IApplicationActivationManager, ACTIVATEOPTIONS,
    };

    if manual_path.is_some_and(|value| value.to_ascii_lowercase().ends_with(".exe")) {
        return Err(app_err!("manual executable"));
    }

    unsafe {
        let coinitialize = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninitialize = coinitialize.is_ok();
        if !should_uninitialize && coinitialize.0 != -2147417850 {
            return Err(app_err!("Windows COM 初始化失败"));
        }

        let result: AppResult<()> = (|| {
            let manager: IApplicationActivationManager =
                CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_ALL)
                    .map_err(|error| app_err!("无法创建应用激活器: {error}"))?;
            let mut last_error = None;
            for aumid in WINDOWS_CODEX_AUMIDS {
                match manager.ActivateApplication(
                    &HSTRING::from(*aumid),
                    &HSTRING::from(""),
                    ACTIVATEOPTIONS(0),
                ) {
                    Ok(_) => return Ok(()),
                    Err(error) => last_error = Some(error),
                }
            }
            Err(app_err!(
                "无法启动 Codex/ChatGPT packaged app: {}",
                last_error
                    .map(|error| error.to_string())
                    .unwrap_or_default()
            ))
        })();

        if should_uninitialize {
            CoUninitialize();
        }
        result
    }
}

#[cfg(windows)]
fn windows_standalone_executable(manual_path: Option<&str>) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = manual_path
        .map(PathBuf::from)
        .into_iter()
        .chain(
            std::env::var_os("LOCALAPPDATA")
                .map(|root| PathBuf::from(root).join("OpenAI").join("Codex")),
        )
        .collect();
    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
        for name in ["Codex.exe", "ChatGPT.exe"] {
            let executable = candidate.join(name);
            if executable.is_file() {
                return Some(executable);
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn macos_app_candidate() -> Option<PathBuf> {
    let home = crate::paths::home_dir();
    let names = [
        "Codex.app",
        "OpenAI Codex.app",
        "OpenAI.Codex.app",
        "ChatGPT.app",
    ];
    let roots = [PathBuf::from("/Applications"), home.join("Applications")];
    roots
        .iter()
        .flat_map(|root| names.iter().map(move |name| root.join(name)))
        .find(|path| path.is_dir())
}

pub fn codex_display_path(manual_path: Option<&str>) -> (String, String) {
    if let Some(manual) = manual_path.filter(|value| !value.trim().is_empty()) {
        return (manual.to_string(), "manual".into());
    }

    #[cfg(windows)]
    {
        (WINDOWS_CODEX_AUMIDS[0].to_string(), "packaged-app".into())
    }

    #[cfg(not(windows))]
    {
        let path = macos_app_candidate()
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| "未识别".into());
        (path, "auto".into())
    }
}

fn is_codex_desktop_process(process: &sysinfo::Process, manual_path: Option<&str>) -> bool {
    #[cfg(windows)]
    {
        let Some(exe) = process.exe() else {
            return false;
        };
        is_windows_codex_process(exe, manual_path)
    }

    #[cfg(not(windows))]
    {
        let _ = manual_path;
        let command = process
            .cmd()
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<String>();
        is_macos_codex_command(&command)
    }
}

#[cfg(windows)]
pub fn is_windows_codex_process(exe: &Path, manual_path: Option<&str>) -> bool {
    let exe_text = exe
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    let file_name = exe
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let is_codex_name = file_name.eq_ignore_ascii_case("Codex.exe")
        || file_name.eq_ignore_ascii_case("ChatGPT.exe");
    if !is_codex_name {
        return false;
    }

    if exe_text.contains("\\windowsapps\\") {
        return !exe_text.contains("\\app\\resources\\")
            && (exe_text.contains("\\openai.codex_")
                || exe_text.contains("\\openai.codexbeta_")
                || exe_text.contains("\\openai.chatgpt-desktop_"));
    }

    if let Some(manual) = manual_path {
        let root = Path::new(manual);
        let root = if root.is_file() {
            root.parent().unwrap_or(root)
        } else {
            root
        };
        let root_text = root
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase();
        return !root_text.is_empty() && exe_text.starts_with(&root_text);
    }

    exe_text.contains("\\openai\\codex\\") || exe_text.contains("\\programs\\openai\\")
}

#[cfg(not(windows))]
pub fn is_macos_codex_command(command: &str) -> bool {
    let is_main = command.contains(".app/Contents/MacOS/ChatGPT")
        || command.contains(".app/Contents/MacOS/Codex");
    is_main && !command.contains("/Contents/MacOS/Helpers/") && !command.contains("/Helpers/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_process_filter_excludes_cli_and_helpers() {
        let package = Path::new(
            r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0_x64__2p2nqsd0c76g0\App\Codex.exe",
        );
        let helper = Path::new(
            r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0_x64__2p2nqsd0c76g0\App\resources\ChatGPT.exe",
        );
        let cli = Path::new(r"C:\Users\me\.codex\bin\codex.exe");
        assert!(is_windows_codex_process(package, None));
        assert!(!is_windows_codex_process(helper, None));
        assert!(!is_windows_codex_process(cli, None));
    }

    #[cfg(not(windows))]
    #[test]
    fn macos_process_filter_excludes_helpers_and_cli() {
        assert!(is_macos_codex_command(
            "/Applications/Codex.app/Contents/MacOS/Codex --type=renderer"
        ));
        assert!(!is_macos_codex_command(
            "/Applications/Codex.app/Contents/MacOS/Helpers/Renderer.app/Contents/MacOS/Renderer"
        ));
        assert!(!is_macos_codex_command("/usr/local/bin/codex"));
    }

    #[test]
    fn wait_state_machine_handles_not_running_and_timeout() {
        assert!(wait_for_exit_with(
            &[1],
            0,
            0,
            |_: &[u32]| Vec::<u32>::new(),
            |_| {}
        ));
        assert!(!wait_for_exit_with(
            &[1],
            0,
            0,
            |ids: &[u32]| ids.to_vec(),
            |_| {}
        ));
    }
}
