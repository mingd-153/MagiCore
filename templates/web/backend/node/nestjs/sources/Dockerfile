FROM node:22-alpine AS builder
WORKDIR /app
RUN apk add --no-cache curl tar bash
RUN curl -fsSL https://raw.githubusercontent.com/mingd-153/MegaGate/main/scripts/install-from-gh.sh | bash -s -- --package megagate-web --dir /usr/local/bin
COPY package*.json tsconfig.json ./
RUN mg install --frozen
COPY . .
RUN mg build

FROM node:22-alpine
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
COPY package*.json ./
EXPOSE 3000
CMD ["node", "dist/server.js"]
