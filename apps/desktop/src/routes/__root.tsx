import { createRootRoute, Outlet } from "@tanstack/react-router";
import "../styles.css";

export const Route = createRootRoute({
  component: () => (
    <div className="min-h-screen bg-base-200 p-8">
      <Outlet />
    </div>
  ),
});
