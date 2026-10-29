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

## Asking permission to act on the computer: what, why, the commands to check

permission-title = Vibefork: Aktion auf deinem Computer
permission-from = Anfrage von { $origin }.
permission-explained = Hier steht, was passiert und warum.
permission-check = Du kannst jeden Befehl vor deiner Antwort bei einer unabhängigen Quelle prüfen.
permission-what = Was
permission-why = Warum
permission-commands = Befehle
permission-verify = Prüfen
permission-build-what = Das Projekt { $project } bauen
permission-build-why = Um zu prüfen, ob sich das Projekt nach den Änderungen bauen lässt. Seine Build-Skripte laufen auf deinem Computer.
permission-run-what = Das Programm des Projekts { $project } starten
permission-run-why = Du hast den Start des gebauten Programms angefordert. Es läuft auf deinem Computer.
permission-rollback-what = Das Projekt { $project } auf { $target } zurücksetzen
permission-rollback-why = Du hast gewählt, die Dateien des Projekts auf diesen Stand zurückzusetzen. Spätere Änderungen werden verworfen.
permission-rollback-part-what = Den Teil „{ $part }“ des Projekts { $project } wie vorher wiederherstellen
permission-rollback-part-why = Du hast gewählt, diesen Teil so wiederherzustellen, wie er vor der letzten Aufgabe war, die ihn geändert hat. Die anderen Teile bleiben, wie sie sind.
permission-save-what = Die Änderungen des Projekts { $project } in deiner Kopie auf GitHub speichern
permission-save-why = Du hast gewählt, deine Arbeit zu speichern. Deine Kopie auf GitHub kann öffentlich sein.
permission-ai-subscription-what = { $provider } (die offizielle App) einrichten und dich im Browser anmelden
permission-ai-subscription-why = Damit der Agent deine Aufgaben über dein { $provider }-Abonnement erledigt.
permission-ai-key-what = { $provider } mit einem API-Schlüssel verbinden
permission-ai-key-why = Damit der Agent deine Aufgaben über { $provider } erledigt.
permission-ai-user-home-what = Deine Anmeldung bei { $provider } aus { $home } nutzen
permission-ai-user-home-why = Damit der Agent dein Abonnement ohne API-Schlüssel nutzt. Die Sitzungen des Agenten erscheinen in deinem { $provider }-Verlauf.
permission-custom-ai-what = Deinen eigenen KI-Server { $endpoint } verbinden
permission-custom-ai-why = Der Agent sendet den Code des Projekts und deine Aufgaben an diese Adresse.
permission-tools-what = Die Werkzeuge installieren, die das Projekt { $project } braucht: { $tools }
permission-tools-why = Das Projekt braucht sie zum Bauen und Ausführen. Sie kommen in den eigenen Ordner des Agenten, nicht ins System.

## Answers in the task's conversation

note-taken = Verstanden, ich berücksichtige das im nächsten Schritt.
thought-rating = Ich schätze ab, wie groß die Aufgabe ist
thought-planning = Ich teile die Aufgabe in Schritte auf
thought-task = Ich arbeite an der Aufgabe
thought-step = Schritt { $n } von { $of }: { $title }
thought-explain = Ich sehe mir deine Frage an
thought-explain-part = Ich sehe mir den Teil „{ $part }“ an

## The map of the project's parts

# Why the map of parts could not be drawn; `error` is the agent's diagnostics, in English.
map-no-files = Das Projekt hat noch keine Dateien, es gibt nichts in Teile zu gliedern
map-no-models = Verbinde eine KI, um das Projekt in Teile zu gliedern
map-failed = Das Projekt konnte nicht in Teile gegliedert werden: { $error }

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
custom-openai-key-prompt = gib den API-Schlüssel für diesen Server ein (leer lassen, wenn dein lokaler Server keine Anmeldung verlangt):
custom-openai-model-prompt = gib die Kennung des Modells ein (z. B. llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Erlauben
btn-block = Nicht erlauben
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

tray-stage-download = wird heruntergeladen
tray-stage-parts = Teile werden angesehen
tray-stage-links = Teile werden verknüpft
tray-stage-tools = Werkzeuge werden vorbereitet
tray-stage-summary = wird zusammengefasst
tray-stage-understand = Aufgabe wird verstanden
tray-stage-plan = wird geplant
tray-stage-change = wird geändert
tray-stage-check = wird geprüft
tray-stage-done = wird abgeschlossen
tray-activity = „{ $project }“
tray-activity-stage = „{ $project }“ — { $stage }
tray-activity-eta = { $line }, noch ≈ { $minutes } Min.
tray-waiting = wartet
tray-needs-you = „{ $project }“ — deine Entscheidung wird gebraucht
tray-failed = „{ $project }“ — der Build ist fehlgeschlagen
tray-recent-projects = Letzte Projekte
tray-open-log = Protokoll öffnen
notify-done = Fertig: { $project }
notify-needs-you = Der Agent braucht deine Entscheidung
notify-build-failed = Der Build ist fehlgeschlagen
tray-agent-status = Agent-Status…

## Agent windows

status-window-title = Vibefork — Agent-Status
window-close = Schließen
first-run-title = Willkommen bei Vibefork
first-run-intro = Der Agent arbeitet auf diesem Computer: Er bereitet die Werkzeuge vor, die ein Projekt braucht, führt dein KI-Modell aus und baut das Ergebnis.
first-run-folder = Ordner für Werkzeuge
first-run-folder-hint = Hier landen die Werkzeuge deiner Projekte. Wähle ein Laufwerk mit genug Platz.
first-run-space-free = { $free } GB frei auf diesem Laufwerk
first-run-space = Bis zu { $needed } GB für alle Werkzeuge · { $free } GB frei
first-run-tools = Werkzeuge, die der Agent installiert, wenn ein Projekt sie braucht
first-run-tool-installed = installiert
size-mb = { $size } MB
size-gb = { $size } GB
first-run-connect-ai = KI-Modell verbinden
first-run-connect-ai-hint = Dein Abo (Claude Code, Codex, GitHub Copilot) oder ein API-Schlüssel — im Editor.
first-run-start = Weiter
first-run-later = Später
first-run-folder-failed = Der Werkzeugordner bleibt, wo er war: { $error }

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

## Installer (install.ps1, install.sh)

install-downloading = Vibefork { $version } wird heruntergeladen…
install-installing = Wird installiert…
install-done = Vibefork { $version } ist installiert: { $path }
install-done-check = Zum Prüfen öffne ein neues Terminal und führe aus: vibefork doctor
install-failed-system = Vibefork kann hier nicht installiert werden: Dieses System ({ $system }) wird nicht unterstützt. Vibefork läuft unter Windows, macOS und Linux.
install-failed-processor = Vibefork kann auf diesem Computer nicht installiert werden: Sein Prozessor ({ $cpu }) wird nicht unterstützt.
install-failed-no-build = Vibefork { $version } ist für diesen Computer ({ $platform }) noch nicht verfügbar. Versuch es später noch einmal.
install-failed-download = { $asset } konnte nicht heruntergeladen werden. Prüfe deine Internetverbindung und versuch es noch einmal.
install-failed-hash = Die heruntergeladene Datei { $asset } ist nicht die erwartete: Sie könnte beschädigt oder ausgetauscht worden sein. Es wurde nichts installiert. Versuch es später noch einmal.
install-failed-install = Vibefork konnte nicht installiert werden. Führe den Installationsbefehl noch einmal aus.
install-failed-start = Vibefork ist installiert, aber der Agent ist nicht gestartet. Starte ihn mit: vibefork start

## Windows installer wizard (scripts/package.py)

msi-setup = Vibefork-Setup
msi-newer-installed = Eine neuere Version von Vibefork ist bereits installiert.
msi-feature-desktop = Desktopverknüpfung
msi-feature-autostart = Beim Anmelden starten
msi-start = Vibefork starten
msi-welcome-title = Willkommen beim Vibefork-Setup
msi-welcome-text = Vibefork wird auf diesem Computer nur für dich installiert: Administratorrechte sind nicht nötig. Klicke auf „Weiter“, um fortzufahren, oder auf „Abbrechen“, um das Setup zu beenden.
msi-welcome-update-text = Vibefork wird auf diesem Computer aktualisiert. Klicke auf „Weiter“, um fortzufahren, oder auf „Abbrechen“, um das Setup zu beenden.
msi-maintenance-text = Du kannst Vibefork reparieren oder vom Computer entfernen. Klicke auf „Weiter“, um fortzufahren, oder auf „Abbrechen“, um das Setup zu beenden.
msi-prepare-text = Bitte warte, das Setup wird vorbereitet.
msi-folder-title = Zielordner
msi-folder-text = Klicke auf „Weiter“, um in diesen Ordner zu installieren, oder auf „Ändern“, um einen anderen zu wählen.
msi-folder-label = Vibefork installieren in:
msi-folder-browse-title = Zielordner ändern
msi-folder-browse-text = Wähle den Ordner für Vibefork.
msi-folder-invalid = Der Ordner muss auf einem lokalen Laufwerk dieses Computers liegen.
msi-shortcuts-title = Verknüpfungen
msi-shortcuts-text = Wähle, wie du Vibefork startest. Im Startmenü ist es immer.
msi-shortcut-desktop = Desktopverknüpfung erstellen
msi-shortcut-sign-in = Vibefork starten, wenn ich mich bei Windows anmelde
msi-ready-install-title = Bereit zur Installation von Vibefork
msi-ready-install-text = Klicke auf „Installieren“, um zu beginnen. Mit „Zurück“ prüfst du deine Auswahl, mit „Abbrechen“ beendest du das Setup.
msi-ready-change-title = Bereit zum Ändern von Vibefork
msi-ready-change-text = Klicke auf „Ändern“, um zu beginnen. Mit „Zurück“ prüfst du deine Auswahl, mit „Abbrechen“ beendest du das Setup.
msi-ready-repair-title = Bereit zur Reparatur von Vibefork
msi-ready-repair-text = Klicke auf „Reparieren“, um fehlende oder beschädigte Dateien, Verknüpfungen und Einstellungen von Vibefork wiederherzustellen. Mit „Zurück“ prüfst du deine Auswahl, mit „Abbrechen“ beendest du das Setup.
msi-ready-remove-title = Bereit zum Entfernen von Vibefork
msi-ready-remove-text = Klicke auf „Entfernen“, um Vibefork vom Computer zu entfernen. Deine Daten und Projekte bleiben erhalten. Mit „Zurück“ prüfst du deine Auswahl, mit „Abbrechen“ beendest du das Setup.
msi-ready-update-title = Bereit zur Aktualisierung von Vibefork
msi-ready-update-text = Klicke auf „Aktualisieren“, um Vibefork zu aktualisieren. Mit „Zurück“ prüfst du deine Auswahl, mit „Abbrechen“ beendest du das Setup.
msi-progress-installing-title = Vibefork wird installiert
msi-progress-installing-text = Bitte warte, während Vibefork installiert wird.
msi-progress-changing-title = Vibefork wird geändert
msi-progress-changing-text = Bitte warte, während Vibefork geändert wird.
msi-progress-repairing-title = Vibefork wird repariert
msi-progress-repairing-text = Bitte warte, während Vibefork repariert wird.
msi-progress-removing-title = Vibefork wird entfernt
msi-progress-removing-text = Bitte warte, während Vibefork entfernt wird.
msi-progress-updating-title = Vibefork wird aktualisiert
msi-progress-updating-text = Bitte warte, während Vibefork aktualisiert wird.
msi-maintenance-type-title = Vibefork ändern, reparieren oder entfernen
msi-maintenance-type-text = Wähle, was du tun möchtest.
msi-maintenance-change-text = Ändert die Installationsoptionen.
msi-maintenance-change-disabled = Vibefork hat keine Optionen, die sich ändern lassen.
msi-maintenance-repair-text = Stellt fehlende oder beschädigte Dateien, Verknüpfungen und Einstellungen wieder her.
msi-maintenance-repair-disabled = Vibefork kann nicht repariert werden.
msi-maintenance-remove-text = Entfernt Vibefork vom Computer. Deine Daten und Projekte bleiben erhalten.
msi-maintenance-remove-disabled = Vibefork kann nicht entfernt werden.
msi-resume-title = Vibefork-Setup fortsetzen
msi-resume-text = Das Setup schließt die Installation von Vibefork auf diesem Computer ab. Klicke auf „Installieren“, um fortzufahren, oder auf „Abbrechen“, um das Setup zu beenden.
msi-exit-title = Fertig
msi-exit-text = Klicke auf „Fertig stellen“, um das Setup zu schließen.
msi-user-exit-title = Das Setup wurde unterbrochen
msi-user-exit-text = Auf dem Computer hat sich nichts geändert. Um Vibefork zu installieren, starte das Setup noch einmal.
msi-fatal-title = Das Setup wurde wegen eines Fehlers beendet
msi-fatal-text = Auf dem Computer hat sich nichts geändert. Um Vibefork zu installieren, starte das Setup später noch einmal.
msi-cancel-question = Vibefork-Setup abbrechen?
msi-files-in-use-title = Dateien in Verwendung
msi-files-in-use-description = Einige Dateien, die aktualisiert werden müssen, sind in Verwendung.
msi-files-in-use-text = Diese Programme verwenden Dateien, die aktualisiert werden müssen. Schließe sie und klicke auf „Wiederholen“, um fortzufahren, oder auf „Beenden“.
msi-files-in-use-close-text = Diese Programme verwenden Dateien, die aktualisiert werden müssen. Das Setup kann sie schließen und neu starten, oder du startest den Computer später neu.
msi-files-in-use-close = Programme &schließen und neu starten
msi-files-in-use-keep = Programme &nicht schließen (der Computer muss neu gestartet werden)
msi-disk-title = Nicht genug Speicherplatz
msi-disk-description = Das Setup braucht mehr Speicherplatz, als frei ist.
msi-disk-cost-title = Speicherplatz
msi-disk-cost-description = Der Speicherplatz, den Vibefork für die Installation braucht.
msi-disk-text = Auf den markierten Laufwerken ist nicht genug Platz frei. Gib dort Platz frei oder wähle ein anderes Laufwerk.
msi-disk-rollback = Du kannst auch das Zurücksetzen bei einem Abbruch ausschalten: Dann kann das Setup den Computer nicht wiederherstellen, wenn die Installation unterbrochen wird. Klicke auf „Ja“, um dieses Risiko einzugehen.
msi-costing-text = Bitte warte, der Speicherplatz wird geprüft.
