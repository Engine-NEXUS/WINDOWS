import { createRoot } from "react-dom/client";
import "../theme/tokens.css";
import { NotifyToastApp } from "./NotifyToastApp";

createRoot(document.getElementById("root")!).render(<NotifyToastApp />);
