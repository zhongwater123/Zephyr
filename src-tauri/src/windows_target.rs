use crate::target_port::{CapturedTarget, TargetContext, TargetPort};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowsTargetIdentity {
    pub hwnd: isize,
    pub process_id: u32,
    pub process_started_at: u64,
    pub executable_name: String,
    pub executable_path: String,
    pub window_title: Option<String>,
}

#[derive(Debug, Default)]
pub struct WindowsTargetAdapter;

impl WindowsTargetAdapter {
    pub(crate) fn identity(target: &CapturedTarget) -> Result<&WindowsTargetIdentity, String> {
        target
            .payload_as::<WindowsTargetIdentity>()
            .ok_or_else(|| "目标窗口身份类型与 Windows 适配器不匹配".to_string())
    }

    fn capture_identity(&self) -> Result<WindowsTargetIdentity, String> {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
        };

        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() || !unsafe { IsWindow(hwnd).as_bool() } {
            return Err("当前没有可用的前台窗口".to_string());
        }
        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id == 0 {
            return Err("无法识别目标窗口所属进程".to_string());
        }

        let executable_path = executable_path(process_id)?;
        Ok(WindowsTargetIdentity {
            hwnd: hwnd.0 as isize,
            process_id,
            process_started_at: process_started_at(process_id)?,
            executable_name: executable_name_from_path(&executable_path),
            executable_path,
            window_title: window_title(hwnd),
        })
    }

    fn validate_exists(identity: &WindowsTargetIdentity) -> Result<(), String> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

        let hwnd = HWND(identity.hwnd as *mut _);
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return Err("原目标窗口已经关闭".to_string());
        }
        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id != identity.process_id {
            return Err("原目标窗口的进程身份已经变化".to_string());
        }
        if process_started_at(process_id)? != identity.process_started_at {
            return Err("原目标进程已经被替换".to_string());
        }
        let current_path = executable_path(process_id)?;
        if !current_path.eq_ignore_ascii_case(&identity.executable_path)
            || !executable_name_from_path(&current_path)
                .eq_ignore_ascii_case(&identity.executable_name)
        {
            return Err("原目标程序身份已经变化".to_string());
        }
        Ok(())
    }

    fn validate_identity_foreground(identity: &WindowsTargetIdentity) -> Result<(), String> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

        Self::validate_exists(identity)?;
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0 as isize != identity.hwnd {
            return Err("识别期间前台窗口已经变化".to_string());
        }
        Ok(())
    }
}

impl TargetPort for WindowsTargetAdapter {
    fn capture(&self) -> Result<CapturedTarget, String> {
        let identity = self.capture_identity()?;
        let context = TargetContext {
            application_key: identity.executable_name.clone(),
            window_title: identity.window_title.clone(),
            process_id: identity.process_id,
            multiline_may_execute: is_multiline_unsafe_target(&identity.executable_name),
        };
        Ok(CapturedTarget::new(context, identity))
    }

    fn exists(&self, target: &CapturedTarget) -> Result<(), String> {
        Self::validate_exists(Self::identity(target)?)
    }

    fn validate_foreground(&self, target: &CapturedTarget) -> Result<(), String> {
        Self::validate_identity_foreground(Self::identity(target)?)
    }

    fn activate(&self, target: &CapturedTarget) -> Result<(), String> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

        let identity = Self::identity(target)?;
        Self::validate_exists(identity)?;
        let hwnd = HWND(identity.hwnd as *mut _);
        if !unsafe { SetForegroundWindow(hwnd).as_bool() } {
            return Err("Windows 拒绝激活原目标窗口".to_string());
        }
        Self::validate_identity_foreground(identity)
    }
}

fn process_started_at(process_id: u32) -> Result<u64, String> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| format!("无法打开目标进程: {error}"))?;
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let result =
        unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) };
    unsafe {
        let _ = CloseHandle(process);
    }
    result.map_err(|error| format!("无法读取目标进程创建时间: {error}"))?;
    Ok(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
}

fn executable_path(process_id: u32) -> Result<String, String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| format!("无法打开目标进程: {error}"))?;
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    unsafe {
        let _ = CloseHandle(process);
    }
    result.map_err(|error| format!("无法读取目标程序路径: {error}"))?;
    let queried = String::from_utf16_lossy(&buffer[..length as usize]);
    Ok(std::fs::canonicalize(&queried)
        .unwrap_or_else(|_| std::path::PathBuf::from(&queried))
        .to_string_lossy()
        .into_owned())
}

fn executable_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

fn window_title(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    (copied > 0).then(|| String::from_utf16_lossy(&buffer[..copied as usize]))
}

fn is_multiline_unsafe_target(executable_name: &str) -> bool {
    const UNSAFE_EXECUTABLES: &[&str] = &[
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "windowsterminal.exe",
        "openconsole.exe",
        "conhost.exe",
    ];

    UNSAFE_EXECUTABLES
        .iter()
        .any(|candidate| executable_name.eq_ignore_ascii_case(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_classification_preserves_existing_windows_policy() {
        assert!(is_multiline_unsafe_target("WindowsTerminal.exe"));
        assert!(is_multiline_unsafe_target("POWERSHELL.EXE"));
        assert!(!is_multiline_unsafe_target("Code.exe"));
    }

    #[test]
    fn process_creation_time_remains_part_of_windows_identity() {
        let first = WindowsTargetIdentity {
            hwnd: 10,
            process_id: 20,
            process_started_at: 30,
            executable_name: "app.exe".to_string(),
            executable_path: r"C:\\Apps\\app.exe".to_string(),
            window_title: None,
        };
        let mut reused = first.clone();
        reused.process_started_at = 31;
        assert_ne!(first, reused);
    }
}
