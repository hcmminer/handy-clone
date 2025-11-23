import Foundation
import ScreenCaptureKit
import CoreMedia
import AVFoundation
import CoreAudio

// Simple stderr logger
func log(_ message: String) {
    FileHandle.standardError.write(Data("\(message)\n".utf8))
}

class AudioCaptureDelegate: NSObject, SCStreamDelegate, SCStreamOutput {
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        log("Stream stopped with error: \(error.localizedDescription)")
        exit(1)
    }
    
    var bufferCount = 0

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        if type != .audio {
            if bufferCount == 0 {
                log("WARNING: Received non-audio sample buffer (type: \(type.rawValue))")
            }
            return
        }
        
        bufferCount += 1
        if bufferCount == 1 {
            log("✅ First audio buffer received!")
        }
        // Reduce log frequency - only log every 500 buffers instead of 100
        if bufferCount % 500 == 0 {
            log("Received \(bufferCount) audio buffers")
        }
        
        // Get audio format description
        guard let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer) else {
            log("Failed to get format description")
            return
        }
        
        let audioStreamBasicDescription = CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription)
        guard let asbd = audioStreamBasicDescription?.pointee else {
            log("Failed to get ASBD")
            return
        }
        
        // Log format info (first time only)
        if bufferCount == 1 {
            log("Audio format: sampleRate=\(asbd.mSampleRate), channels=\(Int(asbd.mChannelsPerFrame)), formatID=\(asbd.mFormatID), bitsPerChannel=\(asbd.mBitsPerChannel)")
        }
        
        // First, query the required buffer size
        var bufferListSizeNeeded: size_t = 0
        var status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: &bufferListSizeNeeded,
            bufferListOut: nil,
            bufferListSize: 0,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: 0,
            blockBufferOut: nil
        )
        
        guard status == noErr && bufferListSizeNeeded > 0 else {
            log("Failed to get required buffer size: \(status)")
            return
        }
        
        // Allocate buffer with correct size
        let audioBufferListPtr = UnsafeMutablePointer<AudioBufferList>.allocate(capacity: 1)
        audioBufferListPtr.initialize(to: AudioBufferList())
        defer {
            audioBufferListPtr.deinitialize(count: 1)
            audioBufferListPtr.deallocate()
        }
        
        var blockBuffer: CMBlockBuffer?
        
        status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: audioBufferListPtr,
            bufferListSize: bufferListSizeNeeded,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: 0,
            blockBufferOut: &blockBuffer
        )
        
        guard status == noErr else {
            log("Failed to get audio buffer list: \(status)")
            return
        }
        
        let audioBufferList = audioBufferListPtr.pointee
        
        // Convert audio to Float32 and write to stdout
        withUnsafePointer(to: &audioBufferListPtr.pointee.mBuffers) { buffersPtr in
            let buffers = UnsafeBufferPointer(start: buffersPtr, count: Int(audioBufferList.mNumberBuffers))
            
            for buffer in buffers {
            
            guard let data = buffer.mData else { continue }
            let byteCount = Int(buffer.mDataByteSize)
            
            // Convert based on format
            let floatSamples: [Float32]
            
            if asbd.mFormatID == kAudioFormatLinearPCM {
                if asbd.mFormatFlags & kAudioFormatFlagIsFloat != 0 {
                    // Already Float32
                    let sampleCount = byteCount / MemoryLayout<Float32>.size
                    let samples = data.assumingMemoryBound(to: Float32.self)
                    floatSamples = Array(UnsafeBufferPointer(start: samples, count: sampleCount))
                } else if asbd.mFormatFlags & kAudioFormatFlagIsSignedInteger != 0 {
                    // Int16 or Int32
                    if asbd.mBitsPerChannel == 16 {
                        let sampleCount = byteCount / MemoryLayout<Int16>.size
                        let samples = data.assumingMemoryBound(to: Int16.self)
                        let intSamples = Array(UnsafeBufferPointer(start: samples, count: sampleCount))
                        // Convert Int16 to Float32 (-32768 to 32767 -> -1.0 to 1.0)
                        floatSamples = intSamples.map { Float32($0) / 32768.0 }
                    } else if asbd.mBitsPerChannel == 32 {
                        let sampleCount = byteCount / MemoryLayout<Int32>.size
                        let samples = data.assumingMemoryBound(to: Int32.self)
                        let intSamples = Array(UnsafeBufferPointer(start: samples, count: sampleCount))
                        // Convert Int32 to Float32
                        floatSamples = intSamples.map { Float32($0) / 2147483648.0 }
                    } else {
                        log("Unsupported bit depth: \(asbd.mBitsPerChannel)")
                        continue
                    }
                } else {
                    log("Unsupported format flags: \(asbd.mFormatFlags)")
                    continue
                }
            } else {
                log("Unsupported format ID: \(asbd.mFormatID)")
                continue
            }
            
                // Write Float32 samples as little-endian bytes
                floatSamples.withUnsafeBufferPointer { ptr in
                    let data = Data(bytes: ptr.baseAddress!, count: ptr.count * MemoryLayout<Float32>.size)
                    FileHandle.standardOutput.write(data)
                }
            }
        }
    }
}

@available(macOS 13.0, *)
func runCapture() {
    let semaphore = DispatchSemaphore(value: 0)
    
    Task {
        do {
            // Check Screen Recording permission first
            // Note: SCShareableContent will trigger permission dialog if not granted
            log("Checking Screen Recording permission...")
            log("Note: If permission dialog appears, please click 'Allow'")
            
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            
            log("✅ Permission check passed - Found \(content.displays.count) displays")
            log("Found \(content.applications.count) applications")
            
            // Try to capture from all applications instead of display
            // SCK may not capture audio from display, but can capture from applications
            guard let display = content.displays.first else {
                log("❌ No display found")
                exit(1)
            }
            
            // Try capturing from all applications that can share content
            let shareableApps = content.applications.filter { app in
                !app.applicationName.isEmpty
            }
            
            log("Found \(shareableApps.count) shareable applications")
            if shareableApps.count > 0 {
                let appNames = shareableApps.map { $0.applicationName }.joined(separator: ", ")
                log("Applications: \(appNames)")
            }
            
            // Try both displays if available (user has 2 monitors)
            log("Trying to capture from display 1 (ID: \(display.displayID))")
            if content.displays.count > 1 {
                log("Also found display 2 (ID: \(content.displays[1].displayID))")
                log("Note: Audio capture may work better from the display where audio is playing")
            }
            
            // Use display but include all applications (this should capture audio from all apps)
            let filter = SCContentFilter(display: display, including: shareableApps, exceptingWindows: [])
            
            let config = SCStreamConfiguration()
            config.capturesAudio = true
            config.excludesCurrentProcessAudio = false
            config.sampleRate = 48000  // macOS standard
            config.showsCursor = false
            
            let delegate = AudioCaptureDelegate()
            let stream = SCStream(filter: filter, configuration: config, delegate: delegate)
            
            try stream.addStreamOutput(delegate, type: .audio, sampleHandlerQueue: DispatchQueue(label: "audio-queue"))
            
            log("Starting capture...")
            do {
                try await stream.startCapture()
                log("✅ Capture started successfully")
                log("⏳ Waiting for audio buffers...")
                log("💡 If no audio appears after 10 seconds, please check:")
                log("   1. System Settings > Privacy & Security > Screen Recording")
                log("   2. Ensure this app (or Terminal) has Screen Recording permission enabled")
                log("   3. Restart the app after granting permission")
            } catch {
                log("❌ Failed to start capture: \(error.localizedDescription)")
                log("Error details: \(error)")
                if error.localizedDescription.contains("permission") || error.localizedDescription.contains("denied") {
                    log("⚠️  This looks like a permission issue. Please grant Screen Recording permission.")
                }
                exit(1)
            }
            
            // Keep running until semaphore signal (never)
            // Log periodically only if no audio received (reduce log spam)
            Task {
                var checkCount = 0
                while true {
                    try? await Task.sleep(nanoseconds: 5_000_000_000) // 5 seconds
                    checkCount += 1
                    // Only log every 30 seconds if no audio yet, or every 60 seconds if audio is coming
                    if delegate.bufferCount == 0 && checkCount % 6 == 0 {
                        log("Still waiting for audio... (checked \(checkCount * 5)s)")
                    } else if delegate.bufferCount > 0 && checkCount % 12 == 0 {
                        log("Audio capture active: \(delegate.bufferCount) buffers received")
                    }
                }
            }
            
        } catch {
            log("❌ Error getting shareable content: \(error.localizedDescription)")
            if error.localizedDescription.contains("permission") || error.localizedDescription.contains("denied") {
                log("⚠️  Permission denied! Please grant Screen Recording permission in System Settings.")
                log("   System Settings > Privacy & Security > Screen Recording")
            }
            exit(1)
        }
    }
    
    semaphore.wait()
}

if #available(macOS 13.0, *) {
    runCapture()
} else {
    log("macOS 13.0 or later required")
    exit(1)
}
