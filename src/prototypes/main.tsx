import React from "react";
import ReactDOM from "react-dom/client";

import { TenderWorkspacePrototype } from "./TenderWorkspacePrototype";

ReactDOM.createRoot(document.getElementById("prototype-root") as HTMLElement).render(
  <React.StrictMode>
    <TenderWorkspacePrototype />
  </React.StrictMode>,
);
