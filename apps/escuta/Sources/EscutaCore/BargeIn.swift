import Foundation

/// Interromper o Jarvis enquanto ele fala, sem confundir a voz dele (eco) com a do dono.
public enum BargeIn {
    /// Além de "Jarvis"/"camarada", estas palavras no começo de uma frase mandam o Jarvis calar.
    public static let stopWords: Set<String> = ["para", "pare", "espera", "espere", "chega", "silencio"]

    public static func words(_ s: String) -> [String] {
        s.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil).lowercased()
            .components(separatedBy: CharacterSet.alphanumerics.inverted).filter { !$0.isEmpty }
    }

    /// Só palavras de parada ("para", "espera aí", "chega"): interrupção, não pedido.
    public static func isOnlyStop(_ text: String) -> Bool {
        let w = words(text).filter { !["ai", "ja", "por", "favor", "jarvis", "camarada"].contains($0) }
        return !w.isEmpty && w.allSatisfy { stopWords.contains($0) }
    }

    /// O trecho ouvido é a própria fala do Jarvis voltando pelo microfone: quase todas as palavras
    /// dele estão no texto que está sendo falado.
    public static func isEcho(_ heard: String, spoken: String?) -> Bool {
        guard let spoken, !spoken.isEmpty else { return false }
        let h = words(heard)
        guard h.count >= 2 else { return false }
        let s = Set(words(spoken))
        let hits = h.filter { s.contains($0) }.count
        return Double(hits) / Double(h.count) >= 0.7
    }

    /// A palavra que justifica interromper agora, ou nil. `text` é o trecho mais recente ouvido.
    public static func trigger(_ text: String, spoken: String?) -> String? {
        let spokenWords = Set(words(spoken ?? ""))
        if let r = Wake.find(text) {
            let w = words(String(text[r])).first ?? "jarvis"
            // Se o próprio Jarvis está dizendo a palavra, ouvir a palavra não prova nada.
            if !Wake.variants.contains(where: { spokenWords.contains($0) }) { return w }
        }
        if let first = words(text).first, stopWords.contains(first), !spokenWords.contains(first) {
            return first
        }
        return nil
    }
}

extension Wake {
    /// Pedido depois da última palavra de ativação em qualquer posição (fala do dono colada no eco).
    public static func lastCommand(in text: String) -> (wake: Range<String.Index>, rest: String)? {
        var cursor = text.startIndex
        var last: Range<String.Index>?
        while cursor < text.endIndex, let r = find(String(text[cursor...])) {
            let sub = String(text[cursor...])
            let lo = text.index(cursor, offsetBy: sub.distance(from: sub.startIndex, to: r.lowerBound))
            let hi = text.index(cursor, offsetBy: sub.distance(from: sub.startIndex, to: r.upperBound))
            last = lo..<hi
            cursor = hi
        }
        guard let w = last else { return nil }
        return (w, String(text[w.upperBound...]).trimmingCharacters(in: edgeStrip))
    }
}
