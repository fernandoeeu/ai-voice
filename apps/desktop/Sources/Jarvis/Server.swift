import Foundation

/// Confere o servidor do dashboard (LaunchAgent dev.jarvis.dashboard) e o sobe pelo launchd se estiver parado.
/// O app nunca roda o servidor por conta própria: quem o mantém vivo é o launchd, como antes.
struct Server {
    static let label = "dev.jarvis.dashboard"
    let url: URL

    init() {
        let env = ProcessInfo.processInfo.environment
        url = URL(string: env["JARVIS_DASHBOARD_URL"] ?? "http://127.0.0.1:7350/")!
    }

    private var domain: String { "gui/\(getuid())" }
    private var plist: String { NSHomeDirectory() + "/Library/LaunchAgents/\(Server.label).plist" }

    func isUp() async -> Bool {
        let request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 2)
        let session = URLSession(configuration: .ephemeral)
        defer { session.finishTasksAndInvalidate() }
        guard let (_, response) = try? await session.data(for: request),
              let http = response as? HTTPURLResponse else { return false }
        return http.statusCode < 500
    }

    /// true quando o servidor responde; se não, carrega/inicia o LaunchAgent e espera até `timeout` segundos.
    func ensureUp(timeout: Double = 20) async -> Bool {
        if await isUp() { return true }
        if launchctl("print", "\(domain)/\(Server.label)") != 0 {
            guard FileManager.default.fileExists(atPath: plist) else { return false }
            _ = launchctl("bootstrap", domain, plist)
        }
        _ = launchctl("kickstart", "\(domain)/\(Server.label)")
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            try? await Task.sleep(nanoseconds: 500_000_000)
            if await isUp() { return true }
        }
        return false
    }

    /// Sobe todos os LaunchAgents dev.jarvis.* de ~/Library/LaunchAgents: carrega os descarregados e dá kickstart
    /// (sem -k, não mexe no que já está rodando) para voltar quem foi encerrado à mão, como a escuta e o tts.
    func ensureAllAgents() {
        let dir = NSHomeDirectory() + "/Library/LaunchAgents"
        let plists = ((try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? [])
            .filter { $0.hasPrefix("dev.jarvis.") && $0.hasSuffix(".plist") }
        for file in plists {
            let label = String(file.dropLast(".plist".count))
            if launchctl("print", "\(domain)/\(label)") != 0 {
                _ = launchctl("bootstrap", domain, dir + "/" + file)
            }
            _ = launchctl("kickstart", "\(domain)/\(label)")
        }
    }

    /// Reinicia o servidor (kickstart -k) e espera ele voltar.
    func restart() async -> Bool {
        _ = launchctl("kickstart", "-k", "\(domain)/\(Server.label)")
        try? await Task.sleep(nanoseconds: 1_000_000_000)
        return await ensureUp()
    }

    @discardableResult
    private func launchctl(_ args: String...) -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        process.arguments = args
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return -1 }
        process.waitUntilExit()
        return process.terminationStatus
    }
}
