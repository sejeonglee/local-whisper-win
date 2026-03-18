import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { StatusOverlay } from "./components/StatusOverlay";
import { defaultAppState, type AppSnapshot } from "./lib/app-state";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export default function App() {
  const [state, setState] = useState<AppSnapshot>(defaultAppState);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let active = true;
    let detach: (() => void) | undefined;

    void invoke<AppSnapshot>("get_app_state")
      .then((snapshot) => {
        if (active) {
          setState(snapshot);
        }
      })
      .catch(() => {
      });

    void listen<AppSnapshot>("app-state-changed", (event) => {
      if (active) {
        setState(event.payload);
      }
    })
      .then((unlisten) => {
        detach = unlisten;
      })
      .catch(() => {
      });

    return () => {
      active = false;
      if (detach) {
        detach();
      }
    };
  }, []);

  return <StatusOverlay state={state} />;
}
