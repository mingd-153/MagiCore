import React from "react";
import ReactDOM from "react-dom/client";

function App() {
  return (
    <main>
      <h1>MegaGate Core-Web Lab</h1>
      <p>React Vite fixture for PM comparison.</p>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
