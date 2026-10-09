//! 只读路径守卫：转换器清单 `readable_paths` 之外一律拒绝。
//!
//! 判定方式（本 crate 唯一的路径规则）：路径按分隔符切分为组件序列后做**组件级前缀包含**判定，
//! 因此 `…\QQ\nt_data` 通过、同前缀的兄弟目录 `…\QQBackup` 被拒；任何含 `..` 的请求（路径穿越）直接拒。
//! 大小写按字节精确比较：Windows 上大小写不一致的合法路径会被**过度拒绝**（fail-closed，绝不放行越界路径）。
//! 清单里含 `..` 或空的声明路径在构造守卫时即被丢弃（同样 fail-closed）。
//!
//! 本模块不做任何文件系统调用：守卫只负责"这条路径能不能读"，打开动作由沙盒运行器执行。

use crate::error::PathDenied;
use std::path::PathBuf;

/// 只读路径白名单守卫，由清单 `readable_paths` 构造。
pub struct PathGuard {
    roots: Vec<Vec<String>>,
}

impl PathGuard {
    /// 用清单声明的可读路径构造守卫；空声明或不含有效声明的清单得到的守卫拒绝一切请求。
    pub fn new(declared: &[String]) -> Self {
        let mut roots: Vec<Vec<String>> = Vec::new();
        for raw in declared {
            let components = normalize(raw);
            if components.is_empty() || components.iter().any(|c| c == "..") {
                continue;
            }
            roots.push(components);
        }
        Self { roots }
    }

    /// 把请求路径解析为可读路径：白名单内返回原样路径，越界返回 [`PathDenied`]。
    pub fn resolve(&self, requested: &str) -> Result<PathBuf, PathDenied> {
        let components = normalize(requested);
        if components.is_empty() || components.iter().any(|c| c == "..") {
            return Err(PathDenied::new(requested));
        }
        if !self
            .roots
            .iter()
            .any(|root| is_prefix(root, &components))
        {
            return Err(PathDenied::new(requested));
        }
        Ok(PathBuf::from(requested))
    }
}

/// 组件级前缀包含：`root` 必须是 `path` 的完整前缀（`QQ` 不匹配 `QQBackup`）。
fn is_prefix(root: &[String], path: &[String]) -> bool {
    root.len() <= path.len() && root.iter().zip(path.iter()).all(|(r, p)| r == p)
}

/// 归一化：`/` 与 `\` 等价、丢弃空段与 `.`，并保留首个前导分隔符作为"根"哨兵，
/// 使绝对路径与相对路径不可能互相匹配（`\srv\share` 不会等于 `srv/share`）。
fn normalize(raw: &str) -> Vec<String> {
    let mut components: Vec<String> = Vec::new();
    let rooted = raw.starts_with('/') || raw.starts_with('\\');
    for part in raw.split(['/', '\\']) {
        if part.is_empty() {
            if rooted && components.is_empty() {
                components.push(String::new());
            }
            continue;
        }
        if part == "." {
            continue;
        }
        components.push(part.to_string());
    }
    components
}
