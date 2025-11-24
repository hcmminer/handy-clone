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
        log("❌ Stream stopped with error: \(error.localizedDescription)")
        log("Error details: \(error)")
        exit(1)
    }
    
    func streamDidStart(_ stream: SCStream) {
        log("✅ SCStreamDelegate: streamDidStart called - stream is active!")
    }
    
    var bufferCount = 0
    var nonAudioCount = 0

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        if type != .audio {
            nonAudioCount += 1
            if nonAudioCount == 1 {
                log("⚠️ WARNING: Received non-audio sample buffer (type: \(type.rawValue))")
                log("   This means SCStream is working but not sending audio buffers")
            }
            return
        }
        
        bufferCount += 1
        if bufferCount == 1 {
            log("✅ First audio buffer received!")
            log("   - Buffer count: \(bufferCount)")
            log("   - Non-audio buffers received: \(nonAudioCount)")
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
            log("🔍 Checking Screen Recording permission...")
            log("Note: If permission dialog appears, please click 'Allow'")
            
            var content: SCShareableContent?
            var permissionGranted = false
            
            // Keep showing permission dialog until granted
            while !permissionGranted {
                do {
                    content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
                    permissionGranted = true
                    log("✅ PERMISSION GRANTED!")
                } catch {
                    log("❌ PERMISSION DENIED: \(error.localizedDescription)")
                    // Show alert dialog on main thread - keep showing until granted
                    await MainActor.run {
                        let alert = NSAlert()
                        alert.messageText = "Screen Recording Permission Required"
                        alert.informativeText = "Handy needs Screen Recording permission to capture system audio.\n\nPlease:\n1. Click 'Open System Settings'\n2. Enable permission for this app (Terminal or Handy)\n3. Click 'Granted' after enabling permission\n\nThis dialog will keep appearing until permission is granted."
                        alert.alertStyle = .critical
                        alert.addButton(withTitle: "Open System Settings")
                        alert.addButton(withTitle: "Granted")
                        alert.addButton(withTitle: "Quit")
                        
                        let response = alert.runModal()
                        if response == .alertFirstButtonReturn {
                            // Open System Settings to Screen Recording
                            if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
                                NSWorkspace.shared.open(url)
                            }
                            // Wait a bit for user to grant permission
                            log("⏳ Waiting 3 seconds for user to grant permission...")
                            Thread.sleep(forTimeInterval: 3.0)
                        } else if response == .alertSecondButtonReturn {
                            // User clicked "Granted" - check permission again
                            log("✅ User clicked 'Granted', checking permission...")
                        } else {
                            // User clicked "Quit"
                            exit(1)
                        }
                    }
                }
            }
            
            guard let content = content else {
                log("❌ No content available after permission granted")
                exit(1)
            }
            
            log("✅ PERMISSION GRANTED - Found \(content.displays.count) displays")
            log("✅ Found \(content.applications.count) applications")
            
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
            
            // Try multiple strategies to capture system audio
            // Strategy 1: Try all applications (most reliable for system audio on macOS)
            // Strategy 2: Try Chrome specifically
            // Strategy 3: Fallback to display
            var filter: SCContentFilter
            
            // Strategy 1: Try capturing from all applications (most reliable for system audio)
            // Display capture often doesn't send audio buffers, but application capture does
            if shareableApps.count > 0 {
                log("🎯 Strategy 1: Capturing from ALL \(shareableApps.count) applications (most reliable for system audio)")
                filter = SCContentFilter(display: display, including: shareableApps, exceptingWindows: [])
            } else {
                // Strategy 2: Fallback to display if no applications
                log("🎯 Strategy 2: Fallback - Capturing from display directly")
                filter = SCContentFilter(display: display, excludingWindows: [])
            }
            
            let config = SCStreamConfiguration()
            config.capturesAudio = true
            config.excludesCurrentProcessAudio = false
            config.sampleRate = 48000  // macOS standard
            config.showsCursor = false
            // Try to enable all audio capture options
            config.queueDepth = 5  // Increase buffer depth
            config.minimumFrameInterval = CMTime(value: 1, timescale: 60)  // 60 FPS
            
            let delegate = AudioCaptureDelegate()
            let stream = SCStream(filter: filter, configuration: config, delegate: delegate)
            
            log("📋 Stream configuration:")
            log("   - capturesAudio: \(config.capturesAudio)")
            log("   - excludesCurrentProcessAudio: \(config.excludesCurrentProcessAudio)")
            log("   - sampleRate: \(config.sampleRate)")
            log("   - queueDepth: \(config.queueDepth)")
            
            log("📋 Content filter:")
            log("   - display: \(display.displayID)")
            log("   - filter type: display capture")
            
            try stream.addStreamOutput(delegate, type: .audio, sampleHandlerQueue: DispatchQueue(label: "audio-queue"))
            log("✅ Added stream output for audio type")
            
            log("Starting capture...")
            do {
                try await stream.startCapture()
                log("✅ Capture started successfully")
                log("⏳ Waiting for audio buffers...")
                log("💡 Debug: Delegate will log when streamDidStart is called")
                log("💡 Debug: If no callbacks received, SCStream may not be sending audio")
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
                        log("Still waiting for audio... (checked \(checkCount * 5)s, bufferCount: \(delegate.bufferCount), nonAudioCount: \(delegate.nonAudioCount))")
                        log("   - If bufferCount=0 and nonAudioCount=0, SCStream is not calling delegate at all")
                        log("   - If nonAudioCount>0, SCStream is working but not sending audio buffers")
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
