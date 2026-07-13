export function App() {
  return (
    <div className="app-shell">
      <div className="scene-glow scene-glow-left" />
      <div className="scene-glow scene-glow-right" />
      <main className="welcome-shell">
        <img src="/megagate-logo.svg" alt="MegaGate" className="brand-mark" />
        <p className="welcome-copy">react + fastify</p>
        <h1 className="hero-title">{{project_name}}</h1>
        <p className="hero-subtitle">
          Fullstack React + Fastify app built with MegaGate, with the frontend served on one side and the API living cleanly beside it.
        </p>
        <p className="hero-slogan">Fullstack-ready, Fastify-powered</p>
        <div className="hero-actions">
          <a href="/api/health" className="action-pill">API health</a>
          <a href="https://github.com/mingd-153/MegaGate" className="action-pill primary">GitHub</a>
        </div>
        <footer className="hero-footer">
          Edit <code>src/App.tsx</code> and <code>server/src/server.ts</code> to shape the product and the API together.
        </footer>
      </main>
    </div>
  );
}
