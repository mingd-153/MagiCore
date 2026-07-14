import "./styles/globals.css";
import { App } from "./App";
document.addEventListener("DOMContentLoaded", () => {
  const root = document.getElementById("root");
  if (root) App(root);
});
