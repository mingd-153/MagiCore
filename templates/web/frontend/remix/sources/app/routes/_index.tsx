import { useLoaderData } from "@remix-run/react";
import { siteContent } from "../config/site-content";

export const loader = () => siteContent;

export default function Index() {
  const { title, description } = useLoaderData<typeof loader>();
  return (
    <main>
      <h1>{title}</h1>
      <p>{description}</p>
    </main>
  );
}