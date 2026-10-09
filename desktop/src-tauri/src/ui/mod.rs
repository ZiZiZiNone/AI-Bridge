//! 宿主 UI 层（薄入口）：只负责窗口 / 托盘 / 菜单的宿主装配。
//!
//! 规则（docs/架构与契约.md §0「薄入口」）：UI 层只做
//! 「接收输入 → 调用下层 → 返回结果」，无业务判断、无数据转换、无状态计算。
//! 业务规则一律在 core；依赖方向 desktop → core，本目录不得反向依赖 core 的内部实现。

pub mod candidate;
pub mod panels;
pub mod tray;
pub mod window;
pub mod wizard;
