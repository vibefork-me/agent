# Strings of the agent's own windows, tray and command line, in English.
# English is the reference: every other language falls back to it, message by message.
# Syntax: https://projectfluent.org/fluent/guide/ — variables are `{ $name }`.

## Pairing and project access dialogs

pair-title = Vibefork: connect a browser?
pair-text =
    Site: { $origin }
    Browser: { $name }
    Code: { $code }

    Allow only if the same code is shown on the page.
project-access-title = Vibefork: give access to a project?
project-access-text =
    { $origin } ({ $name }) asks to work in the project:

    { $project }

    Allow only if you are doing this in your browser right now.

## Confirming an action

confirm-action-title = Vibefork: confirm an action
confirm-action-text =
    { $origin } asks to:

    { $action }
# The same with the object of the action on its own line.
confirm-action-text-target =
    { $origin } asks to:

    { $action }
    Target: { $target }
action-build = build the project (runs its build scripts)
action-run = run the project's program
action-rollback = roll the project back (later changes are dropped)
action-rollback-part = return this part of the project to how it was before the last task changed it (other parts are kept)
action-save = save the project's changes to your copy on GitHub (it may be public)
action-connect-ai-subscription = set up { $provider } (official app) and sign in in your browser
action-connect-ai-key = connect { $provider } with an API key
action-connect-ai-user-home = Allow Vibefork to use your { $provider } login? Agent sessions will appear in your { $provider } history.
action-disconnect-ai = disconnect { $provider }

## Answers in the task's conversation

# The person's message was taken by the running task.
note-taken = Got it, I'll take it into account in the next step.
thought-rating = Working out how big the task is
thought-planning = Splitting the task into steps
thought-task = Working on the task
thought-step = Step { $n } of { $of }: { $title }

## Entering a key or a value

ask-secret-title = Vibefork: enter secret
ask-secret-text =
    { $origin } asks for:

    { $prompt }
openrouter-key-prompt = enter OpenRouter API key (get one at openrouter.ai)
gemini-key-prompt =
    enter Google AI Studio (Gemini) API key (get one at aistudio.google.com):

    Note: Google free tier terms allow using inputs to train models.
ask-text-title = Vibefork: enter value
ask-text-text =
    { $origin } asks for:

    { $prompt }
custom-openai-endpoint-prompt = enter OpenAI-compatible API base URL (e.g. http://localhost:11434/v1):
custom-openai-confirm-text =
    { $origin } asks to connect a custom OpenAI-compatible API:

    { $endpoint }

    Project code and requests will be sent to this address.

    Allow connecting to this server?
custom-openai-key-prompt = enter API key for this server (leave empty if your local server does not require authentication):
custom-openai-model-prompt = enter model identifier (e.g. llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Allow
btn-block = Block
btn-ok = OK
btn-cancel = Cancel

## Tray

tray-checking = Vibefork: checking…
tray-install-update = Install update
tray-install-update-version = Install update { $update }
tray-automatic-updates = Automatic updates
# $enabled is `true` or `false`: whether automatic updates are on now.
tray-auto-updates-toggle =
    { $enabled ->
        [true] Turn automatic updates off
       *[false] Turn automatic updates on
    }
tray-open-vibefork = Open Vibefork
tray-stop-agent = Stop the agent
tray-start-agent = Start the agent
tray-quit = Quit tray
tray-status-running = Vibefork { $version }: running on port { $port }
tray-status-running-update = Vibefork { $version }: running on port { $port } · update { $update } available
tray-status-stopped = Vibefork: stopped
tray-status-stopped-update = Vibefork: stopped · update { $update } available

tray-stage-download = downloading
tray-stage-parts = looking at the parts
tray-stage-links = linking the parts
tray-stage-tools = preparing tools
tray-stage-summary = summing up
tray-stage-understand = understanding the task
tray-stage-plan = planning
tray-stage-change = changing
tray-stage-check = checking
tray-stage-done = finishing
tray-activity = “{ $project }”
tray-activity-stage = “{ $project }” — { $stage }
# $line is tray-activity or tray-activity-stage; $minutes is rounded up.
tray-activity-eta = { $line }, ≈ { $minutes } min left
tray-waiting = waiting
tray-needs-you = “{ $project }” — your decision is needed
tray-failed = “{ $project }” — the build failed
tray-recent-projects = Recent projects
tray-open-log = Open the log
notify-done = Done: { $project }
notify-needs-you = The agent needs your decision
notify-build-failed = The build failed

## Command line

cli-status-running = running · version { $version } · port { $port } · pid { $pid }
cli-status-not-running = not running (port { $port })
cli-status-auto-update =
    automatic updates: { $enabled ->
        [true] on
       *[false] off
    }
cli-status-stopped = stopped
# Appended to the status line, hence the leading space.
cli-status-update-notice = { " " }· update { $update } available (run `vibefork update` to install)
cli-status-update-available = update { $version } is available — `vibefork update`
cli-github-connected = connected
cli-github-connected-login = connected as { $login }
cli-github-not-connected = not connected — `vibefork github connect`
cli-github-disconnected = disconnected (the app stays installed on your copies; remove it in GitHub settings if you want)
cli-storage-header-files = files
cli-storage-unknown-folder = vibefork: unknown folder `{ $kind }` (projects, tools, caches, artifacts)
cli-storage-stop-agent-first = vibefork: stop the agent first (it is listening on port { $port })
cli-storage-moved =
    moved { $kind } ({ $size }, { $files ->
        [one] { $files } file
       *[other] { $files } files
    }) to { $path }
