import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { AppErrorBoundary } from "./components/AppErrorBoundary";
import { installRendererDiagnostics } from "./diagnostics";
import "./index.css";

document.documentElement.lang = "en";
document.documentElement.dir = "ltr";

const queryClient = new QueryClient({
  defaultOptions: { queries: { refetchOnWindowFocus: false, staleTime: 1000 } },
});

installRendererDiagnostics();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <AppErrorBoundary>
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </React.StrictMode>
  </AppErrorBoundary>,
);
