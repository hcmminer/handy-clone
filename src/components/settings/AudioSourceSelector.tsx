import React from "react";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";
import { invoke } from "@tauri-apps/api/core";

interface AudioSourceSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AudioSourceSelector: React.FC<AudioSourceSelectorProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const {
      getSetting,
      updateSetting,
      resetSetting,
      isUpdating,
      isLoading,
    } = useSettings();

    const [audioSource, setAudioSource] = React.useState<string>("microphone");

    React.useEffect(() => {
      const loadAudioSource = async () => {
        try {
          const source = await invoke<string>("get_audio_source");
          setAudioSource(source);
        } catch (error) {
          console.error("Failed to load audio source:", error);
        }
      };
      loadAudioSource();
    }, []);

    const handleAudioSourceSelect = async (source: string) => {
      try {
        // updateSetting will call set_audio_source backend command
        await updateSetting("audio_source", source as "microphone" | "system_audio");
        setAudioSource(source);
      } catch (error) {
        console.error("Failed to set audio source:", error);
      }
    };

    const handleReset = async () => {
      try {
        // resetSetting will call set_audio_source backend command via updateSetting
        await resetSetting("audio_source");
        setAudioSource("microphone");
      } catch (error) {
        console.error("Failed to reset audio source:", error);
      }
    };

    const audioSourceOptions = [
      { value: "microphone", label: "Microphone" },
      { value: "system_audio", label: "System Audio" },
    ];

    return (
      <SettingContainer
        title="Audio Source"
        description="Select audio input source: Microphone or System Audio (macOS only)"
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-center space-x-1">
          <Dropdown
            options={audioSourceOptions}
            selectedValue={audioSource}
            onSelect={handleAudioSourceSelect}
            placeholder="Select audio source..."
            disabled={isUpdating("audio_source") || isLoading}
          />
          <ResetButton
            onClick={handleReset}
            disabled={isUpdating("audio_source") || isLoading}
          />
        </div>
      </SettingContainer>
    );
  },
);

AudioSourceSelector.displayName = "AudioSourceSelector";

