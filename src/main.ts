import { createApp } from "vue";
import App from "./App.vue";
import "./style.css";

window.addEventListener("contextmenu", (e) => e.preventDefault());
window.addEventListener("keydown", (e) => {
  const k = e.key.toLowerCase();
  if (k === "f12") return e.preventDefault();
  const mod = e.metaKey || e.ctrlKey;
  if (mod && (e.shiftKey || e.altKey) && (k === "i" || k === "j" || k === "c")) {
    e.preventDefault();
  }
});

createApp(App).mount("#app");
