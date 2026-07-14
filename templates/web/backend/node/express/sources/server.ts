import { app } from "./lib/app.js";

const port = parseInt(process.env.PORT || "3000", 10);
app.listen(port, () => console.log(`Server running on :${port}`));
