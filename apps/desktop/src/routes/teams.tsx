import { createFileRoute } from "@tanstack/react-router";
import Teams from "../containers/Teams";
export const Route = createFileRoute("/teams")({ component: Teams });
