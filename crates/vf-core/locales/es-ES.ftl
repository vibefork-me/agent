# Strings of the agent's own windows, tray and command line, in the Spanish of Spain ("ordenador",
# "añadir", "introduce", the present perfect for what has just happened).
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

## Asking permission to act on the computer: what, why, the commands to check

permission-title = Vibefork: acción en tu ordenador
permission-from = Solicitud de { $origin }.
permission-explained = Esto es lo que se hará y por qué.
permission-check = Puedes comprobar cada comando en una fuente independiente antes de responder.
permission-what = Qué
permission-why = Por qué
permission-commands = Comandos
permission-verify = Comprobar
permission-build-what = Compilar el proyecto { $project }
permission-build-why = Para comprobar que el proyecto compila tras los cambios. Sus scripts de compilación se ejecutarán en tu ordenador.
permission-run-what = Ejecutar el programa del proyecto { $project }
permission-run-why = Has pedido ejecutar el programa compilado. Se ejecutará en tu ordenador.
permission-rollback-what = Volver el proyecto { $project } a { $target }
permission-rollback-why = Has elegido devolver los archivos del proyecto a este punto. Los cambios posteriores se descartarán.
permission-rollback-part-what = Dejar la parte «{ $part }» del proyecto { $project } como estaba
permission-rollback-part-why = Has elegido dejar esta parte como estaba antes de la última tarea que la cambió. Las demás partes no cambian.
permission-save-what = Guardar los cambios del proyecto { $project } en tu copia de GitHub
permission-save-why = Has elegido guardar tu trabajo. Tu copia de GitHub puede ser pública.
permission-ai-subscription-what = Instalar { $provider } (la aplicación oficial) e iniciar sesión en el navegador
permission-ai-subscription-why = Para que el agente haga tus tareas con tu suscripción de { $provider }.
permission-ai-key-what = Conectar { $provider } con una clave de API
permission-ai-key-why = Para que el agente haga tus tareas con { $provider }.
permission-ai-user-home-what = Usar tu sesión de { $provider } de { $home }
permission-ai-user-home-why = Para que el agente use tu suscripción sin clave de API. Las sesiones del agente aparecerán en tu historial de { $provider }.
permission-custom-ai-what = Conectar tu propio servidor de IA { $endpoint }
permission-custom-ai-why = El agente enviará el código del proyecto y tus tareas a esta dirección.
permission-tools-what = Instalar las herramientas que necesita el proyecto { $project }: { $tools }
permission-tools-why = El proyecto las necesita para compilar y ejecutarse. Van a la carpeta propia del agente, no al sistema.

## Answers in the task's conversation

note-taken = Entendido, lo tendré en cuenta en el próximo paso.
thought-rating = Calculando el tamaño de la tarea
thought-planning = Dividiendo la tarea en pasos
thought-task = Trabajando en la tarea
thought-step = Paso { $n } de { $of }: { $title }

## The map of the project's parts

# Why the map of parts could not be drawn; `error` is the agent's diagnostics, in English.
map-no-files = El proyecto aún no tiene archivos, así que no hay nada que dividir en partes
map-no-models = Conecta una IA para dividir el proyecto en partes
map-failed = No se pudo dividir el proyecto en partes: { $error }

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
custom-openai-key-prompt = introduce la clave de API de este servidor (déjala vacía si tu servidor local no pide autenticación):
custom-openai-model-prompt = introduce el identificador del modelo (p. ej., llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Permitir
btn-block = No permitir
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
tray-failed = «{ $project }» — la compilación ha fallado
tray-recent-projects = Proyectos recientes
tray-open-log = Abrir el registro
notify-done = Listo: { $project }
notify-needs-you = El agente necesita tu decisión
notify-build-failed = La compilación ha fallado
tray-agent-status = Estado del agente…

## Agent windows

status-window-title = Vibefork — estado del agente
window-close = Cerrar
first-run-title = Te damos la bienvenida a Vibefork
first-run-intro = El agente trabaja en este ordenador: prepara las herramientas que necesita un proyecto, ejecuta tu modelo de IA y compila el resultado.
first-run-folder = Carpeta de herramientas
first-run-folder-hint = Aquí se instalan las herramientas de tus proyectos. Elige un disco con espacio de sobra.
first-run-space-free = { $free } GB libres en este disco
first-run-space = Hasta { $needed } GB para todas las herramientas · { $free } GB libres
first-run-tools = Herramientas que el agente instala cuando un proyecto las necesita
first-run-tool-installed = instalada
size-mb = { $size } MB
size-gb = { $size } GB
first-run-connect-ai = Conectar un modelo de IA
first-run-connect-ai-hint = Tu suscripción (Claude Code, Codex, GitHub Copilot) o una clave de API, en el editor.
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
install-done-check = Para comprobarlo, abre un terminal nuevo y ejecuta: vibefork doctor
install-failed-system = No se puede instalar Vibefork aquí: este sistema ({ $system }) no es compatible. Vibefork funciona en Windows, macOS y Linux.
install-failed-processor = No se puede instalar Vibefork en este ordenador: su procesador ({ $cpu }) no es compatible.
install-failed-no-build = Vibefork { $version } todavía no está disponible para este ordenador ({ $platform }). Vuelve a intentarlo más tarde.
install-failed-download = No se ha podido descargar { $asset }. Comprueba tu conexión a internet y vuelve a intentarlo.
install-failed-hash = El archivo descargado { $asset } no es el esperado: puede estar dañado o haber sido sustituido. No se ha instalado nada. Vuelve a intentarlo más tarde.
install-failed-install = No se ha podido instalar Vibefork. Vuelve a ejecutar el comando de instalación.
install-failed-start = Vibefork está instalado, pero el agente no se ha iniciado. Inícialo con: vibefork start
