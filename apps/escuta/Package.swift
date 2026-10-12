// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "escuta",
    platforms: [.macOS("26.0")],
    targets: [
        // Regras puras (palavra de ativação, fim de fala, lock da fala), testáveis sem microfone.
        .target(name: "EscutaCore"),
        .executableTarget(
            name: "Escuta",
            dependencies: ["EscutaCore"],
            linkerSettings: [
                // Info.plist embutido: o TCC lê daqui as frases de uso do microfone e da fala.
                .unsafeFlags(["-Xlinker", "-sectcreate", "-Xlinker", "__TEXT", "-Xlinker", "__info_plist",
                              "-Xlinker", Context.packageDirectory + "/Resources/Info.plist"]),
            ]
        ),
        .testTarget(name: "EscutaCoreTests", dependencies: ["EscutaCore"]),
    ],
    swiftLanguageModes: [.v5]
)
