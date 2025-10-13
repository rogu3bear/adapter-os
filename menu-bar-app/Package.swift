// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "AdapterOSMenu",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(
            name: "AdapterOSMenu",
            targets: ["AdapterOSMenu"]
        ),
    ],
    targets: [
        .executableTarget(
            name: "AdapterOSMenu",
            dependencies: [],
            path: "Sources/AdapterOSMenu"
        ),
    ]
)




