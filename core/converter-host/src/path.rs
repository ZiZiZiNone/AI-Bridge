//! 只读路径守卫：`readable_paths` 之外一律拒绝。
//!
//! 组件级前缀包含判定：`…\QQ\nt_data` 过、兄弟目录 `…\QQBackup` 拒；含 `..` 直接拒。
//! 大小写按字节精确比较，不一致的合法路径会被过度拒绝（fail-closed）。
//! 不做文件系统调用：只判"能不能读"，打开动作由沙盒运行器执行。

use crate::error::PathDenied;
use std::path::PathBuf;

pub struct PathGuard {
    roots: Vec<Vec<String>>,
}

impl PathGuard {
    /// 空声明或无有效声明时，守卫拒绝一切请求。
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

/// 完整前缀包含：`QQ` 不匹配 `QQBackup`。
fn is_prefix(root: &[String], path: &[String]) -> bool {
    root.len() <= path.len() && root.iter().zip(path.iter()).all(|(r, p)| r == p)
}

/// `/` 与 `\` 等价、丢空段与 `.`；保留前导分隔符作"根"哨兵，绝对与相对路径互不匹配。
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
