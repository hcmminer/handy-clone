import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Toaster } from "sonner";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import Footer from "./components/footer";
import Onboarding from "./components/onboarding";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { useSettings } from "./hooks/useSettings";
import LiveCaption from "./components/LiveCaption";
import SystemAudioSetup from "./components/SystemAudioSetup";

const renderSettingsContent = (section: SidebarSection) => {
  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  return <ActiveComponent />;
};

function App() {
  const [showOnboarding, setShowOnboarding] = useState<boolean | null>(null);
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>("general");
  const { settings, updateSetting } = useSettings();

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  const checkOnboardingStatus = async () => {
    // Retry with delay to ensure backend is ready
    const checkWithRetry = async (retries = 5, delay = 500) => {
      for (let i = 0; i < retries; i++) {
        try {
          // Always check if they have any models available
          const modelsAvailable: boolean = await invoke("has_any_models_available");
          console.log("Models available:", modelsAvailable);
          setShowOnboarding(!modelsAvailable);
          return;
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error);
          console.error(`Failed to check onboarding status (attempt ${i + 1}/${retries}):`, error);
          
          // If it's a Tauri not ready error, wait longer and retry
          if (errorMessage.includes("Cannot read properties of undefined") || 
              errorMessage.includes("invoke")) {
            if (i < retries - 1) {
              await new Promise((resolve) => setTimeout(resolve, delay * 2)); // Wait longer
            } else {
              // Final attempt after longer delay
              await new Promise((resolve) => setTimeout(resolve, 2000));
              try {
                const modelsAvailable: boolean = await invoke("has_any_models_available");
                setShowOnboarding(!modelsAvailable);
                return;
              } catch (finalErr) {
                console.warn("Final attempt failed, showing onboarding as fallback");
                setShowOnboarding(true);
              }
            }
          } else if (i < retries - 1) {
            await new Promise((resolve) => setTimeout(resolve, delay));
          } else {
            // If all retries fail, show onboarding as fallback
            console.warn("All retries failed, showing onboarding as fallback");
            setShowOnboarding(true);
          }
        }
      }
    };
    await checkWithRetry();
  };

  const handleModelSelected = () => {
    // Transition to main app - user has started a download
    setShowOnboarding(false);
  };

  // Render SystemAudioSetup outside of conditional rendering
  // so it's always mounted and can listen to events
  if (showOnboarding === null) {
    return (
      <>
        <Toaster />
        <SystemAudioSetup />
        <div className="h-screen flex items-center justify-center">
          <div className="text-gray-400">Loading...</div>
        </div>
      </>
    );
  }

  return (
    <>
      <Toaster />
      <SystemAudioSetup />
      {showOnboarding ? (
        <Onboarding onModelSelected={handleModelSelected} />
      ) : (
        <div className="h-screen flex flex-col">
          {/* Main content area that takes remaining space */}
          <div className="flex-1 flex overflow-hidden">
            <Sidebar
              activeSection={currentSection}
              onSectionChange={setCurrentSection}
            />
            {/* Scrollable content area */}
            <div className="flex-1 flex flex-col overflow-hidden">
              <div className="flex-1 overflow-y-auto">
                <div className="flex flex-col items-center p-4 gap-4">
                  <AccessibilityPermissions />
                  {renderSettingsContent(currentSection)}
                </div>
              </div>
            </div>
          </div>
          {/* Fixed footer at bottom */}
          <Footer />
          {/* Live Caption - Google Translate style */}
          <LiveCaption enabled={(settings?.live_caption_enabled ?? true) && settings?.always_on_microphone && settings?.audio_source === "system_audio"} />
        </div>
      )}
    </>
  );
}

export default App;
