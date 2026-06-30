import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../containers/SettingsPage";

export const Route = createFileRoute("/settings")({ component: SettingsPage });
