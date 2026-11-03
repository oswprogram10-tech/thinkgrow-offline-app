// swift-tools-version:5.3
import PackageDescription
let package = Package(name: "tauri-plugin-thinkgrow-vault", platforms: [.iOS(.v13)],
    products: [.library(name: "tauri-plugin-thinkgrow-vault", type: .static, targets: ["tauri-plugin-thinkgrow-vault"])],
    dependencies: [.package(name: "Tauri", path: "../.tauri/tauri-api")],
    targets: [.target(name: "tauri-plugin-thinkgrow-vault", dependencies: [.byName(name: "Tauri")], path: "Sources")])
