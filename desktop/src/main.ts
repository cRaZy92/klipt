import { mount } from "svelte";
import App from "./App.svelte";
// The heading font is bundled: the webview's CSP only loads fonts from the app itself.
import "@fontsource-variable/bricolage-grotesque/wght.css";
import "./app.css";

const app = mount(App, { target: document.getElementById("app")! });

export default app;
