import SwiftUI

extension Phase {
    var tint: Color {
        switch self {
        case .armed, .sent: return .accentColor
        case .working: return .purple
        case .error: return .red
        case .paused, .starting: return .secondary
        case .listening, .hearing: return .green
        }
    }
}

// MARK: Interface

struct LevelMeter: View {
    let levels: [Float]
    let tint: Color
    var body: some View {
        Canvas { ctx, size in
            let n = levels.count
            let gap: CGFloat = 2
            let w = (size.width - gap * CGFloat(n - 1)) / CGFloat(n)
            for (i, l) in levels.enumerated() {
                let h = max(3, CGFloat(l) * size.height)
                let r = CGRect(x: CGFloat(i) * (w + gap), y: (size.height - h) / 2, width: w, height: h)
                ctx.fill(Path(roundedRect: r, cornerRadius: w / 2), with: .color(tint.opacity(0.35 + Double(l) * 0.65)))
            }
        }
        .frame(height: 34)
        .animation(.linear(duration: 0.08), value: levels)
    }
}

struct StatusRow: View {
    let icon: String
    let title: String
    let value: String
    let ok: Bool?
    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: icon).frame(width: 18).foregroundStyle(.secondary)
            Text(title).foregroundStyle(.secondary)
            Spacer(minLength: 8)
            Text(value).lineLimit(1).truncationMode(.middle)
            if let ok {
                Image(systemName: ok ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                    .foregroundStyle(ok ? .green : .orange)
            }
        }
        .font(.callout)
    }
}

struct HeardRow: View {
    let h: Heard
    var highlighted: AttributedString {
        var a = AttributedString(h.text)
        if let r = h.wakeRange,
           let lo = AttributedString.Index(r.lowerBound, within: a),
           let hi = AttributedString.Index(r.upperBound, within: a) {
            a[lo..<hi].foregroundColor = .accentColor
            a[lo..<hi].font = .callout.weight(.bold)
        }
        return a
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                if h.wakeRange != nil {
                    Label("Jarvis detectado", systemImage: "checkmark.seal.fill")
                        .foregroundStyle(Color.accentColor)
                } else if h.command != nil {
                    Label("dentro da janela", systemImage: "timer")
                        .foregroundStyle(Color.accentColor)
                } else {
                    Label("ignorada, sem palavra-chave", systemImage: "minus.circle")
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text(h.at, style: .time).foregroundStyle(.tertiary)
            }
            .font(.caption)
            Text(highlighted)
                .font(.callout)
                .foregroundStyle(h.wakeRange != nil || h.command != nil ? .primary : .secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 6)
    }
}

struct CommandRow: View {
    let c: Command
    var badge: (String, String, Color) {
        switch c.state {
        case "entregue": return ("Entregue a \(c.target ?? "?")", "checkmark.circle.fill", .green)
        case "falhou": return ("Falhou", "xmark.octagon.fill", .red)
        case "recebido": return ("Claude trabalhando", "ellipsis.bubble.fill", .purple)
        case "respondido": return ("Respondido pela Echo", "speaker.wave.2.fill", .green)
        default: return ("Enviado", "paperplane.fill", .accentColor)
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label(badge.0, systemImage: badge.1).font(.caption.weight(.medium)).foregroundStyle(badge.2)
                Spacer()
                Text("#\(c.id) · \(c.at.formatted(date: .omitted, time: .shortened))").foregroundStyle(.tertiary).font(.caption)
            }
            Text("“\(c.text)”").font(.callout.weight(.medium)).fixedSize(horizontal: false, vertical: true)
            if c.deliverMs != nil || c.speechMs != nil {
                HStack(spacing: 10) {
                    if let s = c.speechMs { Label("fala→texto \(s) ms", systemImage: "waveform") }
                    if let d = c.deliverMs { Label("entrega \(d) ms", systemImage: "bolt.fill") }
                    if let m = c.mode { Text(m) }
                }
                .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            if let r = c.reply {
                Text(r).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 10).fill(.quaternary.opacity(0.5)))
    }
}

struct PanelView: View {
    @ObservedObject var m: Escuta

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            VStack(alignment: .leading, spacing: 8) {
                LevelMeter(levels: m.levels, tint: m.phase.tint)
                Group {
                    if !m.partial.isEmpty {
                        Text(m.partial).foregroundStyle(.primary)
                    } else if m.jarvisSpeaking {
                        Text("Jarvis falando: a voz dele não é transcrita.").foregroundStyle(.secondary)
                    } else if m.phase == .paused {
                        Text("Escuta pausada. O microfone está fechado.").foregroundStyle(.secondary)
                    } else {
                        Text("Diga “Jarvis” e o pedido.").foregroundStyle(.secondary)
                    }
                }
                .font(.callout)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(minHeight: 34, alignment: .topLeading)
            }

            VStack(spacing: 8) {
                StatusRow(icon: "mic", title: "Microfone", value: m.silentFor > 0 ? "\(m.micName) · sem áudio há \(m.silentFor) s" : m.micName, ok: m.phase != .error && m.silentFor == 0)
                if let n = m.airpodsName {
                    StatusRow(icon: "airpodspro", title: n,
                              value: m.airpodsRate >= 44100 ? "\(Int(m.airpodsRate / 1000)) kHz · estéreo" : "\(Int(m.airpodsRate / 1000)) kHz · modo chamada",
                              ok: m.airpodsRate >= 44100)
                } else {
                    StatusRow(icon: "airpodspro", title: "AirPods", value: "não conectados", ok: nil)
                }
                StatusRow(icon: "text.bubble", title: "Reconhecimento", value: m.recognizerState, ok: m.recognizerState == "ok")
            }

            if let e = m.errorText {
                Label(e, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red).font(.callout)
            }

            if !m.waiting.isEmpty {
                section("Esperando você") {
                    ForEach(m.waiting) { w in
                        HStack(alignment: .top, spacing: 10) {
                            Image(systemName: w.blocked ? "hand.raised.fill" : "questionmark.bubble.fill")
                                .foregroundStyle(.orange)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(w.who).font(.callout.weight(.semibold))
                                Text(w.reason).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                            }
                            Spacer()
                            Button("Sim") { m.answer(w, yes: true) }.buttonStyle(.borderedProminent).controlSize(.small)
                            Button("Não") { m.answer(w, yes: false) }.controlSize(.small)
                        }
                    }
                    Text("Ou diga “Jarvis, sim” ou “Jarvis, grokbot não”.").font(.caption).foregroundStyle(.tertiary)
                }
            }

            section("Pedidos") {
                if m.commands.isEmpty {
                    Text("Nenhum ainda.").foregroundStyle(.secondary).font(.callout)
                } else {
                    ForEach(m.commands.prefix(3)) { CommandRow(c: $0) }
                }
            }

            section("Frases ouvidas") {
                if m.heard.isEmpty {
                    Text("Nada ainda. Fale perto do Mac.").foregroundStyle(.secondary).font(.callout)
                } else {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(m.heard) { h in
                                HeardRow(h: h)
                                if h.id != m.heard.last?.id { Divider() }
                            }
                        }
                    }
                    .frame(maxHeight: 190)
                }
            }

            Text(m.diag).font(.caption2.monospacedDigit()).foregroundStyle(.tertiary)
            Divider()
            HStack {
                Button(m.isPaused ? "Retomar escuta" : "Pausar escuta") { m.togglePause() }
                Button { m.restart(reason: "botão Reiniciar") } label: { Label("Reiniciar", systemImage: "arrow.clockwise") }
                Spacer()
                Button("Sair") { m.quit() }.keyboardShortcut("q")
            }
            .controlSize(.regular)
        }
        .padding(16)
        .frame(width: 360)
    }

    var header: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle().fill(m.phase.tint.opacity(0.18)).frame(width: 34, height: 34)
                Image(systemName: m.phase.symbol)
                    .foregroundStyle(m.phase.tint)
                    .symbolEffect(.variableColor.iterative, isActive: m.phase == .listening || m.phase == .hearing || m.phase == .working)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text("Escuta").font(.headline)
                Text(m.phase.label).font(.subheadline).foregroundStyle(m.phase.tint)
            }
            Spacer()
            if let until = m.armedUntil {
                TimelineView(.periodic(from: .now, by: 0.25)) { _ in
                    let left = max(0, until.timeIntervalSinceNow)
                    Gauge(value: left, in: 0...m.config.armedWindow) { EmptyView() } currentValueLabel: {
                        Text("\(Int(left.rounded(.up)))").font(.caption.monospacedDigit())
                    }
                    .gaugeStyle(.accessoryCircularCapacity)
                    .tint(.accentColor)
                    .scaleEffect(0.7)
                    .frame(width: 34, height: 34)
                }
            } else if let w = m.lastWakeAt, Date().timeIntervalSince(w) < 4 {
                Image(systemName: "checkmark.seal.fill").foregroundStyle(Color.accentColor).font(.title3)
            }
        }
    }

    func section<C: View>(_ title: String, @ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title.uppercased()).font(.caption2.weight(.semibold)).tracking(0.6).foregroundStyle(.secondary)
            content()
        }
    }
}

