import type { OllamaModelInfo, OllamaModelDetails } from "../../types/ollama"

interface ModelDefinition {
  readonly slug: string
  readonly name: string
  readonly visible: boolean
}

const AVAILABLE_MODELS: readonly ModelDefinition[] = [
  { slug: "gpt-5.3-codex", name: "gpt-5.3-codex", visible: true },
  { slug: "gpt-5.2-codex", name: "gpt-5.2-codex", visible: true },
  { slug: "gpt-5.1-codex-max", name: "gpt-5.1-codex-max", visible: true },
  { slug: "gpt-5.1-codex-mini", name: "gpt-5.1-codex-mini", visible: true },
  { slug: "gpt-5.2", name: "gpt-5.2", visible: true },
  { slug: "gpt-5.1-codex", name: "gpt-5.1-codex", visible: false },
  { slug: "gpt-5.1", name: "gpt-5.1", visible: false },
  { slug: "gpt-5-codex", name: "gpt-5-codex", visible: false },
  { slug: "gpt-5", name: "gpt-5", visible: false },
  { slug: "gpt-5-codex-mini", name: "gpt-5-codex-mini", visible: false },
]

function createModelDetails(): OllamaModelDetails {
  return {
    parent_model: "",
    format: "api",
    family: "gpt",
    families: ["gpt"],
    parameter_size: "unknown",
    quantization_level: "none",
  }
}

function toOllamaModelInfo(model: ModelDefinition): OllamaModelInfo {
  return {
    name: `${model.slug}:latest`,
    model: `${model.slug}:latest`,
    modified_at: new Date().toISOString(),
    size: 0,
    digest: `sha256:${"0".repeat(64)}`,
    details: createModelDetails(),
  }
}

export function getVisibleModels(): readonly OllamaModelInfo[] {
  return AVAILABLE_MODELS.filter((m) => m.visible).map(toOllamaModelInfo)
}

export function getAllModels(): readonly OllamaModelInfo[] {
  return AVAILABLE_MODELS.map(toOllamaModelInfo)
}

export function modelExists(name: string): boolean {
  const slug = name.replace(/:latest$/, "")
  return AVAILABLE_MODELS.some((m) => m.slug === slug)
}

export { createModelDetails }
