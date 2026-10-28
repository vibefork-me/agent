# Strings of the agent's own windows, tray and command line, in Spanish.
# Same messages as en.ftl; a message missing here is shown in English.
# The person is addressed as "tú"; product terms follow the editor's glossary (src/i18n/glossary.json).

## Pairing and project access dialogs

pair-title = Vibefork: ¿conectar un navegador?
pair-text =
    Sitio: { $origin }
    Navegador: { $name }
    Código: { $code }

    Permítelo solo si en la página aparece el mismo código.
project-access-title = Vibefork: ¿dar acceso a un proyecto?
project-access-text =
    { $origin } ({ $name }) pide trabajar en el proyecto:

    { $project }

    Permítelo solo si lo estás haciendo ahora mismo en tu navegador.

## Confirming an action

confirm-action-title = Vibefork: confirma una acción
confirm-action-text =
    { $origin } pide:

    { $action }
confirm-action-text-target =
    { $origin } pide:

    { $action }
    Destino: { $target }
action-build = compilar el proyecto (ejecuta sus scripts de compilación)
action-run = ejecutar el programa del proyecto
action-rollback = volver el proyecto a una versión anterior (los cambios posteriores se descartan)
action-rollback-part = dejar esta parte del proyecto como estaba antes de la última tarea que la cambió (las demás partes no cambian)
action-save = guardar los cambios del proyecto en tu copia de GitHub (puede ser pública)
action-connect-ai-subscription = configurar { $provider } (app oficial) e iniciar sesión en el navegador
action-connect-ai-key = conectar { $provider } con una clave de API
action-connect-ai-user-home = ¿Permitir que Vibefork use tu sesión de { $provider }? Las sesiones del agente aparecerán en tu historial de { $provider }.
action-disconnect-ai = desconectar { $provider }

## Answers in the task's conversation

note-taken = Entendido, lo tendré en cuenta en el próximo paso.
thought-rating = Calculando el tamaño de la tarea
thought-planning = Dividiendo la tarea en pasos
thought-task = Trabajando en la tarea
thought-step = Paso { $n } de { $of }: { $title }

## Entering a key or a value

ask-secret-title = Vibefork: introduce la clave
ask-secret-text =
    { $origin } pide:

    { $prompt }
openrouter-key-prompt = introduce la clave de API de OpenRouter (consíguela en openrouter.ai)
gemini-key-prompt =
    introduce la clave de API de Google AI Studio (Gemini) (consíguela en aistudio.google.com):

    Aviso: según las condiciones del plan gratuito, Google puede usar las solicitudes para entrenar modelos.
ask-text-title = Vibefork: introduce un valor
ask-text-text =
    { $origin } pide:

    { $prompt }
custom-openai-endpoint-prompt = introduce la dirección base de la API compatible con OpenAI (p. ej., http://localhost:11434/v1):
custom-openai-confirm-text =
    { $origin } pide conectar una API propia compatible con OpenAI:

    { $endpoint }

    El código del proyecto y las solicitudes se enviarán a esta dirección.

    ¿Permitir la conexión con este servidor?
custom-openai-key-prompt = introduce la clave de API de este servidor (déjala vacía si tu servidor local no pide autenticación):
custom-openai-model-prompt = introduce el identificador del modelo (p. ej., llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Permitir
btn-block = Bloquear
btn-ok = Aceptar
btn-cancel = Cancelar

## Tray

tray-checking = Vibefork: comprobando…
tray-install-update = Instalar actualización
tray-install-update-version = Instalar actualización { $update }
tray-automatic-updates = Actualizaciones automáticas
tray-auto-updates-toggle =
    { $enabled ->
        [true] Desactivar actualizaciones automáticas
       *[false] Activar actualizaciones automáticas
    }
tray-open-vibefork = Abrir Vibefork
tray-stop-agent = Detener el agente
tray-start-agent = Iniciar el agente
tray-quit = Cerrar la bandeja
tray-status-running = Vibefork { $version }: en marcha en el puerto { $port }
tray-status-running-update = Vibefork { $version }: en marcha en el puerto { $port } · actualización { $update } disponible
tray-status-stopped = Vibefork: detenido
tray-status-stopped-update = Vibefork: detenido · actualización { $update } disponible

tray-stage-download = descargando
tray-stage-parts = revisando las partes
tray-stage-links = conectando las partes
tray-stage-tools = preparando las herramientas
tray-stage-summary = resumiendo
tray-stage-understand = entendiendo la tarea
tray-stage-plan = planificando
tray-stage-change = haciendo cambios
tray-stage-check = comprobando
tray-stage-done = terminando
tray-activity = «{ $project }»
tray-activity-stage = «{ $project }» — { $stage }
tray-activity-eta = { $line }, quedan ≈ { $minutes } min
tray-waiting = esperando
tray-needs-you = «{ $project }» — se necesita tu decisión
tray-failed = «{ $project }» — la compilación falló
tray-recent-projects = Proyectos recientes
tray-open-log = Abrir el registro
notify-done = Listo: { $project }
notify-needs-you = El agente necesita tu decisión
notify-build-failed = La compilación falló

## Command line

cli-status-running = en marcha · versión { $version } · puerto { $port } · pid { $pid }
cli-status-not-running = no está en marcha (puerto { $port })
cli-status-auto-update =
    actualizaciones automáticas: { $enabled ->
        [true] activadas
       *[false] desactivadas
    }
cli-status-stopped = detenido
cli-status-update-notice = { " " }· actualización { $update } disponible (ejecuta `vibefork update` para instalarla)
cli-status-update-available = la actualización { $version } está disponible — `vibefork update`
cli-github-connected = conectado
cli-github-connected-login = conectado como { $login }
cli-github-not-connected = no conectado — `vibefork github connect`
cli-github-disconnected = desconectado (la app sigue instalada en tus copias; quítala en la configuración de GitHub si quieres)
cli-storage-header-files = archivos
cli-storage-unknown-folder = vibefork: carpeta desconocida `{ $kind }` (projects, tools, caches, artifacts)
cli-storage-stop-agent-first = vibefork: primero detén el agente (está escuchando en el puerto { $port })
cli-storage-moved =
    { $kind } movido ({ $size }, { $files ->
        [one] { $files } archivo
       *[other] { $files } archivos
    }) a { $path }
