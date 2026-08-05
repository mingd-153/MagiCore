import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
await app.listen({ host: "127.0.0.1", port });
console.log(`Server running on :${port}`);
