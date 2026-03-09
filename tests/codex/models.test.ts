import { describe, expect, test } from "vitest"
import {
  getVisibleModels,
  getAllModels,
  modelExists,
  createModelDetails,
} from "../../src/providers/codex/models"

describe("getVisibleModels", () => {
  test("returns only visible models", () => {
    const models = getVisibleModels()
    expect(models.length).toBe(5)
    const names = models.map((m) => m.name)
    expect(names).toContain("gpt-5.4:latest")
    expect(names).toContain("gpt-5.3-codex:latest")
    expect(names).toContain("gpt-5.3-codex-spark:latest")
    expect(names).toContain("gpt-5.2-codex:latest")
    expect(names).toContain("gpt-5.2:latest")
  })

  test("does not include hidden models", () => {
    const names = getVisibleModels().map((m) => m.name)
    expect(names).not.toContain("gpt-5.4-pro:latest")
    expect(names).not.toContain("gpt-5:latest")
    expect(names).not.toContain("gpt-5-codex:latest")
    expect(names).not.toContain("gpt-5-codex-mini:latest")
  })

  test("returns correct model info format", () => {
    const model = getVisibleModels()[0]
    expect(model.name).toMatch(/:latest$/)
    expect(model.model).toBe(model.name)
    expect(model.size).toBe(0)
    expect(model.digest).toMatch(/^sha256:/)
    expect(model.details.family).toBe("gpt")
    expect(model.details.format).toBe("api")
  })
})

describe("getAllModels", () => {
  test("returns all 9 models", () => {
    expect(getAllModels().length).toBe(9)
  })
  test("includes both visible and hidden models", () => {
    const names = getAllModels().map((m) => m.name)
    expect(names).toContain("gpt-5.4:latest")
    expect(names).toContain("gpt-5.3-codex:latest")
    expect(names).toContain("gpt-5.3-codex-spark:latest")
    expect(names).toContain("gpt-5:latest")
  })
})
describe("modelExists", () => {
  test("returns true for existing model", () => {
    expect(modelExists("gpt-5.3-codex")).toBe(true)
    expect(modelExists("gpt-5")).toBe(true)
    expect(modelExists("gpt-5.4")).toBe(true)
  })

  test("strips :latest suffix", () => {
    expect(modelExists("gpt-5.3-codex:latest")).toBe(true)
  })

  test("returns false for nonexistent model", () => {
    expect(modelExists("nonexistent")).toBe(false)
    expect(modelExists("")).toBe(false)
  })

  test("returns true for hidden models", () => {
    expect(modelExists("gpt-5-codex")).toBe(true)
    expect(modelExists("gpt-5-codex-mini")).toBe(true)
    expect(modelExists("gpt-5.4-pro")).toBe(true)
  })
})

describe("createModelDetails", () => {
  test("returns complete details object", () => {
    const details = createModelDetails()
    expect(details.parent_model).toBe("")
    expect(details.format).toBe("api")
    expect(details.family).toBe("gpt")
    expect(details.families).toEqual(["gpt"])
    expect(details.parameter_size).toBe("unknown")
    expect(details.quantization_level).toBe("none")
  })
})
