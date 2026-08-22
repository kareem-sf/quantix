import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./quantixDesignSystem.css";

async function startQuantix() {
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installBrowserPreviewHost } = await import("./browserPreviewHost");
    await installBrowserPreviewHost();
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void startQuantix();
