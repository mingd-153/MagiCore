import { app } from "./app.js";

const port = parseInt(process.env.PORT || "3000", 10);
await app.listen({ host: "0.0.0.0", port });
console.log(`Server running on :${port}`);
