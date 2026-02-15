fn main() {
    // 使用 embed-resource 来嵌入图标和清单文件
    embed_resource::compile("resources.rc", embed_resource::NONE);
}