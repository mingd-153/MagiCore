#!/bin/bash
# Generate feature-gated templates for all backend frameworks
set -e

BASE="/Users/doanmihh/Documents/Workspace/MegaGate/templates/web"

generate_backend() {
  local lang=$1
  local fw=$2
  local dir="$BASE/backend/$lang/$fw"
  
  [ ! -d "$dir/sources" ] && mkdir -p "$dir/sources"
  
  # Create template.toml
  cat > "$dir/template.toml" << TOMLEOF
[[files]]
source = "package.js.json"
target = "package.json"
required_context = ["project_slug"]
exclude_features = ["typescript"]

[[files]]
source = "package.ts.json"
target = "package.json"
required_context = ["project_slug"]
include_features = ["typescript"]

[[files]]
source = "tsconfig.json"
target = "tsconfig.json"
include_features = ["typescript"]

[[files]]
source = "server.js"
target = "src/server.js"
exclude_features = ["typescript"]

[[files]]
source = "server.ts"
target = "src/server.ts"
include_features = ["typescript"]

[[files]]
source = "app.js"
target = "src/lib/app.js"
exclude_features = ["typescript"]

[[files]]
source = "app.ts"
target = "src/lib/app.ts"
include_features = ["typescript"]
TOMLLEOF
}

# Generate template sources for each backend
generate_node() {
  local fw=$1
  local dir="$BASE/backend/node/$fw/sources"
  mkdir -p "$dir"
  
  # Package files
  cat > "$dir/package.ts.json" << EOF
{ "name": "backend", "private": true, "version": "0.1.0", "type": "module",
  "scripts": { "dev": "tsx watch src/server.ts", "build": "tsc", "start": "node dist/server.js" } }
EOF
  cat > "$dir/package.js.json" << EOF
{ "name": "backend", "private": true, "version": "0.1.0", "type": "module",
  "scripts": { "dev": "node --watch src/server.js", "start": "node src/server.js" } }
EOF
  cat > "$dir/tsconfig.json" << 'EOF'
{ "compilerOptions": { "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler", "strict": true, "esModuleInterop": true, "skipLibCheck": true, "outDir": "dist" }, "include": ["src"] }
EOF
  cat > "$dir/server.ts" << 'EXPRESS_EOF'
import { app } from "./lib/app.js";
const PORT = process.env.PORT || 3000;
app.listen(PORT, () => console.log(`Server running on port ${PORT}`));
EXPRESS_EOF
  cat > "$dir/server.js" << 'EXPRESS_EOF'
import { app } from "./lib/app.js";
const PORT = process.env.PORT || 3000;
app.listen(PORT, () => console.log(`Server running on port ${PORT}`));
EXPRESS_EOF
  cat > "$dir/app.ts" << 'EXPRESS_EOF'
import express from "express";
export const app = express();
app.use(express.json());
app.get("/api/health", (_req, res) => res.json({ status: "ok" }));
EXPRESS_EOF
  cat > "$dir/app.js" << 'EXPRESS_EOF'
import express from "express";
export const app = express();
app.use(express.json());
app.get("/api/health", (_req, res) => res.json({ status: "ok" }));
EXPRESS_EOF

  generate_backend "node" "$fw"
  # Node-specific: add eslint+prettier to template.toml
  sed -i '' '/^\[\[files\]\]/,${
    /source = "app.ts"/a\
\
[[files]]\nsource = "eslintrc.json"\ntarget = ".eslintrc.json"\ninclude_features = ["eslint"]\
\
[[files]]\nsource = "prettierrc"\ntarget = ".prettierrc"\ninclude_features = ["prettier"]\
\
[[files]]\nsource = "vitest.config.ts"\ntarget = "vitest.config.ts"\ninclude_features = ["vitest"]\
\
[[files]]\nsource = "jest.config.ts"\ntarget = "jest.config.ts"\ninclude_features = ["jest"]
  }' "$BASE/backend/node/$fw/template.toml"
  
  # Create config files
  echo '{}' > "$dir/eslintrc.json"
  echo '{}' > "$dir/prettierrc"
  echo '{}' > "$dir/vitest.config.ts"
  echo '{}' > "$dir/jest.config.ts"
}

generate_go() {
  local fw=$1
  local dir="$BASE/backend/go/$fw/sources"
  mkdir -p "$dir"
  
  cat > "$dir/go.mod" << EOF
module github.com/user/$(echo $fw)-api
go 1.22
EOF
  cat > "$dir/main.go" << 'GO_EOF'
package main

import (
	"fmt"
	"log"
	"net/http"
	"os"
)

func main() {
	port := os.Getenv("PORT")
	if port == "" { port = "3000" }
	http.HandleFunc("/api/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, `{"status":"ok"}`)
	})
	log.Printf("Server running on port %s", port)
	log.Fatal(http.ListenAndServe(":"+port, nil))
}
GO_EOF
  
  cat > "$dir/template.toml" << TOMLEOF
[[files]]
source = "go.mod"
target = "go.mod"
required_context = ["project_slug"]

[[files]]
source = "main.go"
target = "main.go"

[[files]]
source = "server.go"
target = "server.go"
TOML
  rm -f "$dir/template.toml" 2>/dev/null
  echo "$GO_TOML" > "$BASE/backend/go/$fw/template.toml"
}

echo "Generating Node backends..."
for fw in express fastify nestjs hono; do
  generate_node "$fw"
  echo "  node/$fw done"
done

echo "Generating Go backends..."
for fw in gin echo fiber; do
  generate_go "$fw"
  echo "  go/$fw done"
done

echo "Done"
