import { createFileRoute } from "@tanstack/react-router";
import SearchPage from "../containers/SearchPage";

export const Route = createFileRoute("/search")({ component: SearchPage });
