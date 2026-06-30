import { createFileRoute } from "@tanstack/react-router";
import SchedulePage from "../containers/SchedulePage";

export const Route = createFileRoute("/schedule")({ component: SchedulePage });
