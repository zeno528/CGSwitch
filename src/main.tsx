import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import AppShell from "./app/AppShell";
import { AppErrorBoundary } from "./app/AppErrorBoundary";
import "./style.css";

createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <AppErrorBoundary>
      <AppShell />
    </AppErrorBoundary>
  </StrictMode>,
);
