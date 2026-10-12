import EscutaCore
import Foundation

/// Ajustes por variável de ambiente (o LaunchAgent define as suas) e, nos testes, por argumento.
struct Config {
    var endpoint = EndpointConfig()
    /// Janela depois de "Jarvis" sozinho em que a próxima frase vale como pedido.
    var armedWindow: Double = 60
    /// Depois de entregar um pedido, fala começada até este tempo também é pedido (mensagem nova, inteira).
    var conversationWindow: Double = 8
    /// Microfones em ordem de preferência (parte do nome, separados por vírgula); nenhum presente, o embutido.
    var micHint = "HyperX,iPhone,MacBook"
    /// Painel do herdr do orquestrador.
    var pane = "wT2:p2"
    /// Nome do agente do orquestrador no herdr, usado se o painel mudar de id.
    var orchestratorName = "maestro"
    var herdr = NSHomeDirectory() + "/.local/bin/herdr"
    var speechLock = NSHomeDirectory() + "/.local/share/jarvis/fala.lock"
    /// Depois que o Jarvis para de falar, o eco ainda pode chegar por isto: a fala conta como misturada.
    var speechTail: Double = 1.0
    /// Troca preventiva da tarefa de reconhecimento quando ninguém está falando.
    var rotateEvery: Double = 600
    /// Em vez de mandar ao herdr, acrescenta cada pedido (JSON por linha) a este arquivo.
    var outputFile: String?
    /// Teste: alimenta o reconhecedor com este arquivo de áudio, em tempo real, em vez do microfone.
    var audioFile: String?
    /// Teste: depois do arquivo, segue alimentando silêncio por isto e sai.
    var trailingSilence: Double = 12
    var sound = true
    var headless = false

    static func load(_ args: [String] = CommandLine.arguments, env: [String: String] = ProcessInfo.processInfo.environment) -> Config {
        var c = Config()
        func num(_ k: String) -> Double? { env[k].flatMap { Double($0.replacingOccurrences(of: ",", with: ".")) } }
        if let v = num("ESCUTA_SILENCIO") { c.endpoint.silence = v }
        if let v = num("ESCUTA_SILENCIO_ABERTA") { c.endpoint.openSilence = v }
        if let v = num("ESCUTA_ESPERA_FINAL") { c.endpoint.finalizeWait = v }
        if let v = num("ESCUTA_MAX_FALA") { c.endpoint.maxUtterance = v }
        if let v = num("ESCUTA_JANELA") { c.armedWindow = v }
        if let v = num("ESCUTA_CONVERSA") { c.conversationWindow = v }
        if let v = num("ESCUTA_CAUDA_FALA") { c.speechTail = v }
        if let v = num("ESCUTA_ROTACAO") { c.rotateEvery = v }
        if let v = env["ESCUTA_MIC"], !v.isEmpty { c.micHint = v }
        if let v = env["ESCUTA_PANE"] ?? env["ESCUTA_FALLBACK_PANE"], !v.isEmpty { c.pane = v }
        if let v = env["ESCUTA_ORQUESTRADOR"], !v.isEmpty { c.orchestratorName = v }
        if let v = env["ESCUTA_HERDR"], !v.isEmpty { c.herdr = v }
        if let v = env["ESCUTA_LOCK"], !v.isEmpty { c.speechLock = v }
        if let v = env["ESCUTA_SAIDA"], !v.isEmpty { c.outputFile = v }

        var i = 1
        while i < args.count {
            let a = args[i]
            let next = i + 1 < args.count ? args[i + 1] : nil
            switch a {
            case "--saida": c.outputFile = next; i += 1
            case "--audio": c.audioFile = next; c.headless = true; c.sound = false; i += 1
            case "--silencio-final": c.trailingSilence = next.flatMap(Double.init) ?? c.trailingSilence; i += 1
            case "--sem-som": c.sound = false
            case "--sem-interface": c.headless = true
            default: break
            }
            i += 1
        }
        return c
    }
}

func emit(_ obj: [String: Any]) {
    var o = obj
    o["ts"] = ISO8601DateFormatter().string(from: Date())
    if let d = try? JSONSerialization.data(withJSONObject: o, options: [.sortedKeys]), let s = String(data: d, encoding: .utf8) {
        print(s)
        fflush(stdout)
    }
}

func logLine(_ s: String) {
    let ts = ISO8601DateFormatter().string(from: Date())
    FileHandle.standardError.write("\(ts) [escuta] \(s)\n".data(using: .utf8)!)
}
