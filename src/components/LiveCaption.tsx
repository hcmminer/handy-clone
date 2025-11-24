import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./LiveCaption.css";

interface LiveCaptionProps {
  enabled?: boolean;
}

export default function LiveCaption({ enabled = true }: LiveCaptionProps) {
  const [caption, setCaption] = useState<string>("");
  const [isVisible, setIsVisible] = useState(false);
  const [isListening, setIsListening] = useState(false);

  useEffect(() => {
    if (!enabled) {
      setIsVisible(false);
      setIsListening(false);
      return;
    }

    console.log("🎯 [LiveCaption] Enabled, waiting for events...");
    setIsListening(true); // Show "Đang nghe..." when enabled
    let timeoutId: NodeJS.Timeout;
    let eventCount = 0;

    const unlisten = listen<string>("live-caption-update", (event) => {
      eventCount++;
      const newCaption = event.payload.trim();
      console.log(`🎯 [LiveCaption] Event #${eventCount} received, payload length: ${newCaption.length}, preview: "${newCaption.substring(0, 50)}${newCaption.length > 50 ? '...' : ''}"`);
      
      if (newCaption && newCaption.length > 1) {
        console.log(`✅ [LiveCaption] Setting caption (${newCaption.length} chars): "${newCaption}"`);
        setCaption(newCaption);
        setIsVisible(true);
        setIsListening(false);
        
        // Auto-hide after 5 seconds of no new caption
        clearTimeout(timeoutId);
        timeoutId = setTimeout(() => {
          console.log("⏸️ [LiveCaption] Hiding caption after 5s timeout");
          setIsVisible(false);
          setIsListening(true); // Show "Đang nghe..." again
        }, 5000);
      } else {
        console.warn(`⚠️ [LiveCaption] Received empty or too short caption (length: ${newCaption.length})`);
      }
    });

    // Log when listener is set up
    unlisten.then(() => {
      console.log("✅ [LiveCaption] Event listener registered successfully");
    }).catch((err) => {
      console.error("❌ [LiveCaption] Failed to register event listener:", err);
    });

    return () => {
      console.log(`🛑 [LiveCaption] Cleaning up (received ${eventCount} events total)`);
      unlisten.then((fn) => fn());
      clearTimeout(timeoutId);
    };
  }, [enabled]);

  if (!enabled) {
    return null;
  }

  // Show "Đang nghe..." when listening but no caption yet
  if (!isVisible && isListening) {
    return (
      <div className="live-caption-container">
        <div className="live-caption-content">
          <span className="live-caption-text listening">Đang nghe...</span>
        </div>
      </div>
    );
  }

  if (!isVisible || !caption) {
    return null;
  }

  return (
    <div className="live-caption-container">
      <div className="live-caption-content">
        <span className="live-caption-text">{caption}</span>
      </div>
    </div>
  );
}

