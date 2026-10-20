// Generated from protocol.schema.json by vf-server/tests/schema.rs. Do not edit.

/** `POST /v1/action` — an action that needs the native confirm dialog. */
export type ActionRequest = { action: string; target?: string | null };

/** Response of `POST /v1/action`. */
export type ActionResponse = { action: string; approved: boolean };

/** `POST /v1/auth/challenge`. */
export type ChallengeRequest = { keyId: string };

/** Response of `POST /v1/auth/challenge`: a one-time nonce, valid for 60 s. */
export type ChallengeResponse = { nonce: string };

/** Messages from the browser. */
export type ClientMessage = { token: string; type: "auth" } | { continue?: boolean; model?: string | null; project: string; prompt: string; provider?: string | null; type: "task" } | { project: string; type: "build" } | { type: "cancel" };

/** Cost class of a model. */
export type CostClass = "subscription" | "free" | "paid";

/** Response of `POST /v1/echo` (connectivity check). */
export type EchoResponse = { echo: string };

/** An error body: `{ "error": "…" }`. */
export type ErrorBody = { error: string };

/** A normalized event, tagged by `kind`. */
export type Event = { cli: string; kind: "init"; mcp?: number | null; model?: string | null; session?: string | null; skills?: number | null } | { kind: "text"; text: string } | { detail: string; kind: "tool"; tool: string } | { error: boolean; kind: "tool_result"; text: string } | { costUsd?: number | null; kind: "result"; ms?: number | null; ok: boolean; text: string; turns?: number | null; usage?: Usage | null } | { kind: "error"; text: string };

/** Response of `POST /v1/github/connect`: the code the user enters on GitHub. */
export type GithubConnectResponse = { expiresIn: number; userCode: string; verificationUri: string };

/** Response of `GET /v1/github/status`. */
export type GithubStatusResponse = { connected: boolean; error?: string | null; login?: string | null; pending: boolean };

/** `GET /v1/hello` — the only call that needs neither a session nor an allowed origin. */
export type Hello = { agent: string; platform: string; version: string };

/** Request of `POST /v1/projects/import`: a project from GitHub, without git on the machine. */
export type ImportRequest = { repo: string };

/** Response of `POST /v1/projects/import`. */
export type ImportResponse = { commit: string; project: string; upstream: string };

/** One journal entry: a task or a plan step. */
export type JournalEntry = { at: string; commit?: string | null; commits?: (string)[]; costUsd?: number | null; files?: string; handoff?: boolean; model: string; ms?: number | null; prompt: string; provider: string; session?: string | null; summary?: string; usage?: Usage | null };

/** Response of `GET /v1/journal?project=…`. */
export type JournalResponse = { entries: (JournalEntry)[] };

/** Response of `GET /v1/me`. */
export type Me = { keyId: string; name: string; origin: string };

/** A model available on this machine. */
export type ModelInfo = { cost: CostClass; label: string; model: string; provider: string; tier: number };

/** Response of `GET /v1/models`. */
export type ModelsResponse = { models: (ModelInfo)[] };

/** One paired browser in `GET /v1/pairs`. */
export type PairInfo = { created: number; keyId: string; name: string; origin: string; projects?: (string)[] };

/** `POST /v1/pair/start`. */
export type PairStartRequest = { name?: string | null; publicKeyJwk: PublicJwk };

/** Response of `POST /v1/pair/start`. */
export type PairStartResponse = { code: string; pairId: string };

/** State of a pairing request. */
export type PairState = "pending" | "approved" | "denied";

/** Response of `GET /v1/pair/status`. */
export type PairStatusResponse = { keyId?: string | null; status: PairState };

/** A plan step as shown to the user. */
export type PlanStepInfo = { complexity: string; label: string; title: string };

/** Response of `GET /v1/projects`. */
export type ProjectsResponse = { projects: (string)[]; root: string };

/** A browser's public key (JWK, EC P-256). A private part (`d`) is rejected. */
export type PublicJwk = { crv: string; d?: string | null; kty: string; x: string; y: string };

/** Rating of a task by the Auto mode. */
export type Rating = { classifier: string; complexity: string; decompose: boolean; reason: string };

/** `POST /v1/pairs/revoke`. */
export type RevokeRequest = { keyId: string };

/** Response of `POST /v1/pairs/revoke`. */
export type RevokeResponse = { revoked: string };

/** Request of `POST /v1/rollback`: reset the project to a checkpoint of its journal. */
export type RollbackRequest = { commit: string; project: string };

/** Response of `POST /v1/rollback`. */
export type RollbackResponse = { commit: string; project: string };

/** Request of `POST /v1/save`. */
export type SaveRequest = { copy?: string | null; project: string };

/** Response of `POST /v1/save`. */
export type SaveResponse = { commit?: string | null; copy?: string | null; files?: (string)[]; name?: string | null; next_url?: string | null; step: SaveStep; url?: string | null };

/** What "Save" did or what the user does next. */
export type SaveStep = "saved" | "unchanged" | "connect_github" | "create_copy" | "create_repo" | "allow_app" | "narrow_access" | "conflict";

/** Messages from the agent. */
export type ServerMessage = { keyId: string; type: "authed" } | { data: string; type: "echo" } | { choice: ModelInfo; rating: Rating; type: "routing" } | { rating?: Rating | null; type: "planning" } | { costUsd?: number | null; planner: string; steps: (PlanStepInfo)[]; type: "plan" } | { error?: string | null; path?: string | null; seconds?: string | null; status: ToolchainStatus; tool: string; type: "toolchain" } | { handoff: boolean; isolation?: string; model: string; project: string; provider: string; resume: boolean; step?: StepInfo | null; toolchain: (string)[]; type: "started" } | { ev: Event; provider: string; type: "evt" } | { text: string; type: "stderr" } | { commit?: string | null; n: number; of: number; ok: boolean; type: "step-done" } | { type: "cancelled" } | { code: number; commit?: string | null; diff: string; model?: string | null; plan?: boolean; provider?: string | null; sessionId?: string | null; status: string; type: "done" } | { text: string; type: "estimate"; verdict: string } | { text: string; type: "build_log" } | { artifacts?: string | null; code: number; files: (string)[]; seconds: number; type: "build_done" } | { error: string; type: "error" };

/** `POST /v1/auth/session`. */
export type SessionRequest = { keyId: string; nonce: string; signature: string };

/** Response of `POST /v1/auth/session`. */
export type SessionResponse = { expiresIn: number; token: string };

/** Position of a step in a plan. */
export type StepInfo = { n: number; of: number; title: string };

/** Toolchain preparation progress. */
export type ToolchainStatus = "installing" | "ready" | "failed";

/** Token usage reported by a provider. */
export type Usage = { input_tokens: number; output_tokens: number };

