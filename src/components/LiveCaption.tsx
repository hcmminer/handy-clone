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

    setIsListening(true); // Show "Đang nghe..." when enabled
    let timeoutId: NodeJS.Timeout;

    const unlisten = listen<string>("live-caption-update", (event) => {
      const newCaption = event.payload.trim();
      if (newCaption && newCaption.length > 1) {
        setCaption(newCaption);
        setIsVisible(true);
        setIsListening(false);
        
        // Auto-hide after 5 seconds of no new caption
        clearTimeout(timeoutId);
        timeoutId = setTimeout(() => {
          setIsVisible(false);
          setIsListening(true); // Show "Đang nghe..." again
        }, 5000);
      }
    });

    return () => {
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

