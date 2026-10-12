import Foundation

/// Interromper o Jarvis enquanto ele fala, e o que não vira pedido.
public enum BargeIn {
    /// "Para", "chega"... ditos sozinhos enquanto o Jarvis fala só calam o Jarvis.
    public static let stopWords: Set<String> = ["para", "pare", "espera", "espere", "chega", "silencio"]

    public static func words(_ s: String) -> [String] {
        s.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil).lowercased()
            .components(separatedBy: CharacterSet.alphanumerics.inverted).filter { !$0.isEmpty }
    }

    /// Ruído sem palavras: vazio, só pontuação ou só interjeições.
    public static func isNoise(_ text: String) -> Bool {
        let fillers: Set<String> = ["hum", "hm", "hmm", "ah", "ahn", "eh", "uh", "oh", "you", "a", "e", "o"]
        return words(text).allSatisfy { fillers.contains($0) || $0.count < 2 }
    }

    /// Só palavras de parada ("para", "espera aí", "chega"): interrupção, não pedido.
    public static func isOnlyStop(_ text: String) -> Bool {
        let w = words(text).filter { !["ai", "ja", "por", "favor", "jarvis", "camarada"].contains($0) }
        return !w.isEmpty && w.allSatisfy { stopWords.contains($0) }
    }
}
