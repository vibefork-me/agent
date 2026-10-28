# Strings of the agent's own windows, tray and command line, in Latin American Spanish: neutral Latin
# American usage ("computadora", "agregar", "ingresa", the simple past).
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

ask-secret-title = Vibefork: ingresa la clave
ask-secret-text =
    { $origin } pide:

    { $prompt }
openrouter-key-prompt = ingresa la clave de API de OpenRouter (consíguela en openrouter.ai)
gemini-key-prompt =
    ingresa la clave de API de Google AI Studio (Gemini) (consíguela en aistudio.google.com):

    Aviso: según las condiciones del plan gratuito, Google puede usar las solicitudes para entrenar modelos.
ask-text-title = Vibefork: ingresa un valor
ask-text-text =
    { $origin } pide:

    { $prompt }
custom-openai-endpoint-prompt = ingresa la dirección base de la API compatible con OpenAI (p. ej., http://localhost:11434/v1):
custom-openai-confirm-text =
    { $origin } pide conectar una API propia compatible con OpenAI:

    { $endpoint }

    El código del proyecto y las solicitudes se enviarán a esta dirección.

    ¿Permitir la conexión con este servidor?
custom-openai-key-prompt = ingresa la clave de API de este servidor (déjala vacía si tu servidor local no pide autenticación):
custom-openai-model-prompt = ingresa el identificador del modelo (p. ej., llama3, qwen2.5-coder):

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
tray-stage-links = enlazando las partes
tray-stage-tools = preparando herramientas
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
tray-agent-status = Estado del agente…

## Agent windows

status-window-title = Vibefork — estado del agente
window-close = Cerrar
first-run-title = Te damos la bienvenida a Vibefork
first-run-intro = El agente trabaja en esta computadora: prepara las herramientas que necesita un proyecto, ejecuta tu modelo de IA y compila el resultado.
first-run-folder = Carpeta de herramientas
first-run-folder-hint = Aquí se instalan las herramientas de tus proyectos. Elige un disco con espacio de sobra.
first-run-space-free = { $free } GB libres en este disco
first-run-space = Hasta { $needed } GB para todas las herramientas · { $free } GB libres
first-run-tools = Herramientas que el agente instala cuando un proyecto las necesita
first-run-tool-installed = instalada
size-mb = { $size } MB
size-gb = { $size } GB
first-run-connect-ai = Conectar un modelo de IA
first-run-connect-ai-hint = Tu suscripción (Claude Code, Codex) o una clave de API, en el editor.
first-run-start = Continuar
first-run-later = Más tarde
first-run-folder-failed = La carpeta de herramientas sigue donde estaba: { $error }

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

## Installer (install.ps1, install.sh)

install-downloading = Descargando Vibefork { $version }…
install-installing = Instalando…
install-done = Vibefork { $version } está instalado: { $path }
install-done-check = Para comprobarlo, abre una terminal nueva y ejecuta: vibefork doctor
install-failed-system = No se puede instalar Vibefork aquí: este sistema ({ $system }) no es compatible. Vibefork funciona en Windows, macOS y Linux.
install-failed-processor = No se puede instalar Vibefork en esta computadora: su procesador ({ $cpu }) no es compatible.
install-failed-no-build = Vibefork { $version } todavía no está disponible para esta computadora ({ $platform }). Vuelve a intentarlo más tarde.
install-failed-download = No se pudo descargar { $asset }. Revisa tu conexión a internet y vuelve a intentarlo.
install-failed-hash = El archivo descargado { $asset } no es el esperado: puede estar dañado o haber sido reemplazado. No se instaló nada. Vuelve a intentarlo más tarde.
install-failed-install = No se pudo instalar Vibefork. Vuelve a ejecutar el comando de instalación.
install-failed-start = Vibefork está instalado, pero el agente no se inició. Inícialo con: vibefork start
