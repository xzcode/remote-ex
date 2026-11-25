// Audio-only, bounded ScreenCaptureKit helper. The trusted native owner
// starts it only after verifying the peer and local audio grant. No network,
// files, microphone or screen frames are emitted. stdout is S16LE mono PCM.
// Parent EOF or the independent deadline ends capture even if the owner dies.
import Foundation
import ScreenCaptureKit
import CoreMedia
import CoreGraphics
import AVFoundation
import AudioToolbox
import Darwin

@available(macOS 13.0, *)
final class Output: NSObject, SCStreamOutput, SCStreamDelegate {
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        Darwin.exit(2)
    }
    func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, CMSampleBufferIsValid(sample), CMSampleBufferDataIsReady(sample) else { return }
        guard let desc = CMSampleBufferGetFormatDescription(sample),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(desc),
              asbd.pointee.mFormatID == kAudioFormatLinearPCM,
              asbd.pointee.mChannelsPerFrame == 1,
              asbd.pointee.mSampleRate == 16000,
              let format = AVAudioFormat(streamDescription: asbd) else { Darwin.exit(3) }
        let count = CMSampleBufferGetNumSamples(sample)
        guard count > 0, count <= 16384,
              let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count)) else { Darwin.exit(3) }
        buffer.frameLength = AVAudioFrameCount(count)
        guard CMSampleBufferCopyPCMDataIntoAudioBufferList(sample, at: 0, frameCount: Int32(count), into: buffer.mutableAudioBufferList) == noErr else { Darwin.exit(3) }
        var pcm = [Int16](repeating: 0, count: count)
        if let floats = buffer.floatChannelData {
            for i in 0..<count {
                let v = floats[0][i]
                pcm[i] = v.isFinite ? Int16((max(-1, min(1, v)) * 32767).rounded()).littleEndian : 0
            }
        } else if let ints = buffer.int16ChannelData {
            for i in 0..<count { pcm[i] = ints[0][i].littleEndian }
        } else { Darwin.exit(3) }
        do {
            try FileHandle.standardOutput.write(contentsOf: pcm.withUnsafeBytes { Data($0) })
        } catch { Darwin.exit(4) }
    }
}
@main struct RemoteEXSystemAudio {
    static func main() async {
        guard #available(macOS 13.0, *), isatty(STDIN_FILENO) == 0, isatty(STDOUT_FILENO) == 0 else { Darwin.exit(1) }
        // Fixed short initial line; no arbitrary helper command.
        // Bound lifetime from launch, including permission or framework stalls.
        let launch = DispatchTime.now()
        DispatchQueue.global().asyncAfter(deadline: launch + 305) { Darwin.exit(0) }
        var line = [UInt8]()
        for _ in 0..<32 {
            var byte: UInt8 = 0
            if Darwin.read(STDIN_FILENO, &byte, 1) != 1 { Darwin.exit(1) }
            if byte == 10 { break }
            line.append(byte)
        }
        let extended = line == Array("CAPTURE SYSTEM AUDIO EXTENDED".utf8)
        guard extended || line == Array("CAPTURE SYSTEM AUDIO".utf8) else { Darwin.exit(1) }
        let duration: UInt64 = extended ? 300 : 30
        DispatchQueue.global().asyncAfter(deadline: launch + Int(duration + 5)) { Darwin.exit(0) }
        DispatchQueue.global().async {
            var b: UInt8 = 0
            _ = Darwin.read(STDIN_FILENO, &b, 1)
            Darwin.exit(0)
        }
        guard CGPreflightScreenCaptureAccess() else { Darwin.exit(5) }
        do {
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            guard let display = content.displays.first(where: { $0.displayID == CGMainDisplayID() }) else { Darwin.exit(6) }
            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            config.capturesAudio = true
            config.sampleRate = 16000
            config.channelCount = 1
            config.excludesCurrentProcessAudio = true
            if #available(macOS 15.0, *) { config.captureMicrophone = false }
            config.width = 64
            config.height = 64
            config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
            config.queueDepth = 3
            let output = Output()
            let stream = SCStream(filter: filter, configuration: config, delegate: output)
            try stream.addStreamOutput(output, type: .audio, sampleHandlerQueue: DispatchQueue(label: "remote-ex.system-audio"))
            try await stream.startCapture()
            try await Task.sleep(nanoseconds: duration * 1_000_000_000)
            try await stream.stopCapture()
        } catch { Darwin.exit(7) }
    }
}
