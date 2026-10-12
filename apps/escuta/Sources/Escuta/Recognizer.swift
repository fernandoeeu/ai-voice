import AVFoundation
import Foundation
import Speech

/// Reconhecimento contínuo em pt-BR que não morre: quando a tarefa da Apple termina ou falha,
/// abre outra; o áudio que chega no meio fica guardado e vai para a nova, sem buraco.
final class Recognizer {
    struct Event {
        let generation: Int; let text: String; let isFinal: Bool
        /// Confiança média do reconhecedor neste trecho (0 a 1); nil se ele não informou.
        let confidence: Double?
    }

    let locale = Locale(identifier: "pt-BR")
    private(set) var format: AVAudioFormat?
    private let lock = NSLock()
    private var continuation: AsyncStream<AnalyzerInput>.Continuation?
    private var backlog: [AVAudioPCMBuffer] = []
    private var backlogFrames = 0
    private var analyzer: SpeechAnalyzer?
    private(set) var generation = 0
    private(set) var startedAt = Date()
    private var restarting = false
    private var pendingRestart = false
    private var failures = 0

    /// Chamados na thread principal.
    var onEvent: ((Event) -> Void)?
    var onState: ((String) -> Void)?
    /// A tarefa caiu sozinha e outra vai abrir.
    var onRestart: (() -> Void)?

    /// Formato que o transcritor quer. Chamar uma vez antes de alimentar.
    @MainActor
    func prepare() async throws -> AVAudioFormat {
        if let f = format { return f }
        let t = makeTranscriber()
        guard let f = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [t]) else {
            throw NSError(domain: "escuta", code: 1, userInfo: [NSLocalizedDescriptionKey: "Sem formato de áudio para o transcritor"])
        }
        format = f
        return f
    }

    private func makeTranscriber() -> SpeechTranscriber {
        SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [.volatileResults, .fastResults], attributeOptions: [.transcriptionConfidence])
    }

    /// Pede ao reconhecedor os resultados finais de tudo o que já recebeu, sem esperar ele decidir sozinho
    /// (sozinho ele chega a levar ~10 s para fechar um trecho).
    @MainActor
    func finalizeNow() async {
        guard let a = analyzer else { return }
        do { try await a.finalize(through: nil) } catch { logLine("finalize: \(error.localizedDescription)") }
    }

    /// Thread de áudio. Sem tarefa ativa, guarda até ~30 s para a próxima.
    func feed(_ buffer: AVAudioPCMBuffer) {
        lock.lock()
        defer { lock.unlock() }
        if let c = continuation {
            c.yield(AnalyzerInput(buffer: buffer))
            return
        }
        backlog.append(buffer)
        backlogFrames += Int(buffer.frameLength)
        let limit = Int((format?.sampleRate ?? 16000) * 30)
        while backlogFrames > limit, !backlog.isEmpty {
            backlogFrames -= Int(backlog.removeFirst().frameLength)
        }
    }

    /// Abre uma tarefa nova e passa a alimentá-la; a anterior é encerrada depois da troca.
    @MainActor
    func start(reason: String) async {
        guard !restarting else { return }
        restarting = true
        startedAt = Date()
        defer {
            restarting = false
            if pendingRestart {
                pendingRestart = false
                Task { @MainActor in await self.start(reason: "a tarefa nova caiu enquanto abria") }
            }
        }
        do {
            _ = try await prepare()
            let transcriber = makeTranscriber()
            let next = SpeechAnalyzer(modules: [transcriber])
            let (stream, cont) = AsyncStream<AnalyzerInput>.makeStream()
            generation += 1
            let g = generation
            Task { [weak self] in
                var why = "fim dos resultados"
                do {
                    for try await r in transcriber.results {
                        let text = String(r.text.characters)
                        var sum = 0.0, n = 0
                        for run in r.text.runs {
                            if let c = run.transcriptionConfidence { sum += c; n += 1 }
                        }
                        let conf: Double? = n > 0 ? sum / Double(n) : nil
                        await MainActor.run { self?.onEvent?(Event(generation: g, text: text, isFinal: r.isFinal, confidence: conf)) }
                    }
                } catch {
                    why = "erro: \(error.localizedDescription)"
                }
                await MainActor.run { self?.ended(generation: g, why: why) }
            }
            try await next.start(inputSequence: stream)
            let old = analyzer
            analyzer = next
            let (oldCont, replayed) = lock.withLock { () -> (AsyncStream<AnalyzerInput>.Continuation?, Int) in
                let o = continuation
                continuation = cont
                for b in backlog { cont.yield(AnalyzerInput(buffer: b)) }
                let n = backlog.count
                backlog = []; backlogFrames = 0
                return (o, n)
            }
            oldCont?.finish()
            if let old { Task { await old.cancelAndFinishNow() } }
            startedAt = Date()
            failures = 0
            logLine("reconhecimento #\(g) iniciado (\(reason)); \(replayed) blocos de áudio repassados")
            emit(["type": "recognizer", "generation": g, "reason": reason, "replayed": replayed])
            onState?("ok")
        } catch {
            failures += 1
            let wait = min(30.0, 0.5 * pow(2.0, Double(failures - 1)))
            logLine("falha ao iniciar o reconhecimento (\(error.localizedDescription)); nova tentativa em \(wait) s")
            emit(["type": "recognizer_error", "message": error.localizedDescription, "retry_in": wait])
            onState?("erro: \(error.localizedDescription)")
            DispatchQueue.main.asyncAfter(deadline: .now() + wait) { [weak self] in
                Task { @MainActor in await self?.start(reason: "nova tentativa") }
            }
        }
    }

    /// A tarefa atual acabou sozinha (limite da Apple, erro): segura o áudio e abre outra já.
    @MainActor
    private func ended(generation g: Int, why: String) {
        guard g == generation else { return }  // troca planejada: a velha acabar é o esperado
        lock.withLock {
            continuation?.finish()
            continuation = nil
        }
        if restarting { pendingRestart = true }
        logLine("reconhecimento #\(g) terminou (\(why)); reiniciando")
        onRestart?()
        Task { @MainActor in await self.start(reason: "a tarefa anterior terminou: \(why)") }
    }
}
