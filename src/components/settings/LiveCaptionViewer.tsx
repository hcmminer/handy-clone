import React, { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";

export const LiveCaptionViewer: React.FC = () => {
  const { settings } = useSettings();
  const [caption, setCaption] = useState<string>("");
  const [logs, setLogs] = useState<Array<{ time: string; message: string; type: 'info' | 'warn' | 'error' | 'debug' }>>([]);
  const logEndRef = useRef<HTMLDivElement>(null);
  const maxLogs = 100;

  useEffect(() => {
    if (!settings?.live_caption_enabled) {
      setCaption("");
      return;
    }

    const unlistenCaption = listen<string>("live-caption-update", (event) => {
      const newCaption = event.payload.trim();
      if (newCaption && newCaption.length > 1) {
        setCaption(newCaption);
        addLog('info', `🎯 Caption received: "${newCaption}"`);
      }
    });

    const unlistenLog = listen<string>("log-update", (event) => {
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
      }
    });

    return () => {
      unlistenCaption.then((fn) => fn());
      unlistenLog.then((fn) => fn());
    };
  }, [settings?.live_caption_enabled]);

  const addLog = (type: 'info' | 'warn' | 'error' | 'debug', message: string) => {
    const time = new Date().toLocaleTimeString();
    setLogs((prev) => {
      const newLogs = [...prev, { time, message, type }];
      return newLogs.slice(-maxLogs);
    });
  };

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

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
  }, []);

  const clearLogs = () => {
    setLogs([]);
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
                {settings?.live_caption_enabled 
                  ? "Đang nghe... (Waiting for audio transcription)" 
                  : "Live Caption is disabled"}
              </div>
            )}
          </div>
        </div>
      </SettingsGroup>

      <SettingsGroup title="Real-Time Logs">
        <div className="space-y-4">
          <div className="flex justify-between items-center">
            <div className="text-sm text-text/70">
              Showing last {logs.length} log entries
            </div>
            <button
              onClick={clearLogs}
              className="px-3 py-1 text-sm bg-mid-gray/20 hover:bg-mid-gray/30 rounded-lg transition-colors"
            >
              Clear Logs
            </button>
          </div>
          <div className="bg-background-dark rounded-lg p-4 border border-mid-gray/20 max-h-96 overflow-y-auto font-mono text-xs">
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

