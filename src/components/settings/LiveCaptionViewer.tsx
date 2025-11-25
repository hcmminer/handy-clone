import React, { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";
import { toast } from "sonner";

export const LiveCaptionViewer: React.FC = () => {
  const { settings } = useSettings();
  const [caption, setCaption] = useState<string>("");
  const [logs, setLogs] = useState<Array<{ time: string; message: string; type: 'info' | 'warn' | 'error' | 'debug' }>>([]);
  const logEndRef = useRef<HTMLDivElement>(null);
  const logContainerRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState<boolean>(false); // Disabled by default to prevent UI lag
  const [isAtBottom, setIsAtBottom] = useState<boolean>(true);
  const maxLogs = 100;

  // Throttle log updates to prevent UI lag when too many logs come in
  const logUpdateTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const pendingLogsRef = useRef<Array<{ time: string; message: string; type: 'info' | 'warn' | 'error' | 'debug' }>>([]);
  
  const addLog = React.useCallback((type: 'info' | 'warn' | 'error' | 'debug', message: string) => {
    const time = new Date().toLocaleTimeString();
    pendingLogsRef.current.push({ time, message, type });
    
    // Throttle: only update logs max once per 500ms to prevent UI lag
    if (logUpdateTimeoutRef.current) {
      return; // Already scheduled
    }
    
    logUpdateTimeoutRef.current = setTimeout(() => {
      setLogs((prev) => {
        const newLogs = [...prev, ...pendingLogsRef.current];
        pendingLogsRef.current = [];
        return newLogs.slice(-maxLogs);
      });
      logUpdateTimeoutRef.current = null;
    }, 500);
  }, []);

  useEffect(() => {
    if (!settings?.live_caption_enabled) {
      setCaption("");
      return;
    }

    // Store cleanup functions
    let cleanupCaption: (() => void) | null = null;
    let cleanupLog: (() => void) | null = null;

    const unlistenCaption = listen<string>("live-caption-update", (event) => {
      const newCaption = event.payload.trim();
      console.log(`🎯 [LiveCaptionViewer] Event received! Payload length: ${event.payload.length}, trimmed: ${newCaption.length}, content: "${newCaption.substring(0, 50)}${newCaption.length > 50 ? '...' : ''}"`);
      addLog('info', `🎯 [LiveCaptionViewer] Event received (${event.payload.length} chars raw, ${newCaption.length} chars trimmed)`);
      
      if (newCaption && newCaption.length > 1) {
        console.log(`✅ [LiveCaptionViewer] Setting caption: "${newCaption}"`);
        setCaption(newCaption);
        addLog('info', `✅ Caption set: "${newCaption}"`);
      } else {
        console.warn(`⚠️ [LiveCaptionViewer] Caption too short or empty: length=${newCaption.length}`);
        addLog('warn', `⚠️ Caption too short (length: ${newCaption.length})`);
      }
    });

    unlistenCaption.then((fn) => {
      cleanupCaption = fn;
    }).catch((err) => {
      console.error("❌ [LiveCaptionViewer] Failed to register caption listener:", err);
    });

    // Throttle log listener to prevent UI lag when too many logs come in
    let lastLogTime = 0;
    const LOG_THROTTLE_MS = 500; // Only process logs max once per 500ms (increased to reduce lag)
    
    const unlistenLog = listen<string>("log-update", (event) => {
      const now = Date.now();
      if (now - lastLogTime < LOG_THROTTLE_MS) {
        return; // Skip if too frequent
      }
      lastLogTime = now;
      
      const logMessage = event.payload.trim();
      if (logMessage) {
        // Determine log type from message
        let logType: 'info' | 'warn' | 'error' | 'debug' = 'info';
        if (logMessage.includes('❌') || logMessage.includes('error') || logMessage.includes('Error') || logMessage.includes('Failed')) {
          logType = 'error';
        } else if (logMessage.includes('⚠️') || logMessage.includes('warn') || logMessage.includes('Warn')) {
          logType = 'warn';
        } else if (logMessage.includes('debug') || logMessage.includes('Debug')) {
          logType = 'debug';
        }
        addLog(logType, logMessage);

        // Show popup for permission status (only for important messages)
        if (logMessage.includes('PERMISSION DENIED') || logMessage.includes('❌ PERMISSION DENIED')) {
          toast.error("❌ Screen Recording Permission bị từ chối!", {
            description: "Vui lòng cấp quyền Screen Recording trong System Settings > Privacy & Security > Screen Recording",
            duration: 10000,
          });
        } else if (logMessage.includes('PERMISSION GRANTED') || logMessage.includes('✅ PERMISSION GRANTED')) {
          toast.success("✅ Screen Recording Permission đã được cấp!", {
            description: "App có thể capture system audio rồi",
            duration: 5000,
          });
        } else if (logMessage.includes('First audio buffer received') || logMessage.includes('✅ First audio buffer')) {
          toast.success("🎉 Đã nhận được audio buffers!", {
            description: "System audio capture đang hoạt động",
            duration: 5000,
          });
        }
      }
    });

    unlistenLog.then((fn) => {
      cleanupLog = fn;
    }).catch((err) => {
      console.error("❌ [LiveCaptionViewer] Failed to register log listener:", err);
    });

    return () => {
      if (cleanupCaption && typeof cleanupCaption === 'function') {
        try {
          cleanupCaption();
        } catch (err) {
          console.warn("⚠️ [LiveCaptionViewer] Error cleaning up caption listener:", err);
        }
      }
      if (cleanupLog && typeof cleanupLog === 'function') {
        try {
          cleanupLog();
        } catch (err) {
          console.warn("⚠️ [LiveCaptionViewer] Error cleaning up log listener:", err);
        }
      }
      // Clear pending log updates
      if (logUpdateTimeoutRef.current) {
        clearTimeout(logUpdateTimeoutRef.current);
        logUpdateTimeoutRef.current = null;
      }
      pendingLogsRef.current = [];
    };
  }, [settings?.live_caption_enabled, addLog]);

  // Check if user is at bottom of log container
  const checkIfAtBottom = () => {
    if (!logContainerRef.current) return false;
    const { scrollTop, scrollHeight, clientHeight } = logContainerRef.current;
    // Consider "at bottom" if within 50px of bottom
    return scrollHeight - scrollTop - clientHeight < 50;
  };

  // Handle scroll event to track if user is at bottom
  const handleScroll = () => {
    setIsAtBottom(checkIfAtBottom());
  };

  // Auto-scroll only if user explicitly enables it and is at bottom
  // Disabled by default to prevent UI lag and unwanted scrolling
  const scrollTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  
  useEffect(() => {
    // Only auto-scroll if user explicitly enabled it
    if (!autoScroll) {
      return;
    }
    
    if (isAtBottom && logEndRef.current) {
      // Clear any pending scroll
      if (scrollTimeoutRef.current) {
        clearTimeout(scrollTimeoutRef.current);
      }
      
      // Throttle scroll to max once per 200ms to prevent UI lag
      scrollTimeoutRef.current = setTimeout(() => {
        if (logEndRef.current && isAtBottom && autoScroll) {
          // Use instant scroll instead of smooth to reduce lag
          logEndRef.current.scrollIntoView({ behavior: 'auto' });
        }
      }, 200);
    }
    
    return () => {
      if (scrollTimeoutRef.current) {
        clearTimeout(scrollTimeoutRef.current);
      }
    };
  }, [logs.length, autoScroll, isAtBottom]); // Only depend on logs.length, not logs array itself

  // Listen to console logs from frontend
  useEffect(() => {
    const originalLog = console.log;
    const originalWarn = console.warn;
    const originalError = console.error;

    console.log = (...args) => {
      originalLog(...args);
      const message = args.map(arg => typeof arg === 'string' ? arg : JSON.stringify(arg)).join(' ');
      if (message.includes('[LiveCaption]') || message.includes('🎯')) {
        addLog('info', message);
      }
    };

    console.warn = (...args) => {
      originalWarn(...args);
      const message = args.map(arg => typeof arg === 'string' ? arg : JSON.stringify(arg)).join(' ');
      if (message.includes('[LiveCaption]') || message.includes('⚠️')) {
        addLog('warn', message);
      }
    };

    console.error = (...args) => {
      originalError(...args);
      const message = args.map(arg => typeof arg === 'string' ? arg : JSON.stringify(arg)).join(' ');
      if (message.includes('[LiveCaption]') || message.includes('❌')) {
        addLog('error', message);
      }
    };

    return () => {
      console.log = originalLog;
      console.warn = originalWarn;
      console.error = originalError;
    };
  }, [addLog]);

  const clearLogs = () => {
    setLogs([]);
  };

  const copyLogs = async () => {
    const logText = logs.map(log => `[${log.time}] ${log.message}`).join('\n');
    try {
      await navigator.clipboard.writeText(logText);
      toast.success("Logs đã được copy vào clipboard!");
    } catch (err) {
      toast.error("Không thể copy logs: " + err);
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title="Live Caption Preview">
        <div className="space-y-4">
          <div className="bg-background-dark rounded-lg p-4 border border-mid-gray/20">
            <div className="text-sm text-text/70 mb-2">Current Caption:</div>
            {caption ? (
              <div className="text-lg font-medium text-text break-words">
                {caption}
              </div>
                  ) : (
                    <div className="text-sm text-text/50 italic">
                      {settings?.live_caption_enabled !== false
                        ? "Đang nghe... (Waiting for audio transcription)"
                        : "Live Caption is disabled - Please enable it in Display settings"}
                    </div>
                  )}
          </div>
        </div>
      </SettingsGroup>

      <SettingsGroup title="Real-Time Logs">
        <div className="space-y-4">
          <div className="flex justify-between items-center gap-2">
            <div className="text-sm text-text/70">
              Showing last {logs.length} log entries
            </div>
            <div className="flex gap-2">
              <button
                onClick={() => setAutoScroll(!autoScroll)}
                className={`px-3 py-1 text-sm rounded-lg transition-colors ${
                  autoScroll 
                    ? 'bg-green-500/20 hover:bg-green-500/30 text-green-400' 
                    : 'bg-mid-gray/20 hover:bg-mid-gray/30'
                }`}
                title={autoScroll ? "Auto-scroll đang bật - Click để tắt" : "Auto-scroll đang tắt - Click để bật"}
              >
                {autoScroll ? "⏸️ Pause" : "▶️ Resume"}
              </button>
              <button
                onClick={() => {
                  logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
                  setIsAtBottom(true);
                }}
                className="px-3 py-1 text-sm bg-mid-gray/20 hover:bg-mid-gray/30 rounded-lg transition-colors"
                title="Scroll xuống cuối"
              >
                ⬇️ Scroll to Bottom
              </button>
              <button
                onClick={copyLogs}
                className="px-3 py-1 text-sm bg-mid-gray/20 hover:bg-mid-gray/30 rounded-lg transition-colors"
                disabled={logs.length === 0}
              >
                Copy Logs
              </button>
              <button
                onClick={clearLogs}
                className="px-3 py-1 text-sm bg-mid-gray/20 hover:bg-mid-gray/30 rounded-lg transition-colors"
                disabled={logs.length === 0}
              >
                Clear Logs
              </button>
            </div>
          </div>
          <div 
            ref={logContainerRef}
            onScroll={handleScroll}
            className="bg-background-dark rounded-lg p-4 border border-mid-gray/20 max-h-96 overflow-y-auto font-mono text-xs"
          >
            {logs.length === 0 ? (
              <div className="text-text/50 italic">No logs yet...</div>
            ) : (
              logs.map((log, index) => (
                <div
                  key={index}
                  className={`mb-1 ${
                    log.type === 'error' ? 'text-red-400' :
                    log.type === 'warn' ? 'text-yellow-400' :
                    log.type === 'debug' ? 'text-text/50' :
                    'text-text/80'
                  }`}
                >
                  <span className="text-text/50">[{log.time}]</span>{' '}
                  {log.message}
                </div>
              ))
            )}
            <div ref={logEndRef} />
          </div>
        </div>
      </SettingsGroup>
    </div>
  );
};

