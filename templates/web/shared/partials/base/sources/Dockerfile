FROM node:20-alpine AS base
WORKDIR /app
RUN apk add --no-cache curl tar bash
RUN curl -fsSL https://raw.githubusercontent.com/mingd-153/MegaGate/main/scripts/install-from-gh.sh | bash -s -- --package megagate-web --dir /usr/local/bin
COPY package*.json ./
RUN mg install --frozen

FROM base AS builder
COPY . .
RUN mg build

FROM node:20-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
COPY --from=base /usr/local/bin/mg /usr/local/bin/mg
COPY --from=builder /app ./
EXPOSE 3000
CMD ["mg", "start"]
