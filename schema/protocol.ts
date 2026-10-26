// Generated from protocol.schema.json by vf-server/tests/schema.rs. Do not edit.

/** Request of `POST /v1/ai/connect`: sign in to a subscription through its official CLI (the agent installs a pinned official release first when the CLI is missing). */
export type AiConnectRequest = { mode?: CliHomeMode | null; provider: string };

/** Response of `POST /v1/ai/connect`: the sign-in opened in the browser; `GET /v1/ai` tells when it is done. */
export type AiConnectResponse = { started: boolean };

/** Whether a way to connect AI is set up. */
export type AiConnected = "yes" | "no" | "unknown";

/** Request of `POST /v1/ai/disconnect`. */
export type AiDisconnectRequest = { provider: string };

/** Response of `POST /v1/ai/disconnect`. */
export type AiDisconnectResponse = { ok: boolean };

/** What a way to connect AI costs the user. */
export type AiKind = "subscription" | "free" | "custom";

/** One way to connect AI. */
export type AiProvider = { connected: AiConnected; host?: string | null; id: string; installed: boolean; installing: boolean; kind: AiKind; mode?: CliHomeMode | null; userSignedIn?: boolean | null };

/** `GET /v1/ai`: whether AI is connected — the editor is not usable without it. */
export type AiResponse = { providers: (AiProvider)[]; ready: boolean };

/** `POST /v1/auth/challenge`. */
export type ChallengeRequest = { keyId: string };

/** Response of `POST /v1/auth/challenge`: a one-time nonce, valid for 60 s. */
export type ChallengeResponse = { nonce: string };

/** Where a CLI keeps its sign-in, settings, and sessions. */
export type CliHomeMode = "agent" | "user";

/** Messages from the browser. */
export type ClientMessage = { token: string; type: "auth" } | { continue?: boolean; lang?: string | null; model?: string | null; project: string; prompt: string; provider?: string | null; type: "task" } | { project: string; type: "build" } | { type: "cancel" };

/** Cost class of a model. */
export type CostClass = "subscription" | "free" | "paid";

/** Request of `POST /v1/graph/describe`: have a model draw the project's map. */
export type DescribeRequest = { lang?: string | null; project: string };

/** An error body: `{ "error": "…" }`. */
export type ErrorBody = { code?: string | null; error: string; resetMinutes?: number | null };

/** A normalized event, tagged by `kind`. */
export type Event = { cli: string; kind: "init"; mcp?: number | null; model?: string | null; session?: string | null; skills?: number | null } | { kind: "text"; text: string } | { detail: string; kind: "tool"; tool: string } | { error: boolean; kind: "tool_result"; text: string } | { costUsd?: number | null; kind: "result"; ms?: number | null; ok: boolean; text: string; turns?: number | null; usage?: Usage | null } | { kind: "error"; text: string };

/** `GET /v1/diff?project=…&path=…`: a file as the project was taken and as it is now. */
export type FileDiffResponse = { base?: string | null; binary: boolean; modified?: string | null; original?: string | null; path: string; tooLarge: boolean };

/** `GET /v1/file?project=…&path=…`: a file for the code viewer. */
export type FileResponse = { binary: boolean; content?: string | null; path: string; size: number; tooLarge: boolean };

/** What code from outside the original changes compared with it, so the user sees it before a build. Sorted by file name only; the content is not reviewed. */
export type ForeignChanges = { binaries: (string)[]; buildScripts: (string)[]; compareUrl: string; complete: boolean; dependencies: (string)[]; files: number; source: string };

/** Response of `POST /v1/github/connect`: the code the user enters on GitHub. */
export type GithubConnectResponse = { expiresIn: number; userCode: string; verificationUri: string };

/** Response of `GET /v1/github/status`. */
export type GithubStatusResponse = { connected: boolean; error?: string | null; login?: string | null; pending: boolean };

/** `from` contains `to` (folders) or uses `to` (model's map). */
export type GraphEdge = { from: string; to: string };

/** A part of a project: a folder (or the files at the root). */
export type GraphNode = { changed: number; files: number; id: string; label: string; summary?: string | null };

/** `GET /v1/graph?project=…[&view=folders]`: the map of a project — its parts by folder, and which of them changed since the project was taken. */
export type GraphResponse = { edges: (GraphEdge)[]; nodes: (GraphNode)[]; revision?: string | null; source: GraphSource };

/** Who drew a project map. */
export type GraphSource = "model" | "folders";

/** `GET /v1/hello` — answers without an Origin header (or from allowed origins); does not require a session. */
export type Hello = { agent: string; extraOriginsActive?: boolean; platform: string; version: string };

/** Request of `POST /v1/projects/import`: a project from GitHub, without git on the machine. */
export type ImportRequest = { ref?: string | null; repo: string; upstream?: string | null };

/** Response of `POST /v1/projects/import`. */
export type ImportResponse = { commit: string; foreign?: ForeignChanges | null; project: string; upstream: string };

/** One journal entry: a task or a plan step. */
export type JournalEntry = { at: string; commit?: string | null; commits?: (string)[]; costUsd?: number | null; files?: string; handoff?: boolean; model: string; ms?: number | null; prompt: string; promptVersion?: string | null; provider: string; session?: string | null; summary?: string; usage?: Usage | null };

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

/** A project and where its code comes from. */
export type ProjectInfo = { commit?: string | null; name: string; original?: string | null; source?: string | null };

/** Response of `GET /v1/projects`. */
export type ProjectsResponse = { details?: (ProjectInfo)[]; projects: (string)[]; root: string };

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
export type ServerMessage = { keyId: string; type: "authed" } | { data: string; type: "echo" } | { choice: ModelInfo; rating: Rating; type: "routing" } | { rating?: Rating | null; type: "planning" } | { costUsd?: number | null; planner: string; steps: (PlanStepInfo)[]; type: "plan" } | { error?: string | null; path?: string | null; seconds?: string | null; status: ToolchainStatus; tool: string; type: "toolchain" } | { handoff: boolean; isolation?: string; model: string; project: string; provider: string; resume: boolean; step?: StepInfo | null; toolchain: (string)[]; type: "started" } | { ev: Event; provider: string; type: "evt" } | { text: string; type: "stderr" } | { commit?: string | null; n: number; of: number; ok: boolean; type: "step-done" } | { type: "cancelled" } | { code: number; commit?: string | null; diff: string; model?: string | null; plan?: boolean; provider?: string | null; sessionId?: string | null; status: string; type: "done" } | { text: string; type: "estimate"; verdict: string } | { text: string; type: "build_log" } | { artifacts?: string | null; code: number; files: (string)[]; seconds: number; type: "build_done" } | { code?: string | null; error: string; type: "error" };

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

