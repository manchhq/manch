import { createRootRoute, Outlet } from "@tanstack/react-router";
import { Provider as JotaiProvider } from "jotai";
import "../styles.css";

export const Route = createRootRoute({
  component: () => (
    <JotaiProvider>
      <div data-theme="manch-stage" className="h-screen w-screen overflow-hidden bg-base-300 text-base-content">
        <Outlet />
      </div>
    </JotaiProvider>
  ),
});
