<script setup lang="ts">
import { brandConfig } from "../config/brand";
import { runtimeProfile } from "../config/runtime";
import { siteContent } from "../content/site-content";
import { useProjectLinks } from "../hooks/useProjectLinks";

const { primaryLink, secondaryLink } = useProjectLinks();
</script>

<template>
  <main class="app-shell">
    <div class="scene-glow scene-glow-left" aria-hidden="true" />
    <div class="scene-glow scene-glow-right" aria-hidden="true" />
    <section class="welcome-shell">
      <div class="eyebrow">
        <img class="brand-mark" :src="brandConfig.logoPath" alt="MegaGate logo" />
        <span v-text="siteContent.eyebrow" />
      </div>
      <p class="welcome-copy" v-text="siteContent.welcome" />
      <h1 class="hero-title" v-text="siteContent.title" />
      <p class="hero-subtitle" v-text="siteContent.subtitle" />
      <p class="hero-slogan" v-text="siteContent.slogan" />
      <div class="signal-strip" aria-label="project qualities">
        <span v-for="item in siteContent.signal" :key="item" class="signal-item">
          <span v-text="item" />
        </span>
        <span class="signal-item signal-runtime" v-text="runtimeProfile.engine.mode" />
      </div>
      <div class="hero-actions">
        <a class="action-pill primary" :href="primaryLink.href" target="_blank" rel="noreferrer">
          <span v-text="primaryLink.label" />
        </a>
        <a class="action-pill" :href="secondaryLink.href" target="_blank" rel="noreferrer">
          <span v-text="secondaryLink.label" />
        </a>
      </div>
      <p class="hero-footer" v-text="siteContent.footer" />
    </section>
  </main>
</template>
