import { loadConfig } from "./config"
import { createBackendAdapter } from "./providers/factory"
import { createApp } from "./app"

const config = loadConfig()
const adapter = await createBackendAdapter(config)
const app = createApp(adapter).listen(config.port)

console.log(`codex-ollama-proxy listening on http://localhost:${app.server?.port}`)
console.log(`Backend: ${adapter.name}`)
