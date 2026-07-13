import { brandConfig } from "../config/brand";
import { runtimeProfile } from "../config/runtime";
import { siteContent } from "../content/site-content";
import { useProjectLinks } from "../hooks/useProjectLinks";

export function renderAppShell() {
  const { primaryLink, secondaryLink } = useProjectLinks();
  const signalItems = [...siteContent.signal, runtimeProfile.engine.mode]
    .map((item) => `<span class="signal-item${item === runtimeProfile.engine.mode ? " signal-runtime" : ""}">${item}</span>`)
    .join("");

  return `
    <main class="app-shell">
      <div class="scene-glow scene-glow-left" aria-hidden="true"></div>
      <div class="scene-glow scene-glow-right" aria-hidden="true"></div>
      <section class="welcome-shell">
        <div class="eyebrow">
          <img class="brand-mark" src="${brandConfig.logoPath}" alt="MegaGate logo" />
          <span>${siteContent.eyebrow}</span>
        </div>
        <p class="welcome-copy">${siteContent.welcome}</p>
        <h1 class="hero-title">${siteContent.title}</h1>
        <p class="hero-subtitle">${siteContent.subtitle}</p>
        <p class="hero-slogan">${siteContent.slogan}</p>
        <div class="signal-strip" aria-label="project qualities">${signalItems}</div>
        <div class="hero-actions">
          <a class="action-pill primary" href="${primaryLink.href}" target="_blank" rel="noreferrer">${primaryLink.label}</a>
          <a class="action-pill" href="${secondaryLink.href}" target="_blank" rel="noreferrer">${secondaryLink.label}</a>
        </div>
        <p class="hero-footer">${siteContent.footer}</p>
      </section>
    </main>
  `;
}
