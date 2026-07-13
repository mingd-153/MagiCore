import { json } from "@remix-run/node";

export const loader = () => json({ status: "ok", timestamp: new Date().toISOString() });

export default function Health() {
  return <div className="container"><h1>Health Check</h1><p>OK</p></div>;
}
