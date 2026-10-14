//! UI Automation 填表与前台焦点读取（B3.1 b 级退路 / B6.2 窗口匹配 / B7.2 一键填充 / D2 带态启动）。
//!
//! 边界（docs/功能点全景.md §5）：站点专属逻辑一律属转换器（A3.3「自动刷新仅转换器可选能力」），
//! 本层只做通用注入原语；是否需要「每次填充前确认」（S4）由 core 决定，本层不判断。
//!
//! 接入方式（core 落地后）：核心的填表输出通道 trait 实现于本类型。
//!
//! 待确认（Win32 语义，保持可编译、未实测）：
//! - UIA COM 调用要求线程 COM 初始化（本层按 STA 尝试，`RPC_E_CHANGED_MODE` 下的行为未实测）；
//! - `focus_info.url_hint`：浏览器地址栏 URL 的通用读取无站点无关解法（且站点解析属转换器），
//!   当前恒为空串，候选条的域名线索由调用方（热键流程）另行给出；
//! - `SendInput` 逐字符注入对受保护输入框（如部分银行控件）无效——退路终点是「填不了」，
//!   不是绕过。
//!
//! 验证状态：编译通过；UIA / SendInput 的真实填表行为需要交互会话，未实测
//! （见 desktop/README.md 未验证声明）。

use windows::core::BSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

use super::NativeError;

/// 前台焦点信息（宿主侧输入，供 core 做窗口 / 域名匹配）。
///
/// 字段只放「原始事实」，不放判断结论（匹配规则、白名单比对在 core）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusInfo {
    /// 前台窗口标题
    pub window_title: String,
    /// 前台进程可执行文件名（如 chrome.exe）
    pub process_name: String,
    /// 可解析出的 URL / 域名线索（无则空串）；解析与匹配规则在 core
    pub url_hint: String,
}

/// UIA 填表原语。
pub struct UiaTyping;

impl UiaTyping {
    /// 读取前台焦点信息。
    pub fn focus_info(&self) -> Result<FocusInfo, NativeError> {
        // SAFETY：GetForegroundWindow 可能返回空句柄（无前台窗口），单独处理；
        // 其余调用只读，句柄在调用期间由系统持有。
        unsafe {
            let window: HWND = GetForegroundWindow();
            if window.is_invalid() {
                return Err(NativeError::new(1400u32, "无前台窗口"));
            }

            let mut title = [0u16; 512];
            let length = GetWindowTextW(window, &mut title);
            let window_title = String::from_utf16_lossy(&title[..length.max(0) as usize]);

            let mut process_id = 0u32;
            GetWindowThreadProcessId(window, Some(&mut process_id));
            let process_name = process_image_name(process_id).unwrap_or_default();

            Ok(FocusInfo {
                window_title,
                process_name,
                // 见文件头「待确认」：URL 线索无站点无关读取法，恒为空串。
                url_hint: String::new(),
            })
        }
    }

    /// 向当前焦点控件填入文本：优先 UIA `ValuePattern`，不可用时退路 `SendInput`。
    pub fn fill_focused(&self, value: &str) -> Result<(), NativeError> {
        if self.fill_via_uia(value).is_ok() {
            return Ok(());
        }
        self.fill_via_sendinput(value)
    }

    /// 提交（模拟回车）。仅用于「一键填充并提交」类动作，是否提交由 core 决定。
    pub fn submit(&self) -> Result<(), NativeError> {
        let inputs = [
            key_event(VK_RETURN, KEYBD_EVENT_FLAGS(0)),
            key_event(VK_RETURN, KEYEVENTF_KEYUP),
        ];
        send_inputs(&inputs)
    }

    /// UIA `ValuePattern::SetValue` 路径。
    fn fill_via_uia(&self, value: &str) -> Result<(), NativeError> {
        ensure_com()?;
        // SAFETY：CoCreateInstance 创建 UIA COM 对象；后续调用均为接口方法。
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| NativeError::new(error.code().0 as u32, "UIA 初始化失败"))?;
            let element = automation
                .GetFocusedElement()
                .map_err(|error| NativeError::new(error.code().0 as u32, "UIA 焦点元素获取失败"))?;
            let pattern: IUIAutomationValuePattern = element
                .GetCurrentPatternAs(UIA_ValuePatternId)
                .map_err(|error| NativeError::new(error.code().0 as u32, "UIA 值模式不可用"))?;
            pattern
                .SetValue(&BSTR::from(value))
                .map_err(|error| NativeError::new(error.code().0 as u32, "UIA 填入失败"))
        }
    }

    /// `SendInput` 逐字符退路（KEYEVENTF_UNICODE 直投字符，不走键盘布局）。
    fn fill_via_sendinput(&self, value: &str) -> Result<(), NativeError> {
        let mut inputs: Vec<INPUT> = Vec::new();
        for wide in value.encode_utf16() {
            inputs.push(unicode_event(wide, KEYBD_EVENT_FLAGS(0)));
            inputs.push(unicode_event(wide, KEYEVENTF_KEYUP));
        }
        send_inputs(&inputs)
    }
}

/// 进程映像名（只取文件名部分，不带目录——完整路径属敏感定位信息）。
fn process_image_name(process_id: u32) -> Option<String> {
    // SAFETY：OpenProcess 成功后句柄归本层，用完 CloseHandle。
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let mut buffer = [0u16; 32768];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        Some(
            path.rsplit(['\\', '/'])
                .next()
                .unwrap_or_default()
                .to_string(),
        )
    }
}

/// UIA / COM 前的最小初始化（同 hello.rs 的口径：失败不 panic）。
fn ensure_com() -> Result<(), NativeError> {
    // SAFETY：只影响当前线程。
    let status = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if status.is_ok() {
        Ok(())
    } else {
        Err(NativeError::new(status.0 as u32, "COM 初始化失败"))
    }
}

fn unicode_event(wide: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                wScan: wide,
                dwFlags: KEYEVENTF_UNICODE | flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_event(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_inputs(inputs: &[INPUT]) -> Result<(), NativeError> {
    if inputs.is_empty() {
        return Ok(());
    }
    // SAFETY：inputs 为本层构造的键盘事件数组，长度与 Win32 约定一致。
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(NativeError::new(1400u32, "键盘事件注入失败"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_events_carry_each_utf16_unit() {
        let text = "密A"; // 含非 BMP 前的普通字符
        let mut inputs = Vec::new();
        for wide in text.encode_utf16() {
            inputs.push(unicode_event(wide, KEYBD_EVENT_FLAGS(0)));
            inputs.push(unicode_event(wide, KEYEVENTF_KEYUP));
        }
        assert_eq!(inputs.len(), 4);
        for pair in inputs.chunks(2) {
            let down = unsafe { &pair[0].Anonymous.ki };
            let up = unsafe { &pair[1].Anonymous.ki };
            assert_eq!(down.wScan, up.wScan, "按下/抬起必须同一字符");
            assert!(down.dwFlags.contains(KEYEVENTF_UNICODE));
            assert!(up.dwFlags.contains(KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
    }

    #[test]
    fn empty_input_list_is_noop() {
        assert!(send_inputs(&[]).is_ok());
    }

    #[test]
    fn focus_info_reports_raw_facts_without_url_guessing() {
        // 真实前台窗口依赖交互会话；这里只校验结构口径：url_hint 恒空（不猜测）。
        let info = FocusInfo {
            window_title: String::new(),
            process_name: String::new(),
            url_hint: String::new(),
        };
        assert_eq!(info.url_hint, "");
    }
}
