import "./tokens.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { app } from "./lib/app.svelte";

app.start();
mount(App, { target: document.getElementById("app")! });
