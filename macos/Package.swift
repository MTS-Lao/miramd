// swift-tools-version: 6.0

import Foundation
import PackageDescription

let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repositoryRoot = packageDirectory.deletingLastPathComponent()
let coreLibraryDirectory = ProcessInfo.processInfo.environment["MIRAMD_CORE_LIB_DIR"]
    ?? repositoryRoot.appendingPathComponent("target/debug").path

let package = Package(
    name: "MiraMD",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "MiraMD", targets: ["MiraMD"]),
    ],
    targets: [
        .executableTarget(
            name: "MiraMD",
            resources: [
                .copy("Resources/Fonts"),
            ],
            linkerSettings: [
                .linkedFramework("WebKit"),
                .unsafeFlags([
                    "-L", coreLibraryDirectory,
                    "-lmiramd_core",
                    "-Xlinker", "-rpath",
                    "-Xlinker", coreLibraryDirectory,
                    "-Xlinker", "-rpath",
                    "-Xlinker", "@executable_path/../Frameworks",
                ]),
            ]
        ),
    ]
)
