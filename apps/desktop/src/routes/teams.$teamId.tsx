import { createFileRoute } from "@tanstack/react-router";
import TeamDetailPage from "../containers/TeamDetailPage";
export const Route = createFileRoute("/teams/$teamId")({ component: TeamDetailPage });
