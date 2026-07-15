import { siteContent } from "@/config/site-content";

export default function Home() {
  return (
    <main>
      <h1>{siteContent.title}</h1>
      <p>{siteContent.description}</p>
    </main>
  );
}