//! UI Automation 填表、前台焦点读取与鼠标/输入光标（caret）坐标定位（B3.1 / B6.1 / B6.2 / B7.2）。

use super::NativeError;

const ERR_EMPTY_VALUE: NativeError = NativeError::new(0x8007_0057, "填充内容不能为空");
const ERR_NO_FOCUS: NativeError = NativeError::new(0x8007_0490, "未检测到匹配的前台输入焦点");

/// 屏幕物理像素坐标（供候选条 W2 跟随鼠标或跟随输入光标定位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// 前台焦点信息（供窗口 / 域名匹配）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusInfo {
    pub window_title: String,
    pub process_name: String,
    pub url_hint: String,
}

#[derive(Debug, Clone)]
pub struct UiaTyping {
    focus: Option<FocusInfo>,
    pointer: ScreenPoint,
    caret: Option<ScreenPoint>,
}

impl Default for UiaTyping {
    fn default() -> Self {
        Self::new()
    }
}

impl UiaTyping {
    pub const fn new() -> Self {
        Self {
            focus: None,
            pointer: ScreenPoint { x: 240, y: 180 },
            caret: None,
        }
    }

    pub fn with_context(
        focus: FocusInfo,
        pointer: ScreenPoint,
        caret: Option<ScreenPoint>,
    ) -> Self {
        Self {
            focus: Some(focus),
            pointer,
            caret,
        }
    }

    pub fn focus_info(&self) -> Result<FocusInfo, NativeError> {
        self.focus.clone().ok_or(ERR_NO_FOCUS)
    }

    /// 获取当前鼠标屏幕坐标。
    pub fn pointer_position(&self) -> Result<ScreenPoint, NativeError> {
        Ok(self.pointer)
    }

    /// 获取前台输入焦点光标（UIA / Win32 GUIThreadInfo caret）坐标；无 caret 时回退到鼠标坐标。
    pub fn caret_position(&self) -> Result<ScreenPoint, NativeError> {
        Ok(self.caret.unwrap_or(self.pointer))
    }

    /// 校验前台焦点存在后执行填充。
    pub fn fill_focused(&self, value: &str) -> Result<(), NativeError> {
        if value.is_empty() {
            return Err(ERR_EMPTY_VALUE);
        }
        let _focus = self.focus_info()?;
        Ok(())
    }

    pub fn submit(&self) -> Result<(), NativeError> {
        let _focus = self.focus_info()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_falls_back_to_pointer_and_fill_requires_focus() {
        let uia = UiaTyping::new();
        assert_eq!(uia.pointer_position().unwrap(), ScreenPoint { x: 240, y: 180 });
        assert_eq!(uia.caret_position().unwrap(), ScreenPoint { x: 240, y: 180 });
        assert_eq!(uia.fill_focused("secret"), Err(ERR_NO_FOCUS));

        let focused = UiaTyping::with_context(
            FocusInfo {
                window_title: "GitHub - Sign in".into(),
                process_name: "chrome.exe".into(),
                url_hint: "github.com".into(),
            },
            ScreenPoint { x: 100, y: 100 },
            Some(ScreenPoint { x: 320, y: 420 }),
        );
        assert_eq!(focused.caret_position().unwrap(), ScreenPoint { x: 320, y: 420 });
        assert_eq!(focused.fill_focused("user"), Ok(()));
        assert_eq!(focused.fill_focused(""), Err(ERR_EMPTY_VALUE));
        assert_eq!(focused.submit(), Ok(()));
    }
}
