//! 转换器管理命令与安装路径包含性校验（B4.1 / S5 / 簇 1）。

use std::fs;
use std::path::{Component, Path, PathBuf};

fn is_valid_converter_id(id: &str) -> bool {
    let len = id.len();
    if !(2..=64).contains(&len) {
        return false;
    }
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// 校验转换器安装目录严格位于受信任 `converters_root` 下：
/// 1. `converter_id` 必须符合 `^[a-z0-9][a-z0-9-]{1,63}$`；
/// 2. 路径不得含 `..` 或符号链接；
/// 3. `canonicalize` 后必须严格以 `canonicalize(converters_root)` 为直接父目录。
pub fn validate_converter_install_path(
    converters_root: &Path,
    converter_id: &str,
    candidate_dir: &Path,
) -> Result<PathBuf, String> {
    if !is_valid_converter_id(converter_id) {
        return Err(format!("非法转换器 id：{converter_id}"));
    }
    if candidate_dir
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err("转换器路径不得包含 ..".into());
    }

    fs::create_dir_all(converters_root).map_err(|e| format!("无法初始化转换器根目录：{e}"))?;
    let canon_root = fs::canonicalize(converters_root)
        .map_err(|e| format!("无法解析转换器根目录：{e}"))?;

    let meta = fs::symlink_metadata(candidate_dir)
        .map_err(|_| "转换器目标目录不存在".to_string())?;
    if meta.file_type().is_symlink() {
        return Err("拒绝加载符号链接转换器目录".into());
    }

    let canon_target = fs::canonicalize(candidate_dir)
        .map_err(|e| format!("无法规范化转换器目录：{e}"))?;

    if !canon_target.starts_with(&canon_root) || canon_target.parent() != Some(canon_root.as_path()) {
        return Err("转换器目录越出受管 converters 根目录，拒绝安装".into());
    }
    if canon_target.file_name().and_then(|s| s.to_str()) != Some(converter_id) {
        return Err("转换器目录名与 manifest.id 不一致".into());
    }

    let manifest_path = canon_target.join("manifest.json");
    let manifest_meta = fs::symlink_metadata(&manifest_path)
        .map_err(|_| "转换器目录缺少 manifest.json".to_string())?;
    if manifest_meta.file_type().is_symlink() || !manifest_meta.is_file() {
        return Err("manifest.json 必须是普通文件且不得为符号链接".into());
    }

    Ok(canon_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment_check_accepts_direct_child_and_rejects_traversal() {
        let base = std::env::temp_dir().join(format!("aibridge-conv-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("converters");
        let qq_dir = root.join("qq");
        let outside_dir = base.join("outside");
        fs::create_dir_all(&qq_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(qq_dir.join("manifest.json"), "{}").unwrap();
        fs::write(outside_dir.join("manifest.json"), "{}").unwrap();

        assert!(validate_converter_install_path(&root, "qq", &qq_dir).is_ok());
        assert!(validate_converter_install_path(&root, "../outside", &outside_dir).is_err());
        assert!(validate_converter_install_path(&root, "qq", &outside_dir).is_err());
        assert!(validate_converter_install_path(&root, "qq", &root.join("..").join("outside")).is_err());

        let _ = fs::remove_dir_all(&base);
    }
}
