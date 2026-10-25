import "./tokens.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { app } from "./lib/app.svelte";

// The splash in the appearance the person chose, not only the system's.
try {
  const theme = localStorage.getItem("speccoder-theme");
  if (theme === "light" || theme === "dark") document.getElementById("splash")?.classList.add(theme);
} catch {
  /* no storage: the system's appearance */
}

app.start();
mount(App, { target: document.getElementById("app")! });
