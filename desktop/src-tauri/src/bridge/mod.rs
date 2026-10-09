//! 浏览器扩展桥（native messaging host）：只做协议翻译，业务判断回 core。
//!
//! 依赖方向：`bridge → core`（经注入的处理函数）；**禁止** `bridge → ui`（禁止反向依赖）。
//! 数据纪律（docs/架构与契约.md §5 数据流 2）：cookie / 会话原文不经过 UI 层，也不落盘；
//! 本层不持有任何全局状态（无 static、无单例、无服务定位器）。
//!
//! 未验证：stdio 真实往返未运行（本机无 cargo）；`ext_protocol` 与 `messaging_host`
//! 的纯逻辑单测已写但未运行。

pub mod ext_protocol;
pub mod messaging_host;
