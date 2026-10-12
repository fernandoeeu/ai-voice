import Darwin
import Foundation

/// Quem está falando agora: os processos que seguram o fala.lock (lockf, o script, afplay ou say).
enum Speaker {
    static let spokenFile = ProcessInfo.processInfo.environment["ESCUTA_FALA_TXT"]
        ?? NSHomeDirectory() + "/.local/share/jarvis/fala.txt"

    struct Proc { let pid: Int32; let ppid: Int32; let args: String }

    private static func run(_ path: String, _ args: [String]) -> String {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: path)
        p.arguments = args
        let out = Pipe()
        p.standardOutput = out
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { return "" }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        return String(data: data, encoding: .utf8) ?? ""
    }

    /// Processos que têm o lock aberto, e todos os descendentes deles.
    static func tree(lock: String) -> [Proc] {
        let me = getpid()
        let holders = Set(run("/usr/sbin/lsof", ["-t", "--", lock]).split(whereSeparator: \.isNewline)
            .compactMap { Int32($0.trimmingCharacters(in: .whitespaces)) }.filter { $0 != me })
        guard !holders.isEmpty else { return [] }
        let all: [Proc] = run("/bin/ps", ["-A", "-o", "pid=,ppid=,args="]).split(whereSeparator: \.isNewline).compactMap { line in
            let parts = line.split(separator: " ", maxSplits: 2, omittingEmptySubsequences: true)
            guard parts.count >= 2, let pid = Int32(parts[0]), let ppid = Int32(parts[1]) else { return nil }
            return Proc(pid: pid, ppid: ppid, args: parts.count > 2 ? String(parts[2]) : "")
        }
        var keep = holders
        var grew = true
        while grew {
            grew = false
            for p in all where !keep.contains(p.pid) && keep.contains(p.ppid) && p.pid != me {
                keep.insert(p.pid); grew = true
            }
        }
        return all.filter { keep.contains($0.pid) }
    }

    /// O texto que o Jarvis está falando: o que o servidor de voz acabou de sintetizar, ou o argumento do `say`.
    static func currentText(lock: String) -> String? {
        if let d = FileManager.default.contents(atPath: spokenFile),
           let o = try? JSONSerialization.jsonObject(with: d) as? [String: Any],
           let ts = (o["ts"] as? NSNumber)?.doubleValue, Date().timeIntervalSince1970 - ts < 30,
           let t = o["text"] as? String, !t.isEmpty {
            return t
        }
        for p in tree(lock: lock) where p.args.hasPrefix("/usr/bin/say ") || p.args.hasPrefix("say ") {
            if let r = p.args.range(of: " -- ") { return String(p.args[r.upperBound...]) }
            return p.args
        }
        return nil
    }

    /// Cala o Jarvis: SIGTERM nos tocadores e em quem segura o lock (o script não cai para o `say`).
    /// Devolve os comandos encerrados.
    @discardableResult
    static func stop(lock: String) -> [String] {
        let procs = tree(lock: lock)
        // Filhos primeiro (afplay/say), depois os pais (script, lockf).
        let ordered = procs.sorted { $0.pid > $1.pid }
        var killed: [String] = []
        for p in ordered where kill(p.pid, SIGTERM) == 0 {
            killed.append("\(p.pid) \(p.args.split(separator: " ").first.map(String.init) ?? "?")")
        }
        return killed
    }
}
