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

## Asking permission to act on the computer: what, why, the commands to check

# Shown before every action on the person's computer; `permission-check` only when there are commands.
permission-title = Vibefork: action on your computer
permission-from = Request from { $origin }.
permission-explained = Here is what will be done and why.
permission-check = You can check each command at an independent source before you answer.
permission-what = What
permission-why = Why
permission-commands = Commands
permission-verify = Verify
permission-build-what = Build the project { $project }
permission-build-why = To check that the project builds after the changes. Its build scripts run on your computer.
permission-run-what = Start the program of the project { $project }
permission-run-why = You asked to run the built program. It runs on your computer.
permission-rollback-what = Roll the project { $project } back to { $target }
permission-rollback-why = You chose to return the project's files to this point. Later changes are dropped.
permission-rollback-part-what = Put the part “{ $part }” of the project { $project } back as it was
permission-rollback-part-why = You chose to return this part to how it was before the last task changed it. Other parts are kept.
permission-save-what = Save the changes of the project { $project } to your copy on GitHub
permission-save-why = You chose to save your work. Your copy on GitHub may be public.
permission-ai-subscription-what = Set up { $provider } (the official app) and sign in in your browser
permission-ai-subscription-why = So the agent can do your tasks through your { $provider } subscription.
permission-ai-key-what = Connect { $provider } with an API key
permission-ai-key-why = So the agent can do your tasks through { $provider }.
permission-ai-user-home-what = Use your { $provider } login from { $home }
permission-ai-user-home-why = So the agent works through your subscription without an API key. Its sessions will appear in your { $provider } history.
permission-custom-ai-what = Connect your own AI server { $endpoint }
permission-custom-ai-why = The agent will send the project's code and your tasks to this address.
permission-tools-what = Install the tools the project { $project } needs: { $tools }
permission-tools-why = The project needs them to build and run. They go into the agent's own folder, not into the system.

## Answers in the task's conversation

# The person's message was taken by the running task.
note-taken = Got it, I'll take it into account in the next step.
thought-rating = Working out how big the task is
thought-planning = Splitting the task into steps
thought-task = Working on the task
thought-step = Step { $n } of { $of }: { $title }

## The map of the project's parts

# Why the map of parts could not be drawn; `error` is the agent's diagnostics, in English.
map-no-files = The project has no files yet, so there is nothing to split into parts
map-no-models = Connect an AI to split the project into parts
map-failed = Could not split the project into parts: { $error }

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
custom-openai-key-prompt = enter API key for this server (leave empty if your local server does not require authentication):
custom-openai-model-prompt = enter model identifier (e.g. llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Allow
btn-block = Don't allow
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
tray-agent-status = Agent status…

## Agent windows

status-window-title = Vibefork — agent status
window-close = Close
first-run-title = Welcome to Vibefork
first-run-intro = The agent works on this computer: it prepares the tools a project needs, runs your AI model and builds the result.
first-run-folder = Folder for tools
first-run-folder-hint = Toolchains of your projects go here. Pick a disk with room to spare.
# $free: free room on the folder's disk, in GB (one decimal).
first-run-space-free = { $free } GB free on this disk
# $needed: what every tool not installed yet takes, in GB (one decimal); $free as above.
first-run-space = Up to { $needed } GB for all tools · { $free } GB free
first-run-tools = Tools the agent installs when a project needs them
first-run-tool-installed = installed
# $size: a size in MB (a whole number) or GB (one decimal).
size-mb = { $size } MB
size-gb = { $size } GB
first-run-connect-ai = Connect an AI model
first-run-connect-ai-hint = Your subscription (Claude Code, Codex, GitHub Copilot) or an API key — in the editor.
first-run-start = Continue
first-run-later = Later
# $error: why the folder could not be used, in plain words.
first-run-folder-failed = The tools folder stays where it was: { $error }

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

## Installer (install.ps1, install.sh)
# Single-line messages with plain `{ $name }` variables only: the installer scripts are rendered from
# them and cannot run Fluent. $platform is `<system>-<processor>`, such as `linux-x86_64`.

install-downloading = Downloading Vibefork { $version }…
install-installing = Installing…
install-done = Vibefork { $version } is installed: { $path }
install-done-check = To check it, open a new terminal and run: vibefork doctor
install-failed-system = Vibefork can't be installed here: this system ({ $system }) is not supported. Vibefork works on Windows, macOS and Linux.
install-failed-processor = Vibefork can't be installed on this computer: its processor ({ $cpu }) is not supported.
install-failed-no-build = Vibefork { $version } is not available for this computer ({ $platform }) yet. Try again later.
install-failed-download = Couldn't download { $asset }. Check your internet connection and try again.
install-failed-hash = The downloaded file { $asset } is not the expected one: it may be damaged or replaced. Nothing was installed. Try again later.
install-failed-install = Vibefork could not be installed. Run the install command again.
install-failed-start = Vibefork is installed, but the agent did not start. Start it with: vibefork start
