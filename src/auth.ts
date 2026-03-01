export interface AuthInfo {
  readonly mode: "chatgpt" | "api_key"
  readonly accessToken: string
  readonly accountId?: string
}

export function getAuthHeaders(auth: AuthInfo): Record<string, string> {
  const headers: Record<string, string> = {
    Authorization: `Bearer ${auth.accessToken}`,
  }
  if (auth.mode === "chatgpt" && auth.accountId) {
    headers["ChatGPT-Account-ID"] = auth.accountId
  }
  return headers
}
