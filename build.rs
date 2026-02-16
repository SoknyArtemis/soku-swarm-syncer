fn main() {
    // 只在目标系统为 Windows 时尝试嵌入资源
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        // 使用 let _ = 显式忽略 Result 返回值，消除警告
        let _ = embed_resource::compile("resources.rc", embed_resource::NONE);
    }
}