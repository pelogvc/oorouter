import Elysia from "elysia"

const errorDetails = new WeakMap<Request, string>()

export function setErrorDetail(request: Request, detail: string) {
  errorDetails.set(request, detail)
}

export const loggerPlugin = new Elysia({ name: "logger" })
  .derive({ as: "global" }, () => ({
    startedAt: performance.now(),
  }))
  .onAfterResponse({ as: "global" }, ({ request, set, startedAt }) => {
    const ms = (performance.now() - startedAt).toFixed(1)
    const method = request.method
    const pathname = new URL(request.url).pathname
    const status = typeof set.status === "number" ? set.status : 200
    const detail = errorDetails.get(request)
    errorDetails.delete(request)

    const line = detail
      ? `${method} ${pathname} ${status} ${ms}ms | ${detail}`
      : `${method} ${pathname} ${status} ${ms}ms`

    if (status >= 500) {
      process.stderr.write(`${line}\n`)
    } else if (status >= 400) {
      process.stderr.write(`${line}\n`)
    } else {
      process.stdout.write(`${line}\n`)
    }
  })
