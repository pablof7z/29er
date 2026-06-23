// swift-tools-version:5.9
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
            path: "Sources",
            resources: [
                .process("Resources")
            ]
        )
    ]
)
