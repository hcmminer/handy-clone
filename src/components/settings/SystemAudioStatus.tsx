import React, { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { SettingsGroup } from "../ui/SettingsGroup";

type Status = "unknown" | "granted" | "denied" | "waiting" | "active" | "error";

interface StatusItem {
  label: string;
  status: Status;
  message: string;
  icon: string;
}

export const SystemAudioStatus: React.FC = () => {
  const [permissionStatus, setPermissionStatus] = useState<Status>("unknown");
  const [captureStatus, setCaptureStatus] = useState<Status>("unknown");
  const [audioDetectionStatus, setAudioDetectionStatus] = useState<Status>("unknown");
  const [appReadyStatus, setAppReadyStatus] = useState<Status>("unknown");
  const [lastUpdate, setLastUpdate] = useState<string>("");

  // Query initial status when component mounts
  useEffect(() => {
    const queryInitialStatus = async () => {
      try {
        console.log("🔍 [SystemAudioStatus] Querying initial status...");
        const status = await invoke<{
          permission: string;
          capture: string;
          audio_detection: string;
        }>("get_system_audio_status");
        
        console.log("📊 [SystemAudioStatus] Initial status:", JSON.stringify(status, null, 2));
        console.log("📊 [SystemAudioStatus] Permission:", status.permission);
        console.log("📊 [SystemAudioStatus] Capture:", status.capture);
        console.log("📊 [SystemAudioStatus] Audio Detection:", status.audio_detection);
        
        if (status.permission === "granted") {
          setPermissionStatus("granted");
        }
        if (status.capture === "active") {
          setCaptureStatus("active");
        } else if (status.capture === "waiting") {
          setCaptureStatus("waiting");
        }
        if (status.audio_detection === "active") {
          setAudioDetectionStatus("active");
        } else if (status.audio_detection === "waiting") {
          setAudioDetectionStatus("waiting");
        }
      } catch (err) {
        console.error("❌ [SystemAudioStatus] Failed to query initial status:", err);
      }
    };
    
    queryInitialStatus();
  }, []);

  // Listen to log events and update statuses
  useEffect(() => {
    console.log("🎯 [SystemAudioStatus] Setting up log listener...");
    
    const unlistenLog = listen<string>("log-update", (event) => {
      const logMessage = event.payload.trim();
      const now = new Date().toLocaleTimeString();
      setLastUpdate(now);

      console.log("📥 [SystemAudioStatus] Received log:", logMessage.substring(0, 100));

      // Permission status
      if (logMessage.includes("PERMISSION GRANTED") || logMessage.includes("✅ PERMISSION GRANTED") || logMessage.includes("PERMISSION GRANTED - Found")) {
        console.log("✅ [SystemAudioStatus] Permission granted detected!");
        setPermissionStatus("granted");
      } else if (logMessage.includes("PERMISSION DENIED") || logMessage.includes("❌ PERMISSION DENIED") || logMessage.includes("declined TCCs")) {
        console.log("❌ [SystemAudioStatus] Permission denied detected!");
        setPermissionStatus("denied");
      }

      // Capture status
      if (logMessage.includes("Capture started successfully") || logMessage.includes("✅ Capture started")) {
        console.log("✅ [SystemAudioStatus] Capture started detected!");
        setCaptureStatus("active");
      } else if (logMessage.includes("Failed to start capture") || (logMessage.includes("❌") && logMessage.includes("capture"))) {
        console.log("❌ [SystemAudioStatus] Capture failed detected!");
        setCaptureStatus("error");
      } else if (logMessage.includes("Starting capture") || logMessage.includes("Starting ScreenCaptureKit")) {
        console.log("⏳ [SystemAudioStatus] Starting capture detected!");
        setCaptureStatus("waiting");
      }

      // Audio detection status
      if (logMessage.includes("First audio buffer received") || logMessage.includes("✅ First audio buffer")) {
        console.log("✅ [SystemAudioStatus] First audio buffer detected!");
        setAudioDetectionStatus("active");
      } else if (logMessage.includes("System capture read") && logMessage.includes("samples")) {
        console.log("✅ [SystemAudioStatus] System capture read samples detected!");
        setAudioDetectionStatus("active");
      } else if (logMessage.includes("Still waiting for audio") || logMessage.includes("⏳ Waiting for audio") || logMessage.includes("Waiting for audio buffers")) {
        if (permissionStatus === "granted" && captureStatus === "active") {
          console.log("⏳ [SystemAudioStatus] Still waiting for audio detected!");
          setAudioDetectionStatus("waiting");
        }
      } else if (logMessage.includes("No audio samples available") || logMessage.includes("buffer is empty")) {
        // Only set to waiting if we have permission and capture is active
        if (permissionStatus === "granted" && captureStatus === "active") {
          setAudioDetectionStatus((prev) => {
            if (prev !== "active") return "waiting";
            return prev;
          });
        }
      }
    });

    // Store cleanup function
    let cleanupFn: (() => void) | null = null;
    
    // Log when listener is set up
    unlistenLog.then((fn) => {
      cleanupFn = fn;
      console.log("✅ [SystemAudioStatus] Log listener registered successfully");
    }).catch((err) => {
      console.error("❌ [SystemAudioStatus] Failed to register log listener:", err);
    });

    return () => {
      if (cleanupFn && typeof cleanupFn === 'function') {
        try {
          cleanupFn();
        } catch (err) {
          console.warn("⚠️ [SystemAudioStatus] Error cleaning up log listener:", err);
        }
      }
    };
  }, [permissionStatus, captureStatus]);

  // Update app ready status based on other statuses
  useEffect(() => {
    if (permissionStatus === "granted" && captureStatus === "active" && audioDetectionStatus === "active") {
      setAppReadyStatus("active");
    } else if (permissionStatus === "denied") {
      setAppReadyStatus("error");
    } else if (permissionStatus === "granted" && captureStatus === "active") {
      // Permission granted and capture active, but waiting for audio
      setAppReadyStatus("waiting");
    } else if (permissionStatus === "granted") {
      // Permission granted but capture not started yet
      setAppReadyStatus("waiting");
    } else if (permissionStatus === "unknown") {
      // Still checking
      setAppReadyStatus("waiting");
    } else {
      setAppReadyStatus("unknown");
    }
  }, [permissionStatus, captureStatus, audioDetectionStatus]);

  // Initial check - set app as waiting if we don't have info
  useEffect(() => {
    const timeout = setTimeout(() => {
      if (permissionStatus === "unknown" && captureStatus === "unknown") {
        setAppReadyStatus("waiting");
      }
    }, 2000);

    return () => clearTimeout(timeout);
  }, [permissionStatus, captureStatus]);

  const getStatusConfig = (status: Status): { color: string; bgColor: string; icon: string } => {
    switch (status) {
      case "granted":
      case "active":
        return {
          color: "text-green-400",
          bgColor: "bg-green-500/20 border-green-500/50",
          icon: "✅",
        };
      case "denied":
      case "error":
        return {
          color: "text-red-400",
          bgColor: "bg-red-500/20 border-red-500/50",
          icon: "❌",
        };
      case "waiting":
        return {
          color: "text-yellow-400",
          bgColor: "bg-yellow-500/20 border-yellow-500/50",
          icon: "⏳",
        };
      default:
        return {
          color: "text-gray-400",
          bgColor: "bg-gray-500/20 border-gray-500/50",
          icon: "❓",
        };
    }
  };

  const statusItems: StatusItem[] = [
    {
      label: "Screen Recording Permission",
      status: permissionStatus,
      message:
        permissionStatus === "granted"
          ? "✅ Đã cấp quyền Screen Recording - App có thể capture system audio"
          : permissionStatus === "denied"
          ? "❌ Chưa cấp quyền - Vui lòng vào System Settings > Privacy & Security > Screen Recording và bật quyền cho Terminal hoặc Handy"
          : "⏳ Đang kiểm tra quyền...",
      icon: "🔐",
    },
    {
      label: "Audio Capture",
      status: captureStatus,
      message:
        captureStatus === "active"
          ? "✅ Đã khởi động capture thành công - Đang chờ audio buffers"
          : captureStatus === "error"
          ? "❌ Lỗi khi khởi động capture - Kiểm tra permission và restart app"
          : captureStatus === "waiting"
          ? "⏳ Đang khởi động capture..."
          : "❓ Chưa khởi động capture - Đang chờ permission",
      icon: "🎙️",
    },
    {
      label: "System Audio Detection",
      status: audioDetectionStatus,
      message:
        audioDetectionStatus === "active"
          ? "✅ Đã phát hiện system audio - App đang nhận audio buffers từ Chrome/hệ thống"
          : audioDetectionStatus === "waiting"
          ? "⏳ Đang chờ system audio... - Hãy phát audio từ Chrome hoặc ứng dụng khác"
          : "❓ Chưa phát hiện system audio - Cần permission và audio đang phát",
      icon: "🔊",
    },
    {
      label: "App Status",
      status: appReadyStatus,
      message:
            appReadyStatus === "active"
              ? "✅ App đã sẵn sàng và hoạt động - Live caption sẽ hiển thị khi có audio"
              : appReadyStatus === "error"
              ? "❌ App chưa sẵn sàng - Cần cấp Screen Recording permission trong System Settings"
              : appReadyStatus === "waiting"
              ? permissionStatus === "granted" && captureStatus === "active"
                ? "⏳ App đang chờ system audio... - Hãy phát audio từ Chrome hoặc ứng dụng khác"
                : "⏳ App đang khởi động... - Đang chờ permission và audio capture"
              : "❓ Đang kiểm tra trạng thái...",
      icon: "🚀",
    },
  ];

  return (
    <SettingsGroup title="System Audio Status">
      <div className="space-y-3">
        {statusItems.map((item, index) => {
          const config = getStatusConfig(item.status);
          return (
            <div
              key={index}
              className={`p-4 rounded-lg border ${config.bgColor} transition-all`}
            >
              <div className="flex items-start gap-3">
                <div className="text-2xl">{item.icon}</div>
                <div className="flex-1">
                  <div className="flex items-center justify-between mb-1">
                    <h3 className="font-medium text-text">{item.label}</h3>
                    <div className={`flex items-center gap-2 ${config.color}`}>
                      <span className="text-lg">{config.icon}</span>
                      <span className="text-sm font-medium capitalize">
                        {item.status === "unknown" ? "Đang kiểm tra" : item.status}
                      </span>
                    </div>
                  </div>
                  <p className="text-sm text-text/70">{item.message}</p>
                </div>
              </div>
            </div>
          );
        })}
        {lastUpdate && (
          <div className="text-xs text-text/50 text-right mt-2">
            Cập nhật lần cuối: {lastUpdate}
          </div>
        )}
      </div>
    </SettingsGroup>
  );
};

