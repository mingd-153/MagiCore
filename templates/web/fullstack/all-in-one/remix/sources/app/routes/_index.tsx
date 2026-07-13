import { Link } from "@remix-run/react";

export default function Index() {
  return (
    <div className="container">
      <h1>{{project_name}}</h1>
      <p>Scaffolded with MegaGate · Remix</p>
      <ul>
        <li>
          <Link to="/health">Health</Link>
        </li>
      </ul>
    </div>
  );
}
