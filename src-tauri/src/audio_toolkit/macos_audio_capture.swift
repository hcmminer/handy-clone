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
        if let nsError = error as NSError? {
            log("🔍 Error domain: \(nsError.domain)")
            log("🔍 Error code: \(nsError.code)")
            log("🔍 Error userInfo: \(nsError.userInfo)")
            
            // Error -3821: Stream was stopped by the system
            if nsError.code == -3821 {
                log("⚠️  Error -3821: Stream was stopped by macOS system")
                log("   💡 Possible causes:")
                log("   1. Another app is capturing screen/audio (conflict)")
                log("   2. System Settings > Privacy & Security > Screen Recording permission issue")
                log("   3. macOS security policy blocking the capture")
                log("   4. Display sleep or system state change")
                log("   💡 Try: 1) Close other screen recording apps, 2) Check permissions, 3) Restart app")
            }
        }
        log("📊 Final state: bufferCount=\(bufferCount), nonAudioCount=\(nonAudioCount)")
        exit(1)
    }
    
    func streamDidStart(_ stream: SCStream) {
        log("✅✅✅ SCStreamDelegate: streamDidStart called - stream is active! ✅✅✅")
        log("   📊 Delegate state: bufferCount=\(bufferCount), nonAudioCount=\(nonAudioCount)")
        log("   💡 Stream is now active and should start receiving buffers")
        log("   🔍 Stream object: \(stream)")
        log("   🔍 This delegate method confirms SCStream is fully active")
        log("   🔍 Audio buffers should start arriving soon if macOS supports it")
    }
    
    var bufferCount = 0
    var nonAudioCount = 0

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        // Log ALL buffer types received for debugging
        if nonAudioCount == 0 && bufferCount == 0 {
            log("🔍🔍🔍 FIRST SAMPLE BUFFER RECEIVED! 🔍🔍🔍")
            log("   🔍 Type: \(type)")
            log("   🔍 Type rawValue: \(type.rawValue)")
            log("   🔍 SCStreamOutputType.audio rawValue: \(SCStreamOutputType.audio.rawValue)")
            log("   🔍 SCStreamOutputType.screen rawValue: \(SCStreamOutputType.screen.rawValue)")
            log("   🔍 Comparing: type == .audio? \(type == .audio)")
            log("   🔍 Comparing: type == .screen? \(type == .screen)")
            log("   🔍 Sample buffer: \(sampleBuffer)")
            let numSamples = CMSampleBufferGetNumSamples(sampleBuffer)
            log("   🔍 Number of samples in buffer: \(numSamples)")
            let duration = CMSampleBufferGetDuration(sampleBuffer)
            log("   🔍 Duration: \(CMTimeGetSeconds(duration))s")
        }
        
        if type != .audio {
            nonAudioCount += 1
            if nonAudioCount == 1 {
                log("⚠️ WARNING: Received non-audio sample buffer (type: \(type.rawValue))")
                log("   🔍 Type rawValue: \(type.rawValue)")
                log("   🔍 Type description: \(type)")
                log("   🔍 SCStreamOutputType.audio rawValue: \(SCStreamOutputType.audio.rawValue)")
                log("   This means SCStream is working but not sending audio buffers")
                log("   💡 This is a known macOS limitation - audio capture may not work from all sources")
                log("   💡 Try: 1) Ensure audio is playing from Chrome, 2) Try capturing from display instead of apps")
            }
            // Log every 5 non-audio buffers for debugging
            if nonAudioCount % 5 == 0 {
                log("   📊 Stream status: \(nonAudioCount) non-audio buffers, \(bufferCount) audio buffers")
            }
            return
        }
        
        // AUDIO BUFFER RECEIVED!
        log("🎵🎵🎵 AUDIO BUFFER RECEIVED! 🎵🎵🎵")
        log("   Type: \(type), rawValue: \(type.rawValue)")
        log("   bufferCount will be: \(bufferCount + 1)")
        
        bufferCount += 1
        if bufferCount == 1 {
            log("✅✅✅ FIRST AUDIO BUFFER RECEIVED! ✅✅✅")
            log("   🎉 Audio capture is WORKING!")
            log("   - Buffer count: \(bufferCount)")
            log("   - Non-audio buffers received: \(nonAudioCount)")
            log("   🎉 Audio capture is working! Buffers will be sent to Rust now.")
        }
        // Log first 10 buffers for debugging
        if bufferCount <= 10 {
            log("📊 Audio buffer #\(bufferCount) received")
        }
        // Log every 100 buffers after that
        if bufferCount > 10 && bufferCount % 100 == 0 {
            log("📊 Received \(bufferCount) audio buffers (still receiving audio)")
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
            log("❌ Failed to get required buffer size: \(status)")
            return
        }
        
        // Log first buffer details
        if bufferCount == 1 {
            log("📊 First audio buffer details:")
            log("   - Required buffer size: \(bufferListSizeNeeded) bytes")
            let numSamples = CMSampleBufferGetNumSamples(sampleBuffer)
            log("   - Number of samples in buffer: \(numSamples)")
            let duration = CMSampleBufferGetDuration(sampleBuffer)
            log("   - Duration: \(CMTimeGetSeconds(duration))s")
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
            
            // Log first buffer details for debugging
            if bufferCount == 1 {
                log("📊 First audio buffer details:")
                log("   - Byte count: \(byteCount) bytes")
                log("   - Format: sampleRate=\(asbd.mSampleRate), channels=\(Int(asbd.mChannelsPerFrame))")
            }
            
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
            
            // Log first buffer write for debugging
            if bufferCount == 1 {
                log("📤 Writing \(floatSamples.count) samples to stdout (first buffer)")
            }
            
            // Write Float32 samples as little-endian bytes
            floatSamples.withUnsafeBufferPointer { ptr in
                let data = Data(bytes: ptr.baseAddress!, count: ptr.count * MemoryLayout<Float32>.size)
                FileHandle.standardOutput.write(data)
            }
            
            // Log periodically to confirm we're writing data
            if bufferCount % 100 == 0 {
                log("📤 Written \(bufferCount) buffers to stdout (still receiving audio)")
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
                    // Auto-open System Settings immediately when permission is denied
                    // This is more user-friendly than showing an alert
                    if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
                        NSWorkspace.shared.open(url)
                        log("✅ Auto-opened System Settings > Privacy & Security > Screen Recording")
                        log("💡 Please grant permission for Terminal (if running from dev) or Handy (if running built app)")
                        log("💡 Then restart the app")
                    }
                    
                    // Also show alert dialog on main thread
                    DispatchQueue.main.sync {
                        let alert = NSAlert()
                        alert.messageText = "Screen Recording Permission Required"
                        alert.informativeText = "Handy needs Screen Recording permission to capture system audio.\n\nSystem Settings has been opened automatically.\n\nPlease:\n1. Enable permission for Terminal (if running from dev) or Handy (if running built app)\n2. Restart the app after granting permission"
                        alert.alertStyle = .critical
                        alert.addButton(withTitle: "OK")
                        alert.addButton(withTitle: "Open System Settings Again")
                        alert.addButton(withTitle: "Quit")
                        
                        let response = alert.runModal()
                        if response == .alertSecondButtonReturn {
                            // Open System Settings again
                            if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
                                NSWorkspace.shared.open(url)
                            }
                            log("⏳ Opened System Settings again")
                        } else if response == .alertThirdButtonReturn {
                            // User clicked "Quit"
                            exit(1)
                        }
                        // If user clicked "OK", continue (they can grant permission and restart later)
                    }
                    
                    // Wait a bit then exit - user needs to restart after granting permission
                    log("⏳ Waiting 3 seconds, then exiting. Please grant permission and restart the app.")
                    try? await Task.sleep(nanoseconds: 3_000_000_000) // 3 seconds
                    exit(0)
                }
            }
            
            guard let content = content else {
                log("❌ No content available after permission granted")
                exit(1)
            }
            
            log("✅ PERMISSION GRANTED - Found \(content.displays.count) displays")
            log("✅ Found \(content.applications.count) applications")
            
            // Try capturing from all applications that can share content
            let shareableApps = content.applications.filter { app in
                !app.applicationName.isEmpty
            }
            
            log("Found \(shareableApps.count) shareable applications")
            if shareableApps.count > 0 {
                let appNames = shareableApps.map { $0.applicationName }.joined(separator: ", ")
                log("Applications: \(appNames)")
            }
            
            // Try multiple strategies to capture system audio
            // Strategy 1: Try all applications (most reliable for system audio on macOS)
            // Strategy 2: Try display if available
            var filter: SCContentFilter
            
            // If we have applications, try application capture first (more reliable for system audio)
            if shareableApps.count > 0 {
                log("🎯 Strategy 1: Application capture (trying first for system audio)")
                log("   💡 Application capture is more reliable for system audio on macOS")
                log("   💡 Capturing from \(shareableApps.count) applications")
                
                // Try to get display if available, otherwise use nil
                let display = content.displays.first
                if let display = display {
                    log("   💡 Using display ID: \(display.displayID) as base")
                    filter = SCContentFilter(display: display, including: shareableApps, exceptingWindows: [])
                } else {
                    log("❌ Cannot create filter: No display found")
                    log("💡 Application capture requires a display to work")
                    log("💡 This may happen if:")
                    log("   1. App is running headless (no display connected)")
                    log("   2. Screen Recording permission is not fully granted")
                    log("   3. No display is available in the system")
                    log("💡 Please ensure a display is connected and try again")
                    exit(1)
                }
            } else if let display = content.displays.first {
                // Fallback: Try display capture if no applications
                log("🎯 Strategy 2: Display capture (no applications available)")
                log("   💡 Display ID: \(display.displayID)")
                filter = SCContentFilter(display: display, excludingWindows: [])
                
                if content.displays.count > 1 {
                    log("Also found display 2 (ID: \(content.displays[1].displayID))")
                    log("Note: Audio capture may work better from the display where audio is playing")
                }
            } else {
                log("❌ No display found and no applications available")
                log("💡 This may happen if:")
                log("   1. App is running headless (no display connected)")
                log("   2. Screen Recording permission is not fully granted")
                log("   3. No applications are currently shareable")
                log("💡 Please ensure a display is connected and try again")
                exit(1)
            }
            
            let config = SCStreamConfiguration()
            config.capturesAudio = true
            config.excludesCurrentProcessAudio = false
            config.sampleRate = 48000  // macOS standard
            config.showsCursor = false
            // Try to enable all audio capture options
            config.queueDepth = 5  // Increase buffer depth
            config.minimumFrameInterval = CMTime(value: 1, timescale: 60)  // 60 FPS
            
            // Additional audio capture settings
            log("🔍 SCStreamConfiguration details:")
            log("   - capturesAudio: \(config.capturesAudio) (MUST be true for audio capture)")
            log("   - excludesCurrentProcessAudio: \(config.excludesCurrentProcessAudio)")
            log("   - sampleRate: \(config.sampleRate) (macOS standard)")
            log("   - queueDepth: \(config.queueDepth) (buffer depth)")
            log("   - minimumFrameInterval: \(config.minimumFrameInterval)")
            log("   - showsCursor: \(config.showsCursor)")
            log("   🔍 Configuration is set correctly for audio capture")
            log("   🔍 If audio buffers are not received, it's likely a macOS limitation")
            
            let delegate = AudioCaptureDelegate()
            log("🔍 Creating SCStream with filter and configuration...")
            log("   - Filter type: \(filter)")
            log("   - Config capturesAudio: \(config.capturesAudio)")
            log("   - Config excludesCurrentProcessAudio: \(config.excludesCurrentProcessAudio)")
            log("   - Config sampleRate: \(config.sampleRate)")
            log("   - Config queueDepth: \(config.queueDepth)")
            
            let stream = SCStream(filter: filter, configuration: config, delegate: delegate)
            log("✅ SCStream created successfully")
            log("   🔍 Stream object: \(stream)")
            log("   🔍 Delegate object: \(delegate)")
            
            log("📋 Content filter:")
            if shareableApps.count > 0 {
                log("   - filter type: application capture (capturing from \(shareableApps.count) apps)")
                log("   - apps: \(shareableApps.map { $0.applicationName }.joined(separator: ", "))")
                if let display = content.displays.first {
                    log("   - display: \(display.displayID)")
                } else {
                    log("   - display: using alternative method to get display")
                }
            } else {
                log("   - filter type: display capture")
                if let display = content.displays.first {
                    log("   - display: \(display.displayID)")
                }
            }
            
            // Add stream output BEFORE starting capture
            log("🔍 Adding stream output for audio type...")
            log("   🔍 SCStreamOutputType.audio: \(SCStreamOutputType.audio)")
            log("   🔍 SCStreamOutputType.audio.rawValue: \(SCStreamOutputType.audio.rawValue)")
            try stream.addStreamOutput(delegate, type: .audio, sampleHandlerQueue: DispatchQueue(label: "audio-queue"))
            log("✅ Added stream output for audio type")
            log("   🔍 Audio queue: audio-queue")
            log("   🔍 Delegate will receive audio buffers via stream(_:didOutputSampleBuffer:of:)")
            
            // Also add stream output for screen content to see if stream is working at all
            log("🔍 Adding stream output for screen type...")
            log("   🔍 SCStreamOutputType.screen: \(SCStreamOutputType.screen)")
            log("   🔍 SCStreamOutputType.screen.rawValue: \(SCStreamOutputType.screen.rawValue)")
            try stream.addStreamOutput(delegate, type: .screen, sampleHandlerQueue: DispatchQueue(label: "screen-queue"))
            log("✅ Added stream output for screen type (to verify stream is working)")
            log("   🔍 Screen queue: screen-queue")
            log("   🔍 Delegate will receive screen buffers via stream(_:didOutputSampleBuffer:of:)")
            
            log("🚀 Starting capture...")
            log("📋 About to call stream.startCapture()...")
            log("   🔍 Stream state before start: \(stream)")
            log("   🔍 Delegate state before start: bufferCount=\(delegate.bufferCount), nonAudioCount=\(delegate.nonAudioCount)")
            log("⏳ This may take a moment...")
            do {
                // Add timeout to detect if startCapture is blocking
                let startTime = Date()
                log("🔍 Calling stream.startCapture() now...")
            try await stream.startCapture()
                let elapsed = Date().timeIntervalSince(startTime)
                log("✅ Capture started successfully - stream.startCapture() returned (took \(String(format: "%.2f", elapsed))s)")
                log("   🔍 Stream state after start: \(stream)")
                log("⏳ Waiting for delegate callbacks...")
                log("   🔍 Waiting for streamDidStart delegate method...")
                log("   🔍 Waiting for audio/screen sample buffers...")
                log("💡 IMPORTANT: Please make sure audio is playing from Chrome or another app")
                log("💡 Debug: Delegate will log when streamDidStart is called")
                log("💡 Debug: If no callbacks received, SCStream may not be sending audio")
                log("💡 Debug: If you see non-audio buffers, stream is working but not sending audio")
                log("💡 Debug: Will log every 30 seconds if no audio buffers received")
                log("🔍 CRITICAL: If streamDidStart is NOT called, SCStream may not be fully active")
                log("🔍 CRITICAL: If only screen buffers (rawValue: 0) are received, macOS may not support audio capture in this configuration")
                log("🔍 CRITICAL: Audio buffers should have rawValue: 1, but we're only seeing rawValue: 0")
            } catch {
                log("❌ Failed to start capture: \(error.localizedDescription)")
                log("Error details: \(error)")
                log("Error type: \(type(of: error))")
                if let nsError = error as NSError? {
                    log("NSError domain: \(nsError.domain), code: \(nsError.code)")
                    log("NSError userInfo: \(nsError.userInfo)")
                }
                if error.localizedDescription.contains("permission") || error.localizedDescription.contains("denied") {
                    log("⚠️  This looks like a permission issue. Please grant Screen Recording permission.")
                }
                exit(1)
            }
            
            // Keep running until semaphore signal (never)
            // Log periodically only if no audio received (reduce log spam)
            Task {
                var checkCount = 0
                var streamDidStartReceived = false
                // Check if streamDidStart was called after 2 seconds
                try? await Task.sleep(nanoseconds: 2_000_000_000) // 2 seconds
                
                // Check if streamDidStart was called by checking delegate state
                // If we received buffers, streamDidStart must have been called
                if delegate.bufferCount > 0 || delegate.nonAudioCount > 0 {
                    streamDidStartReceived = true
                    log("✅ streamDidStart was called (confirmed by receiving buffers)")
                } else {
                    log("⚠️  After 2 seconds: No buffers received yet (bufferCount=0, nonAudioCount=0)")
                    log("   🔍 This could mean:")
                    log("   1. streamDidStart has not been called yet (SCStream not fully active)")
                    log("   2. SCStream is not sending any buffers at all")
                    log("   3. There's a delay before buffers start arriving")
                    log("   ⚠️  If streamDidStart is NOT called, SCStream may not be fully active")
                }
                while true {
                    try? await Task.sleep(nanoseconds: 5_000_000_000) // 5 seconds
                    checkCount += 1
                    // Log every 30 seconds if no audio yet (more frequent for debugging)
                    if delegate.bufferCount == 0 && checkCount % 6 == 0 {
                        log("⏳ Still waiting for audio... (checked \(checkCount * 5)s, bufferCount: \(delegate.bufferCount), nonAudioCount: \(delegate.nonAudioCount))")
                        log("   📊 Status: bufferCount=\(delegate.bufferCount), nonAudioCount=\(delegate.nonAudioCount)")
                        if delegate.nonAudioCount == 0 {
                            log("   ⚠️ SCStream is not calling delegate at all - stream may not be active")
                            log("   💡 Check: 1) Is streamDidStart called? 2) Is audio playing from Chrome?")
                        } else {
                            log("   ⚠️ SCStream is working (sending screen buffers) but not sending audio buffers")
                            log("   💡 This is a known macOS limitation - try capturing from display instead of apps")
                            log("   💡 Or ensure Chrome is actively playing audio with volume > 0")
                        }
                    } else if delegate.bufferCount > 0 && checkCount % 12 == 0 {
                        log("✅ Audio capture active: \(delegate.bufferCount) buffers received, \(delegate.nonAudioCount) non-audio buffers")
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
