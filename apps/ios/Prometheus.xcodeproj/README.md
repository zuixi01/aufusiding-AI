# Xcode Project Placeholder

此目录应包含 Xcode 项目文件。请使用 Xcode 创建新的 iOS App 项目。

## 创建步骤

1. 打开 Xcode
2. File → New → Project
3. 选择 iOS → App
4. Product Name: Prometheus
5. Interface: SwiftUI
6. Language: Swift
7. 保存到此目录

## 集成 Rust 库

在 Build Phases 中添加：
- Link Binary with Libraries: libprometheus_ffi.a
- 设置 Library Search Paths 指向 Rust 编译输出目录
