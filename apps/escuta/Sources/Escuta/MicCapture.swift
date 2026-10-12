import AVFoundation
import CoreAudio
import CoreMedia

/// Captura só de entrada (AVCaptureSession), num microfone escolhido pelo nome.
/// Não abre unidade de saída, não usa voice processing, não faz ducking e não troca o dispositivo
/// padrão do sistema: a saída de áudio do Mac (AirPods, alto-falante) fica intocada.
final class MicCapture: NSObject, AVCaptureAudioDataOutputSampleBufferDelegate {
    private let session = AVCaptureSession()
    private let output = AVCaptureAudioDataOutput()
    private let queue = DispatchQueue(label: "escuta.mic", qos: .userInteractive)
    private(set) var device: AVCaptureDevice?
    var onBuffer: ((AVAudioPCMBuffer) -> Void)?

    static func inputs() -> [AVCaptureDevice] {
        AVCaptureDevice.DiscoverySession(deviceTypes: [.microphone, .external], mediaType: .audio, position: .unspecified).devices
    }

    static func isBluetooth(_ d: AVCaptureDevice) -> Bool {
        let t = UInt32(bitPattern: d.transportType)
        return t == kAudioDeviceTransportTypeBluetooth || t == kAudioDeviceTransportTypeBluetoothLE
    }

    /// O microfone com o nome pedido; senão o embutido. Nunca um Bluetooth (abrir o mic dos AirPods
    /// derruba eles para o modo chamada).
    static func pick(hint: String) -> AVCaptureDevice? {
        let all = inputs().filter { !isBluetooth($0) }
        return all.first { $0.localizedName.localizedCaseInsensitiveContains(hint) }
            ?? all.first { UInt32(bitPattern: $0.transportType) == kAudioDeviceTransportTypeBuiltIn }
            ?? all.first
    }

    func start(_ d: AVCaptureDevice) throws {
        device = d
        let input = try AVCaptureDeviceInput(device: d)
        session.beginConfiguration()
        guard session.canAddInput(input) else { session.commitConfiguration(); throw err("não deu para usar \(d.localizedName)") }
        session.addInput(input)
        output.audioSettings = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVLinearPCMBitDepthKey: 32,
            AVLinearPCMIsFloatKey: true,
            AVLinearPCMIsNonInterleaved: true,
            AVNumberOfChannelsKey: 1,
        ]
        output.setSampleBufferDelegate(self, queue: queue)
        guard session.canAddOutput(output) else { session.commitConfiguration(); throw err("saída de captura indisponível") }
        session.addOutput(output)
        session.commitConfiguration()
        session.startRunning()
    }

    func stop() { session.stopRunning() }

    func captureOutput(_ o: AVCaptureOutput, didOutput sb: CMSampleBuffer, from c: AVCaptureConnection) {
        guard let desc = sb.formatDescription, var asbd = desc.audioStreamBasicDescription,
              let fmt = AVAudioFormat(streamDescription: &asbd) else { return }
        let frames = AVAudioFrameCount(CMSampleBufferGetNumSamples(sb))
        guard frames > 0, let buf = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: frames) else { return }
        buf.frameLength = frames
        let st = CMSampleBufferCopyPCMDataIntoAudioBufferList(sb, at: 0, frameCount: Int32(frames), into: buf.mutableAudioBufferList)
        guard st == noErr else { return }
        onBuffer?(buf)
    }

    private func err(_ m: String) -> NSError { NSError(domain: "escuta", code: 2, userInfo: [NSLocalizedDescriptionKey: m]) }
}
