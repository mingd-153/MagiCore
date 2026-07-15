import { component$ } from '@builder.io/qwik';
import { QwikCityProvider, RouterOutlet, ServiceWorkerRegister } from '@builder.io/qwik-city';
export default component$(() => (
  <QwikCityProvider>
    <head><meta charSet="utf-8"/><title>{{project_name}}</title></head>
    <body lang="en"><RouterOutlet /><ServiceWorkerRegister /></body>
  </QwikCityProvider>
));
