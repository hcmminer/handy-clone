import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";

interface SetupDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onRetry: () => void;
  title: string;
  message: string;
  instructions: string[];
  isRetrying: boolean;
}

function SetupDialog({ isOpen, onClose, onRetry, title, message, instructions, isRetrying }: SetupDialogProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50">
      <div className="bg-[#2a2a2a] rounded-lg p-6 max-w-2xl mx-4 shadow-xl border border-gray-700">
        <div className="flex justify-between items-start mb-4">
          <h2 className="text-xl font-semibold text-white">{title}</h2>
        </div>
        
        <div className="text-gray-300 mb-4">
          <p className="mb-4">{message}</p>
          
          <div className="space-y-3">
            {instructions.map((instruction, index) => (
              <div key={index} className="flex gap-3">
                <span className="text-blue-400 font-semibold min-w-[24px]">
                  {index + 1}.
                </span>
                <span>{instruction}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="flex gap-3 justify-end mt-6">
          <button
            onClick={() => {
              openUrl("https://existential.audio/blackhole/");
            }}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors"
          >
            Download BlackHole
          </button>
          <button
            onClick={onRetry}
            disabled={isRetrying}
            className="px-4 py-2 bg-green-600 hover:bg-green-700 disabled:bg-gray-600 disabled:cursor-not-allowed text-white rounded transition-colors"
          >
            {isRetrying ? "Checking..." : "I've Completed Setup - Retry"}
          </button>
        </div>
        
        <p className="text-sm text-gray-500 mt-4 text-center">
          This dialog will close automatically once system audio is properly configured
        </p>
      </div>
    </div>
  );
}

export default function SystemAudioSetup() {
  const [setupDialog, setSetupDialog] = useState<{
    isOpen: boolean;
    title: string;
    message: string;
    instructions: string[];
  }>({
    isOpen: false,
    title: "",
    message: "",
    instructions: [],
  });
  
  const [isRetrying, setIsRetrying] = useState(false);

  const handleRetry = async () => {
    setIsRetrying(true);
    try {
      // Try to restart the audio stream
      await invoke("restart_audio_stream");
      toast.success("System audio configured successfully!");
      setSetupDialog(prev => ({ ...prev, isOpen: false }));
    } catch (error) {
      console.error("Retry failed:", error);
      toast.error("Setup not complete yet. Please follow all steps and try again.");
    } finally {
      setIsRetrying(false);
    }
  };

  useEffect(() => {
    console.log("🔧 [SystemAudioSetup] Component mounted, setting up listeners...");
    
    // Check audio initialization status on mount
    const checkInitStatus = async () => {
      try {
        console.log("📞 [SystemAudioSetup] Calling check_audio_initialization_status...");
        const status = await invoke<string>("check_audio_initialization_status");
        console.log("📊 [SystemAudioSetup] Audio initialization status:", status);
        // Event will be emitted automatically if setup is required
      } catch (error) {
        console.error("❌ [SystemAudioSetup] Failed to check audio initialization status:", error);
      }
    };

    checkInitStatus();

    // Listen for system audio setup required events
    console.log("👂 [SystemAudioSetup] Setting up system-audio-setup-required listener...");
    const unsubscribePromise = listen<string>("system-audio-setup-required", (event) => {
      console.log("✅ [SystemAudioSetup] Received system-audio-setup-required event:", event.payload);
      alert("Event received! Opening setup dialog..."); // Debug
      
      setSetupDialog({
        isOpen: true,
        title: "🎵 System Audio Setup Required",
        message: "To capture system audio (e.g., from Chrome, Zoom), you need to install BlackHole and configure a Multi-Output Device:",
        instructions: [
          "Install BlackHole: brew install blackhole-2ch (or download from existential.audio/blackhole/)",
          "Open Audio MIDI Setup (Applications > Utilities > Audio MIDI Setup)",
          "Click '+' button at bottom-left, select 'Create Multi-Output Device'",
          "In the Multi-Output Device, check both 'BlackHole 2ch' and your current speakers (e.g., 'Built-in Output')",
          "Set Master Device to your speakers (click the gear icon) to keep volume control working",
          "Go to System Settings > Sound > Output and select the Multi-Output Device you just created",
          "Click 'I've Completed Setup - Retry' button below"
        ],
      });
    });

    // Listen for ScreenCaptureKit permission required
    console.log("👂 [SystemAudioSetup] Setting up screencapture-permission-required listener...");
    const unsubscribeScreenCapturePromise = listen<string>("screencapture-permission-required", (event) => {
      console.log("ScreenCaptureKit permission required:", event.payload);
      
      setSetupDialog({
        isOpen: true,
        title: "🔒 Screen Recording Permission Required",
        message: "To capture system audio on macOS 13+, this app needs Screen Recording permission:",
        instructions: [
          "Open System Settings > Privacy & Security > Screen Recording",
          "Find 'Handy' (or 'Terminal' if running in dev mode) in the list",
          "Toggle the switch to grant permission",
          "Click 'I've Completed Setup - Retry' button below",
        ],
      });
    });

    console.log("✅ [SystemAudioSetup] All listeners setup complete");

    return () => {
      console.log("🧹 [SystemAudioSetup] Cleaning up listeners...");
      unsubscribePromise.then(fn => fn());
      unsubscribeScreenCapturePromise.then(fn => fn());
    };
  }, []);

  return (
    <SetupDialog
      {...setupDialog}
      onClose={() => setSetupDialog(prev => ({ ...prev, isOpen: false }))}
      onRetry={handleRetry}
      isRetrying={isRetrying}
    />
  );
}
