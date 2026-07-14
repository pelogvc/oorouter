import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  createClientApiKey,
  copyClientApiKey,
  deleteClientApiKey,
  isTauriRuntime,
  listClientApiKeys,
  revealClientApiKey,
  setClientAuthEnabled,
  type ClientApiKey,
  type ClientAuthState,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  AlertCircle,
  Check,
  Clipboard,
  Eye,
  EyeOff,
  KeyRound,
  Loader2,
  LockKeyhole,
  MonitorCog,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  Unlock,
} from "lucide-react";

type LoadState = "loading" | "ready" | "error" | "unavailable";

const REVEAL_TIMEOUT_MS = 30_000;

interface RevealedKey {
  id: string;
  value: string;
}

type MutationSuccessMessage = string | ((state: ClientAuthState) => string);

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function displayName(key: ClientApiKey): string {
  return key.name || "Unnamed key";
}

function formatCreatedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

function savedKeySummary(auth: ClientAuthState): string {
  if (auth.keys.length === 1) return "1 saved key is accepted.";
  return `${auth.keys.length} saved keys are accepted.`;
}

function switchHelpText(
  desktop: boolean,
  loadState: LoadState,
  auth: ClientAuthState
): string {
  if (!desktop) return "Authentication status is available only in the desktop app.";
  if (loadState === "loading") return "Loading saved keys and authentication status.";
  if (loadState === "error") return "Authentication status is unavailable. Retry below.";
  if (auth.keys.length === 0) return "Create at least one key before turning authentication on.";
  if (auth.enabled) return savedKeySummary(auth);
  return "Saved keys stay available while authentication is off.";
}

export default function Auth() {
  const desktop = useMemo(() => isTauriRuntime(), []);
  const [auth, setAuth] = useState<ClientAuthState>({ enabled: false, keys: [] });
  const [loadState, setLoadState] = useState<LoadState>(
    desktop ? "loading" : "unavailable"
  );
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState("");
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [revealed, setRevealed] = useState<RevealedKey | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ClientApiKey | null>(null);
  const activeActionRef = useRef<string | null>(null);
  const loadRequestRef = useRef(0);
  const revealedRef = useRef<RevealedKey | null>(null);
  const revealTimeoutRef = useRef<number | null>(null);
  const deleteTriggerRef = useRef<HTMLButtonElement | null>(null);
  const deleteButtonRefs = useRef(new Map<string, HTMLButtonElement>());
  const generateButtonRef = useRef<HTMLButtonElement | null>(null);

  const clearRevealTimer = useCallback(() => {
    if (revealTimeoutRef.current !== null) {
      window.clearTimeout(revealTimeoutRef.current);
      revealTimeoutRef.current = null;
    }
  }, []);

  const maskRevealedKey = useCallback(
    (message?: string) => {
      clearRevealTimer();
      if (!revealedRef.current) return;
      revealedRef.current = null;
      setRevealed(null);
      if (message) setStatusMessage(message);
    },
    [clearRevealTimer]
  );

  const scheduleRevealMask = useCallback(
    (key: ClientApiKey) => {
      clearRevealTimer();
      revealTimeoutRef.current = window.setTimeout(() => {
        if (revealedRef.current?.id !== key.id) return;
        revealedRef.current = null;
        setRevealed(null);
        setStatusMessage(`${displayName(key)} hidden automatically.`);
      }, REVEAL_TIMEOUT_MS);
    },
    [clearRevealTimer]
  );

  const beginAction = useCallback((action: string) => {
    if (activeActionRef.current) return false;
    activeActionRef.current = action;
    loadRequestRef.current += 1;
    setPendingAction(action);
    setActionError(null);
    setStatusMessage("");
    return true;
  }, []);

  const finishAction = useCallback(() => {
    activeActionRef.current = null;
    setPendingAction(null);
  }, []);

  const load = useCallback(async () => {
    if (!desktop) return;
    const requestId = loadRequestRef.current + 1;
    loadRequestRef.current = requestId;
    setLoadState("loading");
    setLoadError(null);
    try {
      const state = await listClientApiKeys();
      if (loadRequestRef.current !== requestId) return;
      setAuth(state);
      setLoadState("ready");
    } catch (error) {
      if (loadRequestRef.current !== requestId) return;
      setLoadError(errorMessage(error));
      setLoadState("error");
    }
  }, [desktop]);

  useEffect(() => {
    if (desktop) void load();
    return () => {
      loadRequestRef.current += 1;
      clearRevealTimer();
    };
  }, [clearRevealTimer, desktop, load]);

  useEffect(() => {
    const handleBlur = () => maskRevealedKey();
    const handleVisibilityChange = () => {
      if (document.visibilityState === "hidden") maskRevealedKey();
    };

    window.addEventListener("blur", handleBlur);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      window.removeEventListener("blur", handleBlur);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [maskRevealedKey]);

  const runMutation = useCallback(
    async (
      action: string,
      mutation: () => Promise<ClientAuthState>,
      success: MutationSuccessMessage
    ) => {
      if (!beginAction(action)) return false;
      try {
        const state = await mutation();
        setAuth(state);
        setStatusMessage(typeof success === "function" ? success(state) : success);
        return true;
      } catch (error) {
        setActionError(errorMessage(error));
        return false;
      } finally {
        finishAction();
      }
    },
    [beginAction, finishAction]
  );

  const handleCreate = async (event: FormEvent) => {
    event.preventDefault();
    if (!desktop || loadState !== "ready" || activeActionRef.current) return;
    const trimmedName = name.trim();
    if (Array.from(trimmedName).length > 64) {
      setActionError("Key name must be 64 characters or fewer.");
      return;
    }
    const created = await runMutation(
      "create",
      () => createClientApiKey(trimmedName || undefined),
      (state) => {
        if (state.enabled) return "API key created and accepted for OpenAI endpoints.";
        return "API key created. Authentication remains off until you enable it.";
      }
    );
    if (created) setName("");
  };

  const handleToggle = async (enabled: boolean) => {
    if (loadState !== "ready" || activeActionRef.current) return;
    await runMutation(
      "toggle",
      () => setClientAuthEnabled(enabled),
      enabled ? "OpenAI endpoint authentication enabled." : "OpenAI endpoint authentication disabled."
    );
  };

  const handleReveal = async (key: ClientApiKey) => {
    if (revealedRef.current?.id === key.id) {
      maskRevealedKey(`${displayName(key)} hidden.`);
      return;
    }
    const action = `reveal:${key.id}`;
    if (!beginAction(action)) return;
    maskRevealedKey();
    try {
      const secret = await revealClientApiKey(key.id);
      const nextRevealed = { id: secret.id, value: secret.value };
      revealedRef.current = nextRevealed;
      setRevealed(nextRevealed);
      scheduleRevealMask(key);
      setStatusMessage(`${displayName(key)} revealed.`);
    } catch (error) {
      maskRevealedKey();
      setActionError(errorMessage(error));
    } finally {
      finishAction();
    }
  };

  const handleCopy = async (key: ClientApiKey) => {
    const action = `copy:${key.id}`;
    if (!beginAction(action)) return;
    try {
      await copyClientApiKey(key.id);
      setStatusMessage(`${displayName(key)} copied to clipboard.`);
    } catch (error) {
      setActionError(`Could not copy the key. ${errorMessage(error)}`);
    } finally {
      finishAction();
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget || activeActionRef.current) return;
    const target = deleteTarget;
    const confirmedAutoDisable = auth.enabled && auth.keys.length === 1;
    const action = `delete:${target.id}`;
    if (!beginAction(action)) return;
    setDeleteError(null);
    try {
      const result = await deleteClientApiKey(target.id, confirmedAutoDisable);
      setAuth(result.state);
      if (revealedRef.current?.id === target.id) maskRevealedKey();
      setStatusMessage(
        result.autoDisabled
          ? `${displayName(target)} deleted. Authentication was turned off.`
          : `${displayName(target)} deleted.`
      );
      setDeleteTarget(null);
    } catch (error) {
      setDeleteError(`Could not delete the key. ${errorMessage(error)}`);
    } finally {
      finishAction();
    }
  };

  const openDeleteDialog = (key: ClientApiKey, trigger: HTMLButtonElement) => {
    if (activeActionRef.current) return;
    deleteTriggerRef.current = trigger;
    setDeleteError(null);
    setDeleteTarget(key);
  };

  const handleDeleteDialogCloseAutoFocus = (event: Event) => {
    event.preventDefault();
    const trigger = deleteTriggerRef.current;
    if (trigger?.isConnected) {
      trigger.focus();
    } else {
      const nextDeleteButton = Array.from(deleteButtonRefs.current.values()).find(
        (button) => button.isConnected && !button.disabled
      );
      (nextDeleteButton ?? generateButtonRef.current)?.focus();
    }
    deleteTriggerRef.current = null;
  };

  const busy = pendingAction !== null;
  const noKeys = loadState === "ready" && auth.keys.length === 0;
  const deletingLastEnabledKey =
    Boolean(deleteTarget) && auth.enabled && auth.keys.length === 1;
  const deleting = pendingAction?.startsWith("delete:") ?? false;
  const switchHelp = switchHelpText(desktop, loadState, auth);

  let authStatusLabel = "Unavailable";
  let authStatusIcon = <MonitorCog className="h-3 w-3" aria-hidden="true" />;
  let authStatusClass = "text-muted-foreground";
  if (loadState === "loading") {
    authStatusLabel = "Loading";
    authStatusIcon = (
      <Loader2 className="h-3 w-3 animate-spin motion-reduce:animate-none" aria-hidden="true" />
    );
  } else if (loadState === "ready") {
    authStatusLabel = auth.enabled ? "On" : "Off";
    if (auth.enabled) authStatusClass = "border-primary/30 bg-primary/10 text-primary";
    authStatusIcon = auth.enabled ? (
      <LockKeyhole className="h-3 w-3" aria-hidden="true" />
    ) : (
      <Unlock className="h-3 w-3" aria-hidden="true" />
    );
  } else if (loadState === "error") {
    authStatusClass = "border-destructive-text/25 text-destructive-text";
    authStatusIcon = <AlertCircle className="h-3 w-3" aria-hidden="true" />;
  }

  let unavailableControlLabel = "Desktop only";
  if (desktop && loadState === "loading") unavailableControlLabel = "Loading";

  return (
    <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col gap-4 p-4 pb-6">
      <section aria-labelledby="client-auth-heading">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h1 id="client-auth-heading" className="text-lg font-semibold tracking-tight">
              Client API authentication
            </h1>
          </div>
          <Badge
            variant="outline"
            className={cn(
              "h-6 shrink-0 gap-1.5 rounded-md px-2 text-[10px] font-semibold uppercase tracking-wide",
              authStatusClass
            )}
          >
            {authStatusIcon}
            {authStatusLabel}
          </Badge>
        </div>
      </section>

      <div className="grid overflow-hidden rounded-lg border bg-card sm:grid-cols-2" role="note">
        <div className="flex min-w-0 items-center gap-3 border-b px-4 py-3 sm:border-b-0 sm:border-r">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
            <ShieldCheck className="h-4 w-4" aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <p className="truncate text-xs font-semibold">OpenAI · /v1/*</p>
            <p className="truncate text-[11px] text-muted-foreground">Protected when enabled</p>
          </div>
        </div>
        <div className="flex min-w-0 items-center gap-3 px-4 py-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
            <Unlock className="h-4 w-4" aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <p className="truncate text-xs font-semibold">Ollama · /api/*</p>
            <p className="truncate text-[11px] text-muted-foreground">Always accessible without a key</p>
          </div>
        </div>
      </div>

      {!desktop && (
        <div className="flex gap-3 rounded-lg border border-info/25 bg-info/5 px-4 py-3" role="note">
          <MonitorCog className="mt-0.5 h-4 w-4 shrink-0 text-info" aria-hidden="true" />
          <div>
            <p className="text-xs font-semibold">Desktop app required</p>
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              Browser preview is read-only. Open the Tauri app to create or manage keys.
            </p>
          </div>
        </div>
      )}

      <Card>
        <CardContent className="flex items-start justify-between gap-5 p-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <LockKeyhole className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
              <h2 className="text-sm font-semibold">Require a key for OpenAI endpoints</h2>
            </div>
            <p id="auth-switch-help" className="mt-1.5 text-xs leading-5 text-muted-foreground">
              {switchHelp}
            </p>
          </div>
          {loadState === "ready" ? (
            <Switch
              checked={auth.enabled}
              onCheckedChange={(checked) => void handleToggle(checked)}
              disabled={noKeys || busy}
              aria-label="Require an API key for OpenAI-compatible endpoints"
              aria-describedby="auth-switch-help"
            />
          ) : (
            <Badge variant="outline" className="h-6 shrink-0 rounded-md text-[10px] text-muted-foreground">
              {unavailableControlLabel}
            </Badge>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardContent className="space-y-4 p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h2 className="text-sm font-semibold">API keys</h2>
              <p className="mt-0.5 text-xs text-muted-foreground">
                Keys are stored locally and masked by default.
              </p>
            </div>
            {loadState === "ready" && (
              <span className="font-mono text-xs text-muted-foreground">
                {auth.keys.length} {auth.keys.length === 1 ? "key" : "keys"}
              </span>
            )}
          </div>

          <form className="flex items-end gap-2" onSubmit={(event) => void handleCreate(event)}>
            <div className="min-w-0 flex-1">
              <label htmlFor="client-api-key-name" className="block text-xs font-medium">
                Name <span className="font-normal text-muted-foreground">(optional)</span>
              </label>
              <Input
                id="client-api-key-name"
                name="client-api-key-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                maxLength={64}
                autoComplete="off"
                placeholder="e.g. Local development"
                disabled={!desktop || busy || loadState !== "ready"}
                aria-describedby="client-api-key-name-help"
                className="mt-1.5"
              />
              <p id="client-api-key-name-help" className="sr-only">
                Optional label, up to 64 characters. Duplicate names are allowed.
              </p>
            </div>
            <Button
              ref={generateButtonRef}
              type="submit"
              className="shrink-0 gap-2"
              disabled={!desktop || busy || loadState !== "ready"}
            >
              {pendingAction === "create" ? (
                <Loader2 className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
              ) : (
                <Plus className="h-4 w-4" aria-hidden="true" />
              )}
              Generate key
            </Button>
          </form>

          {loadState === "loading" && (
            <div className="space-y-2" aria-busy="true" aria-label="Loading API keys">
              {[1, 2].map((item) => (
                <div key={item} className="h-20 animate-pulse rounded-md bg-muted motion-reduce:animate-none" />
              ))}
            </div>
          )}

          {loadState === "error" && (
            <div className="flex items-center justify-between gap-3 rounded-md border border-destructive-text/25 px-3 py-3" role="alert">
              <div className="flex min-w-0 gap-2">
                <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-destructive-text" aria-hidden="true" />
                <div className="min-w-0">
                  <p className="text-xs font-semibold text-destructive-text">Could not load API keys</p>
                  <p className="mt-0.5 truncate text-xs text-muted-foreground">{loadError}</p>
                </div>
              </div>
              <Button type="button" variant="outline" size="sm" className="gap-1.5" onClick={() => void load()}>
                <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                Retry
              </Button>
            </div>
          )}

          {loadState === "unavailable" && (
            <div className="flex min-h-24 items-center gap-3 rounded-md border border-dashed px-4 py-4" role="status">
              <MonitorCog className="h-5 w-5 shrink-0 text-muted-foreground" aria-hidden="true" />
              <div>
                <p className="text-xs font-semibold">Key list unavailable in browser preview</p>
                <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
                  Open the desktop app to view saved keys and authentication status.
                </p>
              </div>
            </div>
          )}

          {loadState === "ready" && noKeys && (
            <div className="flex min-h-28 flex-col items-center justify-center rounded-md border border-dashed px-4 py-6 text-center">
              <KeyRound className="h-7 w-7 text-muted-foreground/40" aria-hidden="true" />
              <p className="mt-2 text-sm font-medium">No API keys yet</p>
              <p className="mt-1 text-xs text-muted-foreground">
                Generate a key above. Authentication will remain off.
              </p>
            </div>
          )}

          {loadState === "ready" && !noKeys && (
            <ul className="divide-y overflow-hidden rounded-md border" aria-label="Client API keys">
              {auth.keys.map((key) => {
                const isRevealed = revealed?.id === key.id;
                const revealPanelId = `client-api-key-reveal-${key.id}`;
                let revealIcon = <Eye className="h-4 w-4" aria-hidden="true" />;
                if (pendingAction === `reveal:${key.id}`) {
                  revealIcon = (
                    <Loader2
                      className="h-4 w-4 animate-spin motion-reduce:animate-none"
                      aria-hidden="true"
                    />
                  );
                } else if (isRevealed) {
                  revealIcon = <EyeOff className="h-4 w-4" aria-hidden="true" />;
                }
                return (
                  <li key={key.id} className="bg-background px-3 py-3">
                    <div className="flex min-w-0 items-start justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-medium" title={displayName(key)}>
                          {displayName(key)}
                        </p>
                        <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                          <time dateTime={key.createdAt}>{formatCreatedAt(key.createdAt)}</time>
                          <span aria-hidden="true"> · </span>
                          <span title={key.id}>ID {key.id.slice(0, 8)}</span>
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-9 w-9"
                          onClick={() => void handleReveal(key)}
                          disabled={!desktop || busy}
                          aria-label={isRevealed ? `Hide ${displayName(key)}` : `Reveal ${displayName(key)}`}
                          aria-expanded={isRevealed}
                          aria-controls={revealPanelId}
                        >
                          {revealIcon}
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-9 w-9"
                          onClick={() => void handleCopy(key)}
                          disabled={!desktop || busy}
                          aria-label={`Copy ${displayName(key)}`}
                        >
                          {pendingAction === `copy:${key.id}` ? (
                            <Loader2 className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
                          ) : (
                            <Clipboard className="h-4 w-4" aria-hidden="true" />
                          )}
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-9 w-9 text-muted-foreground hover:text-destructive-text"
                          ref={(button) => {
                            if (button) deleteButtonRefs.current.set(key.id, button);
                            else deleteButtonRefs.current.delete(key.id);
                          }}
                          onClick={(event) => openDeleteDialog(key, event.currentTarget)}
                          disabled={!desktop || busy}
                          aria-label={`Delete ${displayName(key)}`}
                        >
                          <Trash2 className="h-4 w-4" aria-hidden="true" />
                        </Button>
                      </div>
                    </div>
                    <div className="mt-2 truncate rounded bg-muted px-2.5 py-1.5 font-mono text-[11px] text-muted-foreground">
                      {key.redactedValue}
                    </div>
                    <div
                      id={revealPanelId}
                      hidden={!isRevealed}
                      className="mt-2 rounded-md border border-primary/20 bg-primary/[0.04] p-2.5"
                    >
                      {isRevealed && (
                        <>
                          <p className="mb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                            Revealed value · hides in 30 seconds
                          </p>
                          <code className="block break-all font-mono text-[11px] leading-5 text-foreground">
                            {revealed.value}
                          </code>
                        </>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </CardContent>
      </Card>

      {(actionError || statusMessage) && (
        <div
          className={cn(
            "flex items-start gap-2 rounded-md border px-3 py-2.5 text-xs",
            actionError
              ? "border-destructive-text/25 text-destructive-text"
              : "border-success/25 text-foreground"
          )}
          role={actionError ? "alert" : "status"}
          aria-live={actionError ? "assertive" : "polite"}
        >
          {actionError ? (
            <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          ) : (
            <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-success" aria-hidden="true" />
          )}
          <span>{actionError || statusMessage}</span>
        </div>
      )}

      <Dialog
        open={Boolean(deleteTarget)}
        onOpenChange={(open) => {
          if (!open && !activeActionRef.current?.startsWith("delete:")) {
            setDeleteError(null);
            setDeleteTarget(null);
          }
        }}
      >
        <DialogContent
          onCloseAutoFocus={handleDeleteDialogCloseAutoFocus}
          aria-describedby={
            deleteError
              ? "delete-client-api-key-description delete-client-api-key-error"
              : "delete-client-api-key-description"
          }
        >
          <DialogHeader>
            <DialogTitle className="break-words">
              Delete {deleteTarget ? displayName(deleteTarget) : "API key"}?
            </DialogTitle>
            <DialogDescription id="delete-client-api-key-description">
              {deletingLastEnabledKey
                ? "This is the last saved key. Deleting it will also turn OpenAI endpoint authentication off."
                : "Requests using this key will be rejected immediately. Other saved keys and the current authentication setting will not change."}
            </DialogDescription>
          </DialogHeader>
          {deleteError && (
            <div
              id="delete-client-api-key-error"
              className="flex items-start gap-2 rounded-md border border-destructive-text/25 px-3 py-2.5 text-xs text-destructive-text"
              role="alert"
            >
              <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              <span>{deleteError}</span>
            </div>
          )}
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline" disabled={deleting}>
                Cancel
              </Button>
            </DialogClose>
            <Button
              type="button"
              variant="destructive"
              className="gap-2"
              onClick={() => void handleDelete()}
              disabled={deleting}
            >
              {deleting && (
                <Loader2 className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
              )}
              Delete key
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
