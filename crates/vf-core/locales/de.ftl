# Strings of the agent's own windows, tray and command line, in German.
# Same messages as en.ftl; a message missing here is shown in English.
# The person is addressed as "du"; product terms follow the editor's glossary (src/i18n/glossary.json).

## Pairing and project access dialogs

pair-title = Vibefork: Browser verbinden?
pair-text =
    Website: { $origin }
    Browser: { $name }
    Code: { $code }

    Erlaube es nur, wenn auf der Seite derselbe Code angezeigt wird.
project-access-title = Vibefork: Zugriff auf ein Projekt geben?
project-access-text =
    { $origin } ({ $name }) möchte im Projekt arbeiten:

    { $project }

    Erlaube es nur, wenn du das gerade in deinem Browser tust.

## Confirming an action

confirm-action-title = Vibefork: Aktion bestätigen
confirm-action-text =
    { $origin } möchte:

    { $action }
confirm-action-text-target =
    { $origin } möchte:

    { $action }
    Ziel: { $target }
action-build = das Projekt bauen (führt seine Build-Skripte aus)
action-run = das Programm des Projekts starten
action-rollback = das Projekt auf eine frühere Version zurücksetzen (spätere Änderungen werden verworfen)
action-rollback-part = diesen Teil des Projekts so zurücksetzen, wie er vor der letzten Aufgabe war, die ihn geändert hat (die anderen Teile bleiben, wie sie sind)
action-save = die Änderungen des Projekts in deiner Kopie auf GitHub speichern (sie kann öffentlich sein)
action-connect-ai-subscription = { $provider } einrichten (offizielle App) und dich im Browser anmelden
action-connect-ai-key = { $provider } mit einem API-Schlüssel verbinden
action-connect-ai-user-home = Darf Vibefork deine Anmeldung bei { $provider } nutzen? Die Sitzungen des Agenten erscheinen dann in deinem { $provider }-Verlauf.
action-disconnect-ai = { $provider } trennen

## Answers in the task's conversation

note-taken = Verstanden, ich berücksichtige das im nächsten Schritt.
thought-rating = Ich schätze ab, wie groß die Aufgabe ist
thought-planning = Ich teile die Aufgabe in Schritte auf
thought-task = Ich arbeite an der Aufgabe
thought-step = Schritt { $n } von { $of }: { $title }

## Entering a key or a value

ask-secret-title = Vibefork: Schlüssel eingeben
ask-secret-text =
    { $origin } bittet um:

    { $prompt }
openrouter-key-prompt = gib den API-Schlüssel für OpenRouter ein (erhältlich auf openrouter.ai)
gemini-key-prompt =
    gib den API-Schlüssel für Google AI Studio (Gemini) ein (erhältlich auf aistudio.google.com):

    Hinweis: Laut den Bedingungen des kostenlosen Tarifs darf Google die Anfragen zum Trainieren von Modellen verwenden.
ask-text-title = Vibefork: Wert eingeben
ask-text-text =
    { $origin } bittet um:

    { $prompt }
custom-openai-endpoint-prompt = gib die Basis-URL der OpenAI-kompatiblen API ein (z. B. http://localhost:11434/v1):
custom-openai-confirm-text =
    { $origin } möchte eine eigene OpenAI-kompatible API verbinden:

    { $endpoint }

    Der Code des Projekts und die Anfragen werden an diese Adresse gesendet.

    Verbindung zu diesem Server erlauben?
custom-openai-key-prompt = gib den API-Schlüssel für diesen Server ein (leer lassen, wenn dein lokaler Server keine Anmeldung verlangt):
custom-openai-model-prompt = gib die Kennung des Modells ein (z. B. llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Erlauben
btn-block = Blockieren
btn-ok = OK
btn-cancel = Abbrechen

## Tray

tray-checking = Vibefork: wird geprüft…
tray-install-update = Update installieren
tray-install-update-version = Update { $update } installieren
tray-automatic-updates = Automatische Updates
tray-auto-updates-toggle =
    { $enabled ->
        [true] Automatische Updates ausschalten
       *[false] Automatische Updates einschalten
    }
tray-open-vibefork = Vibefork öffnen
tray-stop-agent = Agent stoppen
tray-start-agent = Agent starten
tray-quit = Tray beenden
tray-status-running = Vibefork { $version }: läuft auf Port { $port }
tray-status-running-update = Vibefork { $version }: läuft auf Port { $port } · Update { $update } verfügbar
tray-status-stopped = Vibefork: gestoppt
tray-status-stopped-update = Vibefork: gestoppt · Update { $update } verfügbar

## Command line

cli-status-running = läuft · Version { $version } · Port { $port } · PID { $pid }
cli-status-not-running = läuft nicht (Port { $port })
cli-status-auto-update =
    automatische Updates: { $enabled ->
        [true] an
       *[false] aus
    }
cli-status-stopped = gestoppt
cli-status-update-notice = { " " }· Update { $update } verfügbar (`vibefork update` installiert es)
cli-status-update-available = Update { $version } ist verfügbar — `vibefork update`
cli-github-connected = verbunden
cli-github-connected-login = verbunden als { $login }
cli-github-not-connected = nicht verbunden — `vibefork github connect`
cli-github-disconnected = getrennt (die App bleibt in deinen Kopien installiert; entferne sie bei Bedarf in den GitHub-Einstellungen)
cli-storage-header-files = Dateien
cli-storage-unknown-folder = vibefork: unbekannter Ordner `{ $kind }` (projects, tools, caches, artifacts)
cli-storage-stop-agent-first = vibefork: stoppe zuerst den Agenten (er lauscht auf Port { $port })
cli-storage-moved =
    { $kind } verschoben ({ $size }, { $files ->
        [one] { $files } Datei
       *[other] { $files } Dateien
    }) nach { $path }
