import React from "react";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface LiveCaptionToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const LiveCaptionToggle: React.FC<LiveCaptionToggleProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("live_caption_enabled") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("live_caption_enabled", enabled)}
        isUpdating={isUpdating("live_caption_enabled")}
        label="Live Caption"
        description="Display real-time transcription captions on screen (like Google Translate)"
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);


