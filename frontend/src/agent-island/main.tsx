import { createRoot } from "react-dom/client";
import "../theme/tokens.css";
import "./agent-island.css";
import { AgentIslandApp } from "./AgentIslandApp";

createRoot(document.getElementById("root")!).render(<AgentIslandApp />);
