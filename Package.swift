// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "29er",
    platforms: [
        .iOS(.v18)
    ],
    targets: [
        .executableTarget(
            name: "29er",
            dependencies: [],
            path: "Sources"
        )
    ]
)
