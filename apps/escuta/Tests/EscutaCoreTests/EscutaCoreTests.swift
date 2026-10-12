import Foundation
import Testing
@testable import EscutaCore

@Suite struct WakeTests {
    @Test func comandoDepoisDoJarvis() {
        let c = Wake.command(in: "Jarvis, abre a agenda de amanhã.")
        #expect(c?.rest == "abre a agenda de amanhã")
    }

    @Test func variacaoDaTranscricao() {
        #expect(Wake.command(in: "Jarbas que horas são?")?.rest == "que horas são")
        #expect(Wake.command(in: "Camarada, liga a luz")?.rest == "liga a luz")
    }

    @Test func noMeioDaFraseNaoConta() {
        #expect(Wake.command(in: "eu falei com o Jarvis ontem") == nil)
    }

    @Test func depoisDeUmPontoConta() {
        #expect(Wake.command(in: "Pronto, já está feito. Jarvis, obrigado")?.rest == "obrigado")
    }

    @Test func prefixoParecidoComJarvisSai() {
        #expect(Wake.stripPrefix("Jardes, liga a luz da sala.") == "liga a luz da sala")
        #expect(Wake.stripPrefix("Jarvas que horas são?") == "que horas são")
        #expect(Wake.stripPrefix("na verdade muda pra amanhã") == "na verdade muda pra amanhã")
    }

    @Test func ruidoNaoViraPedido() {
        #expect(BargeIn.isNoise("."))
        #expect(BargeIn.isNoise("Hum."))
        #expect(BargeIn.isNoise("L"))
        #expect(!BargeIn.isNoise("ok"))
        #expect(!BargeIn.isNoise("sim"))
    }

    @Test func jarvisSozinho() {
        #expect(Wake.command(in: "Jarvis.")?.rest == "")
    }
}

@Suite struct EndpointTests {
    let c = EndpointConfig(silence: 1.0, openSilence: 2.5, settle: 0.4, finalizeWait: 1.5, maxUtterance: 120)

    func d(_ t: String, volatile: Bool = false, silence: Double, sinceText: Double? = nil, fin: Double? = 0.3,
           elapsed: Double = 5) -> EndpointDecision {
        Endpoint.decide(text: t, volatile: volatile, silence: silence, sinceText: sinceText ?? silence,
                        sinceFinalize: fin, elapsed: elapsed, config: c)
    }

    @Test func frasesAbertas() {
        #expect(Endpoint.endsOpen("Jarvis, quero ver a agenda e"))
        #expect(Endpoint.endsOpen("Jarvis, manda uma mensagem pra"))
        #expect(Endpoint.endsOpen("Jarvis, olha só,"))
        #expect(Endpoint.endsOpen("Jarvis, abre o calendário porque."))
        #expect(Endpoint.endsOpen("Jarvis, então..."))
        #expect(Endpoint.endsOpen("Jarvis"))
    }

    @Test func frasesFechadas() {
        #expect(!Endpoint.endsOpen("Jarvis, que horas são?"))
        #expect(!Endpoint.endsOpen("Jarvis, abre a agenda de amanhã."))
        #expect(!Endpoint.endsOpen("Jarvis, o que é"))
        #expect(!Endpoint.endsOpen(""))
    }

    @Test func falandoNaoFecha() {
        #expect(d("Jarvis, abre a agenda", silence: 0.3) == .wait)
    }

    @Test func pausaPedeOsFinaisAntes() {
        #expect(d("Jarvis, abre a agenda", volatile: true, silence: 1.1, fin: nil) == .finalize)
    }

    @Test func fraseCompletaFechaRapido() {
        #expect(d("Jarvis, abre a agenda de amanhã.", silence: 1.1) == .close("silêncio"))
    }

    @Test func textoAindaMudandoEspera() {
        #expect(d("Jarvis, abre a agenda", silence: 1.5, sinceText: 0.2) == .wait)
    }

    @Test func conectivoEsperaMais() {
        #expect(d("Jarvis, abre a agenda e", silence: 1.5) == .wait)
        #expect(d("Jarvis, abre a agenda,", silence: 2.0) == .wait)
        #expect(d("Jarvis, abre a agenda e", silence: 2.6) == .close("silêncio após frase aberta"))
    }

    @Test func esperaOFinalDoTrechoEmRevisao() {
        #expect(d("Jarvis, abre a agenda", volatile: true, silence: 1.2, fin: 0.2) == .wait)
        #expect(d("Jarvis, abre a agenda", volatile: true, silence: 2.8, fin: 1.6) == .close("silêncio"))
    }

    @Test func limiteDeDuracao() {
        #expect(d("Jarvis, e", volatile: true, silence: 0, elapsed: 121) == .close("limite de 120 s"))
    }
}

@Suite struct BargeInTests {
    @Test func palavraSoltaSemVozNaoValeNada() {
        // O "Não." que o reconhecedor inventou com o microfone em silêncio.
        #expect(BargeIn.isWeak("Não.", confidence: 0.9, hadVoice: false))
        #expect(BargeIn.isWeak("Não", confidence: 0.3, hadVoice: true))
        #expect(!BargeIn.isWeak("Não", confidence: 0.72, hadVoice: true))
        #expect(!BargeIn.isWeak("liga a luz da sala", confidence: 0.2, hadVoice: false))
    }

    @Test func soPararNaoViraPedido() {
        #expect(BargeIn.isOnlyStop("para"))
        #expect(BargeIn.isOnlyStop("espera aí"))
        #expect(!BargeIn.isOnlyStop("para de tocar música na sala"))
    }
}

@Suite struct SpeechLockTests {
    @Test func arquivoAusenteEstaLivre() {
        #expect(!SpeechLock.isHeld("/tmp/nao-existe-\(UUID().uuidString).lock"))
    }

    @Test func lockOcupadoPorOutroProcesso() throws {
        let path = NSTemporaryDirectory() + "escuta-teste-\(UUID().uuidString).lock"
        FileManager.default.createFile(atPath: path, contents: nil)
        defer { try? FileManager.default.removeItem(atPath: path) }
        #expect(!SpeechLock.isHeld(path))
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/lockf")
        p.arguments = ["-k", path, "/bin/sleep", "2"]
        try p.run()
        Thread.sleep(forTimeInterval: 0.5)
        #expect(SpeechLock.isHeld(path))
        p.waitUntilExit()
        #expect(!SpeechLock.isHeld(path))
    }
}
