import { createApp } from "vue";
import App from "./App.vue";
import { router } from "./router/AppRouter.vue";
import "./styles/globals.css";

createApp(App).use(router).mount("#app");