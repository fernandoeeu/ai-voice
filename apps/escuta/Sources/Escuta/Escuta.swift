import AppKit
import AVFoundation
import CoreAudio
import EscutaCore
import Speech
import Combine

struct Heard: Identifiable {
    let id = UUID()
    let at: Date
    let text: String
    let wakeRange: Range<String.Index>?
    let command: String?
}

struct Command: Identifiable {
    let id: Int
    let at: Date
    let text: String
    var state: String = "enviado"   // enviado → entregue | falhou → recebido → respondido
    var reply: String?
    var target: String?
    var mode: String?
    var speechMs: Int?
    var deliverMs: Int?
}

enum Phase: Equatable {
    case starting, listening, hearing, armed, sent, working, paused, error

    var symbol: String {
        switch self {
        case .starting: return "ellipsis.circle"
        case .listening: return "waveform"
        case .hearing: return "waveform.badge.mic"
        case .armed: return "waveform.circle.fill"
        case .sent: return "paperplane.fill"
        case .working: return "ellipsis.bubble.fill"
        case .paused: return "mic.slash"
        case .error: return "exclamationmark.triangle.fill"
        }
    }
    var label: String {
        switch self {
        case .starting: return "Iniciando"
        case .listening: return "Ouvindo"
        case .hearing: return "Ouvindo voz"
        case .armed: return "Jarvis detectado, pode falar"
        case .sent: return "Pedido enviado ao Claude"
        case .working: return "Claude trabalhando"
        case .paused: return "Pausado"
        case .error: return "Erro"
        }
    }
}

/// Estado do áudio compartilhado com a thread de captura.
final class AudioGate: @unchecked Sendable {
    private let lock = NSLock()
    private var _paused = false
    var paused: Bool {
        get { lock.lock(); defer { lock.unlock() }; return _paused }
        set { lock.lock(); _paused = newValue; lock.unlock() }
    }
}

final class Escuta: ObservableObject {
    @Published var phase: Phase = .starting
    @Published var levels: [Float] = Array(repeating: 0, count: 56)
    @Published var partial = ""
    @Published var heard: [Heard] = []
    @Published var commands: [Command] = []
    @Published var micName = "—"
    @Published var airpodsName: String?
    @Published var airpodsRate: Double = 0
    @Published var errorText: String?
    @Published var armedUntil: Date?
    @Published var lastWakeAt: Date?
    @Published var diag = "áudio: aguardando"
    @Published var waiting: [WaitItem] = []
    @Published var silentFor = 0
    @Published var jarvisSpeaking = false
    /// A fala em andamento é um pedido (começou com "Jarvis" ou veio na janela depois dele).
    @Published var capturing = false
    @Published var recognizerState = "iniciando"

    let config: Config
    let router: Router
    let recognizer = Recognizer()
    let gate = AudioGate()
    private let mic = MicCapture()
    private var converter: AVAudioConverter?   // só na thread de captura
    private var nextID = 1
    private var claudeOffset: UInt64 = 0
    private var phaseReset: DispatchWorkItem?
    private var lastPartialAt: Date?
    private var buffersIn = 0, framesOut = 0, convErrors = 0, partials = 0, finals = 0
    private var peakDB: Float = -120
    private var noiseFloor: Float = -60
    private var formats = ""
    private let startedAt = Date()
    private var audioStartedAt: Date?
    private var lastDiagLog = Date.distantPast
    private var fileFeedDone = false

    // Fim de fala
    private var segments: [String] = []
    private var livePartial = ""
    private var lastWhole = ""
    private var utteranceStart: Date?
    private var lastTextChangeAt = Date.distantPast
    private var lastVoiceAt = Date.distantPast
    private var lastEventAt = Date()
    private var spokenText: String?
    private var bargedIn = false
    private var speechEndedAt = Date.distantPast
    private var overlapped = false
    private var finalizeAt: Date?
    private var finalizeDone = false
    private let lockQueue = DispatchQueue(label: "escuta.fala-lock")

    var isPaused: Bool { phase == .paused }

    init(config: Config) {
        self.config = config
        self.router = Router(config: config)
        // Status antigo do claude.jsonl não vale para os pedidos deste processo (os ids recomeçam em 1).
        let status = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent("claude.jsonl")
        claudeOffset = (try? FileManager.default.attributesOfItem(atPath: status.path)[.size] as? UInt64) ?? 0
        recognizer.onEvent = { [weak self] e in MainActor.assumeIsolated { self?.onTranscript(e) } }
        recognizer.onState = { [weak self] s in self?.recognizerState = s }
        recognizer.onRestart = { [weak self] in self?.keepVolatileText() }
        Task { @MainActor in await self.start() }
        Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in MainActor.assumeIsolated { self?.tick() } }
        Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in self?.reportDiag() }
        if config.audioFile == nil {
            Timer.scheduledTimer(withTimeInterval: 1.5, repeats: true) { [weak self] _ in self?.checkAirPods() }
            Timer.scheduledTimer(withTimeInterval: 0.4, repeats: true) { [weak self] _ in self?.readClaudeStatus() }
            Timer.scheduledTimer(withTimeInterval: 1.5, repeats: true) { [weak self] _ in self?.refreshWaiting() }
        }
        pollSpeechLock()
    }

    // MARK: Ciclo de vida

    @MainActor
    func start() async {
        logLine("iniciando; silêncio \(config.endpoint.silence) s (\(config.endpoint.openSilence) s em frase aberta), "
                + "destino \(config.outputFile ?? "herdr \(config.pane)")")
        if let file = config.audioFile { await startFromFile(file); return }

        watchAudioSystem()
        checkAirPods()

        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: break
        case .notDetermined:
            logLine("pedindo acesso ao microfone (o macOS mostra um aviso)")
            if !(await AVCaptureDevice.requestAccess(for: .audio)) { waitForMicPermission(); return }
        default:
            waitForMicPermission(); return
        }
        let speechAuth = SFSpeechRecognizer.authorizationStatus()
        if speechAuth == .notDetermined {
            logLine("pedindo acesso ao reconhecimento de fala")
            let s = await withCheckedContinuation { c in SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) } }
            logLine("reconhecimento de fala: \(s.rawValue)")
        } else {
            logLine("reconhecimento de fala: \(speechAuth.rawValue) (3 = autorizado)")
        }

        let fmt: AVAudioFormat
        do { fmt = try await recognizer.prepare() } catch {
            fail("Transcritor: \(error.localizedDescription)")
            restart(reason: "transcritor indisponível", after: 30)
            return
        }
        await recognizer.start(reason: "início")

        guard let device = MicCapture.pick(hint: config.micHint) else {
            fail("Nenhum microfone encontrado")
            restart(reason: "sem microfone", after: 30)
            return
        }
        micName = device.localizedName
        mic.onBuffer = { [weak self] buf in self?.process(buf, to: fmt) }
        do { try mic.start(device) } catch {
            fail("Microfone: \(error.localizedDescription)")
            restart(reason: "mic não abriu", after: 15)
            return
        }
        formats = "mic \(device.localizedName) → \(Int(fmt.sampleRate)) Hz"
        logLine("captura: \(formats)")
        audioStartedAt = Date()
        phase = .listening
        emit(["type": "ready", "mic": micName, "wake_word": "jarvis", "locale": "pt-BR",
              "silence": config.endpoint.silence, "open_silence": config.endpoint.openSilence])
    }

    /// Sem permissão do microfone: fica de pé avisando e confere a cada 10 s; liberou, recomeça.
    private func waitForMicPermission() {
        fail("Libere o microfone para “Escuta” em Ajustes do Sistema > Privacidade e Segurança > Microfone")
        Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] t in
            if AVCaptureDevice.authorizationStatus(for: .audio) == .authorized {
                t.invalidate()
                self?.restart(reason: "microfone liberado")
            }
        }
    }

    /// Captura → medidor → (silêncio se o Jarvis fala) → conversão → reconhecedor. Thread de áudio.
    private func process(_ buffer: AVAudioPCMBuffer, to fmt: AVAudioFormat) {
        if gate.paused { return }
        if converter?.inputFormat != buffer.format { converter = AVAudioConverter(from: buffer.format, to: fmt) }
        guard let converter else { return }
        meter(buffer)
        let cap = AVAudioFrameCount(Double(buffer.frameLength) * fmt.sampleRate / buffer.format.sampleRate) + 64
        guard let out = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: cap) else { return }
        var fed = false
        var e: NSError?
        converter.convert(to: out, error: &e) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true; status.pointee = .haveData; return buffer
        }
        let ok = e == nil
        let n = Int(out.frameLength)
        DispatchQueue.main.async {
            self.buffersIn += 1
            if ok { self.framesOut += n } else { self.convErrors += 1 }
        }
        if ok, n > 0 { recognizer.feed(out) }
    }

    // MARK: Teste com arquivo de áudio

    @MainActor
    private func startFromFile(_ path: String) async {
        let file: AVAudioFile
        do { file = try AVAudioFile(forReading: URL(fileURLWithPath: path)) } catch {
            logLine("não abri \(path): \(error.localizedDescription)"); exit(2)
        }
        let fmt: AVAudioFormat
        do { fmt = try await recognizer.prepare() } catch { logLine("transcritor: \(error)"); exit(2) }
        await recognizer.start(reason: "início (arquivo)")
        let src = file.processingFormat
        micName = "arquivo \(URL(fileURLWithPath: path).lastPathComponent)"
        formats = "arquivo \(Int(src.sampleRate)) Hz → \(Int(fmt.sampleRate)) Hz"
        phase = .listening
        audioStartedAt = Date()
        emit(["type": "ready", "mic": micName, "silence": config.endpoint.silence])
        let chunk = AVAudioFrameCount(src.sampleRate / 10)
        var silenceLeft = Int(config.trailingSilence * 10)
        let q = DispatchQueue(label: "escuta.arquivo")
        let timer = DispatchSource.makeTimerSource(queue: q)
        timer.schedule(deadline: .now(), repeating: 0.1)
        timer.setEventHandler { [weak self] in
            guard let self, let buf = AVAudioPCMBuffer(pcmFormat: src, frameCapacity: chunk) else { return }
            if file.framePosition < file.length {
                try? file.read(into: buf, frameCount: chunk)
            } else if silenceLeft > 0 {
                silenceLeft -= 1
                buf.frameLength = chunk
                for b in UnsafeMutableAudioBufferListPointer(buf.mutableAudioBufferList) where b.mData != nil {
                    memset(b.mData, 0, Int(b.mDataByteSize))
                }
            } else {
                timer.cancel()
                DispatchQueue.main.async { self.finishFileRun() }
                return
            }
            self.process(buf, to: fmt)
        }
        timer.resume()
        fileTimer = timer
    }
    private var fileTimer: DispatchSourceTimer?
    private var lastVoiceAtForCommand: [Int: Date] = [:]

    @MainActor
    private func finishFileRun() {
        if !(segments.isEmpty && livePartial.isEmpty) { flush(reason: "fim do arquivo") }
        router.q.async {
            DispatchQueue.main.async {
                emit(["type": "stopped", "reason": "fim do arquivo"])
                exit(0)
            }
        }
    }

    // MARK: Controles

    func togglePause() {
        if isPaused {
            gate.paused = false
            phase = .listening; emit(["type": "resumed"])
        } else {
            gate.paused = true
            segments = []; livePartial = ""; utteranceStart = nil
            phase = .paused; partial = ""; emit(["type": "paused"])
        }
    }

    /// Sai para o launchd abrir de novo do zero (mic, transcritor e tap): mais confiável que remontar o AVAudioEngine.
    private var lastRestart = Date.distantPast
    func restart(reason: String, after delay: TimeInterval = 0) {
        guard config.audioFile == nil else { return }
        guard Date().timeIntervalSince(lastRestart) > 20 || delay > 0 else { return }
        lastRestart = Date()
        emit(["type": "restart", "reason": reason, "in": delay])
        logLine("reiniciando o processo: \(reason)")
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { exit(75) }
    }

    /// Sai de verdade (código 0): o LaunchAgent só reabre depois de falha ou no próximo login.
    func quit() {
        emit(["type": "stopped", "reason": "Sair"])
        logLine("encerrada pelo Sair")
        exit(0)
    }

    private func fail(_ msg: String) {
        errorText = msg; phase = .error
        logLine("erro: \(msg)")
        emit(["type": "error", "message": msg])
    }

    // MARK: Áudio

    private func meter(_ buffer: AVAudioPCMBuffer) {
        guard let ch = buffer.floatChannelData?[0] else { return }
        let n = Int(buffer.frameLength)
        var sum: Float = 0
        for i in 0..<n { sum += ch[i] * ch[i] }
        let rms = sqrt(sum / Float(max(n, 1)))
        let db = 20 * log10(max(rms, 1e-6))
        let level = max(0, min(1, (db + 60) / 50))
        DispatchQueue.main.async {
            self.peakDB = max(self.peakDB, db)
            // Piso de ruído: desce rápido, sobe devagar. Voz = bem acima do piso.
            self.noiseFloor = db < self.noiseFloor ? db : min(db, self.noiseFloor + 0.05)
            if db > -55, db > self.noiseFloor + 12 { self.lastVoiceAt = Date() }
            self.levels.removeFirst()
            self.levels.append(level)
        }
    }

    /// coreaudiod reiniciou, o microfone sumiu ou a captura deu erro: sai e o launchd abre de novo do zero.
    private func watchAudioSystem() {
        var addr = Audio.address(kAudioHardwarePropertyServiceRestarted)
        AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &addr, DispatchQueue.main) { [weak self] _, _ in
            self?.restart(reason: "coreaudiod reiniciou")
        }
        let nc = NotificationCenter.default
        nc.addObserver(forName: AVCaptureSession.runtimeErrorNotification, object: nil, queue: .main) { [weak self] n in
            let e = n.userInfo?[AVCaptureSessionErrorKey] as? NSError
            self?.restart(reason: "erro na captura: \(e?.localizedDescription ?? "?")")
        }
        nc.addObserver(forName: AVCaptureDevice.wasDisconnectedNotification, object: nil, queue: .main) { [weak self] n in
            guard let self, let d = n.object as? AVCaptureDevice, d.uniqueID == self.mic.device?.uniqueID else { return }
            self.restart(reason: "microfone \(d.localizedName) desconectou")
        }
        nc.addObserver(forName: AVCaptureDevice.wasConnectedNotification, object: nil, queue: .main) { [weak self] n in
            guard let self, let d = n.object as? AVCaptureDevice, !MicCapture.isBluetooth(d),
                  d.localizedName.localizedCaseInsensitiveContains(self.config.micHint),
                  !self.micName.localizedCaseInsensitiveContains(self.config.micHint) else { return }
            self.restart(reason: "microfone \(d.localizedName) voltou")
        }
    }

    private func checkAirPods() {
        let out = Audio.defaultDevice(kAudioHardwarePropertyDefaultOutputDevice)
        guard Audio.isBluetooth(out) else { airpodsName = nil; return }
        let r = Audio.rate(out)
        if r != airpodsRate || airpodsName == nil {
            emit(["type": "airpods", "device": Audio.name(out), "rate": r, "profile": r >= 44100 ? "A2DP estéreo" : "HFP mono"])
        }
        airpodsName = Audio.name(out)
        airpodsRate = r
    }

    // MARK: Jarvis falando

    /// Confere o fala.lock a cada 50 ms. O reconhecedor continua ouvindo enquanto o Jarvis fala,
    /// para o dono poder interromper; o texto falado serve para reconhecer o eco.
    private func pollSpeechLock() {
        let path = config.speechLock
        lockQueue.async { [weak self] in
            var wasHeld = false
            while let self {
                let held = SpeechLock.isHeld(path)
                if held != wasHeld {
                    wasHeld = held
                    let spoken = held ? Speaker.currentText(lock: path) : nil
                    DispatchQueue.main.async {
                        self.jarvisSpeaking = held
                        if held {
                            self.spokenText = spoken
                            self.bargedIn = false
                        } else {
                            self.speechEndedAt = Date()
                        }
                        emit(["type": held ? "jarvis_speaking" : "jarvis_quiet", "text": spoken ?? ""])
                    }
                }
                Thread.sleep(forTimeInterval: 0.05)
            }
        }
    }

    /// O Jarvis está falando, ou acabou há pouco (o eco da sala ainda chega ao reconhecedor).
    private var inSpeech: Bool { jarvisSpeaking || Date().timeIntervalSince(speechEndedAt) < config.speechTail }

    /// O dono disse "Jarvis" (ou "para", "espera", "chega") por cima da fala: cala o Jarvis na hora.
    @MainActor
    private func checkBargeIn(_ text: String) {
        guard jarvisSpeaking, !bargedIn, !text.isEmpty else { return }
        // O trecho em revisão pode trazer o eco antes do "Jarvis": julga a partir da palavra de ativação.
        let candidate = Wake.lastCommand(in: text).map { String(text[$0.wake.lowerBound...]) } ?? text
        if BargeIn.isEcho(Wake.lastCommand(in: candidate)?.rest ?? candidate, spoken: spokenText) { return }
        guard let word = BargeIn.trigger(candidate, spoken: spokenText) else { return }
        bargedIn = true
        let path = config.speechLock
        // Fila própria: a lockQueue está ocupada para sempre com o laço que vigia o fala.lock.
        DispatchQueue.global(qos: .userInteractive).async {
            let killed = Speaker.stop(lock: path)
            DispatchQueue.main.async {
                logLine("interrompeu o Jarvis (\"\(word)\"); processos encerrados: \(killed)")
                emit(["type": "barge_in", "word": word, "heard": text, "killed": killed])
            }
        }
    }

    // MARK: Transcrição e fim de fala

    /// Decide se a frase vira pedido. arm = "Jarvis" sozinho.
    private func decide(_ text: String, startedAt: Date, overlapped: Bool) -> (wake: Range<String.Index>?, command: String?, arm: Bool) {
        if overlapped {
            // Fala misturada com a voz do Jarvis: só vale o que vem depois de "Jarvis"; sem janela de conversa.
            guard let c = Wake.lastCommand(in: text) else { return (nil, nil, false) }
            if BargeIn.isEcho(c.rest, spoken: spokenText) { return (nil, nil, false) }
            // "Jarvis, para" só cala o Jarvis; não vira pedido.
            if BargeIn.isOnlyStop(c.rest) { return (c.wake, nil, false) }
            return (c.wake, c.rest.isEmpty ? nil : c.rest, c.rest.isEmpty)
        }
        if let c = Wake.command(in: text) {
            return (c.wake, c.rest.isEmpty ? nil : c.rest, c.rest.isEmpty)
        }
        let rest = text.trimmingCharacters(in: Wake.edgeStrip)
        // Vale quando a fala começou dentro da janela ("Jarvis" sozinho, ou conversa logo depois de um pedido).
        if let until = armedUntil, startedAt < until { return (nil, rest, false) }
        if !waiting.isEmpty {
            // Com alguém esperando resposta, "sim", "pode" ou "não" soltos bastam.
            let n = Router.norm(text).joined(separator: " ")
            if Router.yes.contains(n) || Router.no.contains(n) { return (nil, rest, false) }
        }
        return (nil, nil, false)
    }

    private var whole: String { (segments + [livePartial]).filter { !$0.isEmpty }.joined(separator: " ") }

    @MainActor
    private func onTranscript(_ e: Recognizer.Event) {
        lastEventAt = Date()
        if gate.paused { return }
        let current = e.generation == recognizer.generation
        // Resultado parcial de uma tarefa já trocada: a nova é quem manda agora.
        if !current && !e.isFinal { return }
        let text = e.text.trimmingCharacters(in: .whitespacesAndNewlines)
        if e.isFinal { finals += 1 } else { partials += 1 }
        if e.isFinal {
            if !text.isEmpty { segments.append(text) }
            if current { livePartial = "" }
        } else {
            livePartial = text
        }
        let w = whole
        partial = w
        if ProcessInfo.processInfo.environment["ESCUTA_DEBUG"] != nil { logLine("evento final=\(e.isFinal) falando=\(jarvisSpeaking) texto=\(text)") }
        checkBargeIn(text)
        guard w != lastWhole else { return }
        lastWhole = w
        guard !w.isEmpty else { return }
        lastTextChangeAt = Date()
        lastPartialAt = Date()
        if inSpeech { overlapped = true }
        let isCommand = overlapped ? Wake.lastCommand(in: w) != nil : (Wake.leading(w) != nil || armedUntil != nil)
        if capturing != isCommand { capturing = isCommand }
        if utteranceStart == nil { utteranceStart = Date() }
        if phase == .listening { setPhase(.hearing, resetAfter: 1.5) }
        if Wake.leading(w) != nil, lastWakeAt.map({ Date().timeIntervalSince($0) > 2 }) ?? true { lastWakeAt = Date() }
    }

    /// A tarefa de reconhecimento caiu: o trecho ainda em revisão vira definitivo, para não sumir.
    private func keepVolatileText() {
        if !livePartial.isEmpty { segments.append(livePartial); livePartial = "" }
    }

    @MainActor
    private func tick() {
        let now = Date()
        // Expira a janela, mas não no meio de uma fala que começou dentro dela.
        if let until = armedUntil, now >= until, utteranceStart.map({ $0 >= until }) ?? true {
            armedUntil = nil
            if phase == .armed { phase = .listening }
        }
        if let start = utteranceStart, !whole.isEmpty {
            // Silêncio pelo medidor do microfone (tempo real). Se o medidor se confundir com ruído de fundo,
            // o texto parado também conta como silêncio, com 1,5 s de folga para o atraso do reconhecedor.
            let silence = max(now.timeIntervalSince(lastVoiceAt), now.timeIntervalSince(lastTextChangeAt) - 1.5)
            let activityAt = now.addingTimeInterval(-silence)
            let sinceFinalize = finalizeAt.flatMap { $0 > activityAt && finalizeDone ? now.timeIntervalSince($0) : nil }
            let d = Endpoint.decide(text: whole, volatile: !livePartial.isEmpty, silence: silence,
                                    sinceText: now.timeIntervalSince(lastTextChangeAt), sinceFinalize: sinceFinalize,
                                    elapsed: now.timeIntervalSince(start), config: config.endpoint)
            switch d {
            case .wait: break
            case .finalize:
                if let f = finalizeAt, f > activityAt { break }  // já pedido nesta pausa
                finalizeAt = now
                finalizeDone = false
                Task { @MainActor in
                    await recognizer.finalizeNow()
                    finalizeDone = true
                }
            case .close(let reason):
                flush(reason: reason)
            }
            return
        }
        guard audioStartedAt != nil, !gate.paused else { return }
        // Ninguém falando: troca preventiva da tarefa de reconhecimento, ou troca porque ela parou de responder.
        let quiet = now.timeIntervalSince(lastTextChangeAt) > 5 && now.timeIntervalSince(lastVoiceAt) > 3
        if quiet, now.timeIntervalSince(recognizer.startedAt) > config.rotateEvery {
            Task { @MainActor in await recognizer.start(reason: "troca preventiva") }
        } else if now.timeIntervalSince(lastVoiceAt) < 1, now.timeIntervalSince(lastEventAt) > 90,
                  now.timeIntervalSince(recognizer.startedAt) > 90 {
            lastEventAt = now
            Task { @MainActor in await recognizer.start(reason: "sem resultados há 90 s com voz no microfone") }
        }
    }

    /// Fim da fala: junta tudo e manda UMA mensagem.
    @MainActor
    private func flush(reason: String) {
        let text = whole.trimmingCharacters(in: .whitespacesAndNewlines)
        let began = utteranceStart ?? Date()
        let duration = Date().timeIntervalSince(began)
        segments = []; livePartial = ""; lastWhole = ""; utteranceStart = nil; partial = ""
        finalizeAt = nil
        capturing = false
        let wasOverlapped = overlapped
        overlapped = false
        guard !text.isEmpty else { return }
        let d = decide(text, startedAt: began, overlapped: wasOverlapped)
        if d.wake != nil { lastWakeAt = Date() }
        if d.arm {
            armedUntil = Date().addingTimeInterval(config.armedWindow)
            setPhase(.armed)
            emit(["type": "armed", "seconds": config.armedWindow])
        }
        heard.insert(Heard(at: Date(), text: text, wakeRange: d.wake, command: d.command), at: 0)
        if heard.count > 12 { heard.removeLast() }
        emit(["type": "heard", "text": text, "wake": d.wake != nil, "end": reason, "during_speech": wasOverlapped, "seconds": (duration * 10).rounded() / 10])
        if let c = d.command, !c.isEmpty { send(c) }
    }

    @MainActor
    private func send(_ text: String) {
        armedUntil = Date().addingTimeInterval(config.conversationWindow)
        if config.sound { NSSound(named: "Tink")?.play() }  // recebido, na hora
        let id = nextID; nextID += 1
        // fala→texto: do fim da fala (medidor do microfone) até o pedido pronto para enviar.
        let speechMs = Int(Date().timeIntervalSince(lastVoiceAt) * 1000)
        commands.insert(Command(id: id, at: Date(), text: text, speechMs: speechMs), at: 0)
        if commands.count > 8 { commands.removeLast() }
        lastVoiceAtForCommand[id] = lastVoiceAt
        emit(["type": "command", "id": id, "text": text])
        setPhase(.sent, resetAfter: 20)
        router.q.async { [weak self] in
            guard let self else { return }
            let r = self.router.route(text)
            DispatchQueue.main.async { self.applyResult(id: id, r) }
        }
    }

    @MainActor
    private func applyResult(id: Int, _ r: Router.Result) {
        if let i = commands.firstIndex(where: { $0.id == id }) {
            commands[i].target = r.who
            commands[i].mode = r.mode
            commands[i].deliverMs = r.ms
            commands[i].state = r.ok ? "entregue" : "falhou"
            if !r.ok { commands[i].reply = r.detail }
        }
        let latency = Int(Date().timeIntervalSince(lastVoiceAtForCommand[id] ?? Date()) * 1000)
        lastVoiceAtForCommand[id] = nil
        logLine("pedido #\(id) entregue a \(r.who) (\(r.mode)) \(latency) ms depois do fim da fala; envio \(r.ms) ms")
        emit(["type": "routed", "id": id, "latency_ms": latency, "pane": r.pane, "who": r.who, "mode": r.mode, "ok": r.ok, "ms": r.ms, "detail": r.detail])
        if !r.ok { logLine("entrega falhou: \(r.detail)") }
        refreshWaiting()
    }

    func refreshWaiting() {
        guard config.outputFile == nil else { return }
        router.q.async { [weak self] in
            guard let self else { return }
            self.router.refresh()
            let w = self.router.waiting()
            DispatchQueue.main.async { if self.waiting != w { self.waiting = w } }
        }
    }

    /// Resposta por clique na barra (o caminho "por texto").
    func answer(_ item: WaitItem, yes: Bool) {
        let id = nextID; nextID += 1
        commands.insert(Command(id: id, at: Date(), text: yes ? "sim (clique)" : "não (clique)"), at: 0)
        router.q.async { [weak self] in
            guard let self, let a = self.router.agent(item.pane) else { return }
            let r = self.router.deliver(to: a, text: yes ? "sim" : "não", isYes: yes, isNo: !yes)
            DispatchQueue.main.async { self.applyResult(id: id, r) }
        }
    }

    private func reportDiag() {
        diag = "áudio \(buffersIn / 2) buf/s · pico \(Int(peakDB)) dB · parciais \(partials) · finais \(finals) · rec #\(recognizer.generation)"
        if Date().timeIntervalSince(lastDiagLog) >= 60 {
            lastDiagLog = Date()
            logLine("\(diag) · conv_err \(convErrors) · frames \(framesOut) · \(formats)")
        }
        if config.audioFile == nil, phase != .paused, phase != .error,
           let s = audioStartedAt, Date().timeIntervalSince(s) > 8 {
            silentFor = buffersIn == 0 ? silentFor + 2 : 0
            if silentFor >= 6 { restart(reason: "sem áudio do microfone há \(silentFor) s") }
        }
        buffersIn = 0; peakDB = -120
    }

    private func setPhase(_ p: Phase, resetAfter: TimeInterval? = nil) {
        guard phase != .paused, phase != .error else { return }
        phase = p
        phaseReset?.cancel()
        if let t = resetAfter {
            let w = DispatchWorkItem { [weak self] in
                guard let self, self.phase == p else { return }
                self.phase = self.armedUntil != nil ? .armed : .listening
            }
            phaseReset = w
            DispatchQueue.main.asyncAfter(deadline: .now() + t, execute: w)
        }
    }

    // MARK: Status escrito pelo Claude (claude.jsonl: {"id":1,"state":"recebido"|"respondido","reply":"..."})

    private func readClaudeStatus() {
        let url = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent("claude.jsonl")
        guard let h = try? FileHandle(forReadingFrom: url) else { return }
        defer { try? h.close() }
        let size = (try? h.seekToEnd()) ?? 0
        if size < claudeOffset { claudeOffset = 0 }
        guard size > claudeOffset else { return }
        try? h.seek(toOffset: claudeOffset)
        guard let data = try? h.readToEnd(), let s = String(data: data, encoding: .utf8) else { return }
        claudeOffset = size
        for line in s.split(separator: "\n") {
            guard let d = line.data(using: .utf8),
                  let o = try? JSONSerialization.jsonObject(with: d) as? [String: Any],
                  let id = o["id"] as? Int, let state = o["state"] as? String,
                  let i = commands.firstIndex(where: { $0.id == id }) else { continue }
            commands[i].state = state
            if let r = o["reply"] as? String { commands[i].reply = r }
            if state == "recebido" { setPhase(.working, resetAfter: 120) }
            if state == "respondido" { setPhase(.listening) }
        }
    }
}
