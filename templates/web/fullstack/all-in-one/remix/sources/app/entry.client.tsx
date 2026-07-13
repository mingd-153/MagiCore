import type { LinksFunction } from "@remix-run/node";
import styles from "./styles/app.css?url";

export const links: LinksFunction = () => [
  { rel: "stylesheet", href: styles },
];
