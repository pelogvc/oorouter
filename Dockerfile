FROM oven/bun:1.3.10-slim AS base
WORKDIR /app

FROM base AS install
COPY package.json bun.lock* ./
RUN bun install --frozen-lockfile --production

FROM base AS release
COPY --from=install /app/node_modules ./node_modules
COPY package.json ./
COPY src ./src

ENV PORT=11434
EXPOSE 11434

USER bun
CMD ["bun", "run", "src/index.ts"]
