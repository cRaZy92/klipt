# Build context is the repo root: docker build -f server/Dockerfile .
FROM node:22-bookworm-slim AS build
ENV CI=true
RUN corepack enable
WORKDIR /repo
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY packages ./packages
COPY server ./server
RUN pnpm install --frozen-lockfile
RUN pnpm --filter @klipt/server build
RUN pnpm --filter @klipt/server deploy --prod /out

FROM node:22-bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /out/package.json ./package.json
COPY --from=build /out/node_modules ./node_modules
COPY --from=build /repo/server/dist ./dist
COPY --from=build /repo/server/public ./public
COPY --from=build /repo/server/src/db/migrations ./migrations
ENV NODE_ENV=production \
    MIGRATIONS_DIR=/app/migrations \
    PUBLIC_DIR=/app/public \
    DATA_DIR=/data
RUN mkdir -p /data && chown node:node /data
USER node
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:'+(process.env.PORT||3000)+'/api/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node","dist/main.js"]
