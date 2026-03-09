import type { OllamaModelInfo, OllamaModelDetails } from "../../types/ollama"

interface ModelDefinition {
  readonly slug: string
  readonly name: string
  readonly visible: boolean
  readonly contextLength: number
  readonly supportsVision: boolean
}
const AVAILABLE_MODELS: readonly ModelDefinition[] = [
  { slug: "gpt-5.4", name: "gpt-5.4", visible: true, contextLength: 1_050_000, supportsVision: true },
  { slug: "gpt-5.3-codex", name: "gpt-5.3-codex", visible: true, contextLength: 400_000, supportsVision: true },
  { slug: "gpt-5.2-codex", name: "gpt-5.2-codex", visible: true, contextLength: 400_000, supportsVision: true },
  { slug: "gpt-5.2", name: "gpt-5.2", visible: true, contextLength: 400_000, supportsVision: true },
  { slug: "gpt-5.3-codex-spark", name: "gpt-5.3-codex-spark", visible: true, contextLength: 128_000, supportsVision: false },
  { slug: "gpt-5.4-pro", name: "gpt-5.4-pro", visible: false, contextLength: 1_050_000, supportsVision: true },
  { slug: "gpt-5-codex", name: "gpt-5-codex", visible: false, contextLength: 400_000, supportsVision: true },
  { slug: "gpt-5", name: "gpt-5", visible: false, contextLength: 400_000, supportsVision: true },
  { slug: "gpt-5-codex-mini", name: "gpt-5-codex-mini", visible: false, contextLength: 400_000, supportsVision: false },
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

export function getContextLength(name: string): number {
  const slug = name.replace(/:latest$/, "")
  const model = AVAILABLE_MODELS.find((m) => m.slug === slug)
  return model?.contextLength ?? 400_000
}

export { createModelDetails }

export function getCapabilities(name: string): readonly string[] {
  const slug = name.replace(/:latest$/, "")
  const model = AVAILABLE_MODELS.find((m) => m.slug === slug)
  const base: string[] = ["completion", "tools"]
  if (model?.supportsVision !== false) {
    base.push("vision")
  }
  return base
}
