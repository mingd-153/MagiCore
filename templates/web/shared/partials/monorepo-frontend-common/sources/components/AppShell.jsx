import { brandConfig } from "../config/brand";
import { runtimeProfile } from "../config/runtime";
import { siteContent } from "../content/site-content";
import { useProjectLinks } from "../hooks/useProjectLinks";

export function AppShell() {
  const { primaryLink, secondaryLink } = useProjectLinks();

  return (
    <main className="app-shell">
      <div className="scene-glow scene-glow-left" aria-hidden="true" />
      <div className="scene-glow scene-glow-right" aria-hidden="true" />
      <section className="welcome-shell">
        <div className="eyebrow">
          <img className="brand-mark" src={brandConfig.logoPath} alt="MegaGate logo" />
          <span>{siteContent.eyebrow}</span>
        </div>
        <p className="welcome-copy">{siteContent.welcome}</p>
        <h1 className="hero-title">{siteContent.title}</h1>
        <p className="hero-subtitle">{siteContent.subtitle}</p>
        <p className="hero-slogan">{siteContent.slogan}</p>
        <div className="signal-strip" aria-label="project qualities">
          {siteContent.signal.map((item) => (
            <span className="signal-item" key={item}>
              {item}
            </span>
          ))}
          <span className="signal-item signal-runtime">{runtimeProfile.engine.mode}</span>
        </div>
        <div className="hero-actions">
          <a className="action-pill primary" href={primaryLink.href} target="_blank" rel="noreferrer">
            {primaryLink.label}
          </a>
          <a className="action-pill" href={secondaryLink.href} target="_blank" rel="noreferrer">
            {secondaryLink.label}
          </a>
        </div>
        <p className="hero-footer">{siteContent.footer}</p>
      </section>
    </main>
  );
}
