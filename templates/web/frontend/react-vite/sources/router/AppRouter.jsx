import { frameworkConfig } from "../config/framework";

export function AppRouter() {
  return (
    <main className="min-h-screen bg-neutral-950 px-6 py-10 text-white">
      <div className="mx-auto flex min-h-[calc(100vh-5rem)] max-w-5xl items-center justify-center">
        <section className="w-full rounded-3xl border border-white/10 bg-white/[0.03] p-8 shadow-2xl shadow-cyan-950/20 backdrop-blur sm:p-12">
          <div className="mb-6 inline-flex rounded-full border border-cyan-400/20 bg-cyan-400/10 px-3 py-1 text-xs font-medium uppercase tracking-[0.24em] text-cyan-200">
            Welcome to MegaGate
          </div>
          <div className="space-y-6">
            <div className="space-y-3">
              <h1 className="text-4xl font-semibold tracking-tight text-white sm:text-6xl">
                MG + {frameworkConfig.shortName}
              </h1>
              <p className="max-w-2xl text-base leading-7 text-neutral-300 sm:text-lg">
                Native web scaffolding with room for stricter package policy, sharper local
                performance, and a cleaner path into heavier apps.
              </p>
            </div>
            <div className="flex flex-wrap gap-3 text-sm text-neutral-200">
              {frameworkConfig.signal.map((item) => (
                <span
                  key={item}
                  className="rounded-full border border-white/10 bg-white/5 px-3 py-1"
                >
                  {item}
                </span>
              ))}
            </div>
            <div className="flex flex-wrap gap-3">
              <a
                className="inline-flex items-center justify-center rounded-full bg-white px-5 py-2.5 text-sm font-medium text-neutral-950 transition hover:bg-cyan-100"
                href="https://github.com/mingd-153/MegaGate"
                rel="noreferrer"
                target="_blank"
              >
                MegaGate GitHub
              </a>
              <a
                className="inline-flex items-center justify-center rounded-full border border-white/15 px-5 py-2.5 text-sm font-medium text-white transition hover:border-cyan-300/40 hover:bg-white/5"
                href={frameworkConfig.docs.href}
                rel="noreferrer"
                target="_blank"
              >
                {frameworkConfig.docs.label}
              </a>
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}
