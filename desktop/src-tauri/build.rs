//! Tauri 2 构建脚本（标准形态）：确保前端 dist 占位目录存在后生成上下文与 capabilities schema。

fn main() {
    let dist_dir = std::path::Path::new("../dist");
    if !dist_dir.exists() {
        let _ = std::fs::create_dir_all(dist_dir);
        let _ = std::fs::write(
            dist_dir.join("index.html"),
            "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><title>AI-Bridge</title></head><body><div id=\"app\"></div></body></html>\n",
        );
    }
    tauri_build::build();
}
