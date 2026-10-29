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

    Escribe este código en la página y luego haz clic en Permitir.
    Si no acabas de abrir Vibefork, haz clic en No permitir.
project-access-title = Vibefork: ¿dar acceso a un proyecto?
project-access-text =
    { $origin } ({ $name }) pide trabajar en el proyecto:

    { $project }

    Permítelo solo si lo estás haciendo ahora mismo en tu navegador.

## Starting work on a project: the first task of the editor in it

start-work-title = Vibefork: ¿empezar a trabajar en el proyecto?
start-work-text =
    { $origin } pide empezar a trabajar en el proyecto { $project } con la primera tarea:

    { $task }

    El modelo podrá cambiar los archivos del proyecto y ejecutar comandos en tu ordenador. Las siguientes tareas de este proyecto empezarán sin preguntar mientras el editor siga abierto.

    Empieza solo si acabas de dar esta tarea tú.

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
thought-explain = Estoy mirando tu pregunta
thought-explain-part = Estoy mirando la parte «{ $part }»

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
btn-start = Empezar
btn-not-now = Ahora no
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

## Windows installer wizard (scripts/package.py)

msi-setup = Instalación de Vibefork
msi-newer-installed = Ya está instalada una versión más reciente de Vibefork.
msi-feature-desktop = Acceso directo en el escritorio
msi-feature-autostart = Iniciar con Windows
msi-start = Iniciar Vibefork
msi-welcome-title = Te damos la bienvenida a la instalación de Vibefork
msi-welcome-text = Vibefork se instalará en este ordenador solo para ti: no hacen falta permisos de administrador. Haz clic en Siguiente para continuar o en Cancelar para salir.
msi-welcome-update-text = Vibefork se actualizará en este ordenador. Haz clic en Siguiente para continuar o en Cancelar para salir.
msi-maintenance-text = Puedes reparar Vibefork o quitarlo del ordenador. Haz clic en Siguiente para continuar o en Cancelar para salir.
msi-prepare-text = Espera mientras se prepara la instalación.
msi-folder-title = Carpeta de destino
msi-folder-text = Haz clic en Siguiente para instalar en esta carpeta o en Cambiar para elegir otra.
msi-folder-label = Instalar Vibefork en:
msi-folder-browse-title = Cambiar la carpeta de destino
msi-folder-browse-text = Elige la carpeta de Vibefork.
msi-folder-invalid = La carpeta debe estar en un disco local de este ordenador.
msi-shortcuts-title = Accesos directos
msi-shortcuts-text = Elige cómo abrir Vibefork. Siempre está en el menú Inicio.
msi-shortcut-desktop = Crear un acceso directo en el escritorio
msi-shortcut-sign-in = Iniciar Vibefork al iniciar sesión en Windows
msi-ready-install-title = Todo listo para instalar Vibefork
msi-ready-install-text = Haz clic en Instalar para empezar. Haz clic en Atrás para revisar lo que has elegido o en Cancelar para salir.
msi-ready-change-title = Todo listo para cambiar Vibefork
msi-ready-change-text = Haz clic en Cambiar para empezar. Haz clic en Atrás para revisar lo que has elegido o en Cancelar para salir.
msi-ready-repair-title = Todo listo para reparar Vibefork
msi-ready-repair-text = Haz clic en Reparar para restaurar los archivos, accesos directos y ajustes de Vibefork que falten o estén dañados. Haz clic en Atrás para revisar lo que has elegido o en Cancelar para salir.
msi-ready-remove-title = Todo listo para quitar Vibefork
msi-ready-remove-text = Haz clic en Quitar para quitar Vibefork del ordenador. Tus datos y proyectos se conservan. Haz clic en Atrás para revisar lo que has elegido o en Cancelar para salir.
msi-ready-update-title = Todo listo para actualizar Vibefork
msi-ready-update-text = Haz clic en Actualizar para actualizar Vibefork. Haz clic en Atrás para revisar lo que has elegido o en Cancelar para salir.
msi-progress-installing-title = Instalando Vibefork
msi-progress-installing-text = Espera mientras se instala Vibefork.
msi-progress-changing-title = Cambiando Vibefork
msi-progress-changing-text = Espera mientras se cambia Vibefork.
msi-progress-repairing-title = Reparando Vibefork
msi-progress-repairing-text = Espera mientras se repara Vibefork.
msi-progress-removing-title = Quitando Vibefork
msi-progress-removing-text = Espera mientras se quita Vibefork.
msi-progress-updating-title = Actualizando Vibefork
msi-progress-updating-text = Espera mientras se actualiza Vibefork.
msi-maintenance-type-title = Cambiar, reparar o quitar Vibefork
msi-maintenance-type-text = Elige qué hacer.
msi-maintenance-change-text = Cambia las opciones de la instalación.
msi-maintenance-change-disabled = Vibefork no tiene opciones que cambiar.
msi-maintenance-repair-text = Restaura los archivos, accesos directos y ajustes que falten o estén dañados.
msi-maintenance-repair-disabled = Vibefork no se puede reparar.
msi-maintenance-remove-text = Quita Vibefork del ordenador. Tus datos y proyectos se conservan.
msi-maintenance-remove-disabled = Vibefork no se puede quitar.
msi-resume-title = Reanudando la instalación de Vibefork
msi-resume-text = Se terminará de instalar Vibefork en este ordenador. Haz clic en Instalar para continuar o en Cancelar para salir.
msi-exit-title = Listo
msi-exit-text = Haz clic en Finalizar para cerrar la instalación.
msi-user-exit-title = Se ha interrumpido la instalación
msi-user-exit-text = No ha cambiado nada en el ordenador. Para instalar Vibefork, vuelve a ejecutar la instalación.
msi-fatal-title = La instalación se ha detenido por un error
msi-fatal-text = No ha cambiado nada en el ordenador. Para instalar Vibefork, vuelve a ejecutar la instalación más tarde.
msi-cancel-question = ¿Cancelar la instalación de Vibefork?
msi-files-in-use-title = Archivos en uso
msi-files-in-use-description = Algunos archivos que hay que actualizar están en uso.
msi-files-in-use-text = Estos programas usan archivos que hay que actualizar. Ciérralos y haz clic en Reintentar para continuar, o haz clic en Salir.
msi-files-in-use-close-text = Estos programas usan archivos que hay que actualizar. La instalación puede cerrarlos y volver a abrirlos, o puedes reiniciar el ordenador más tarde.
msi-files-in-use-close = &Cerrar los programas y volver a abrirlos
msi-files-in-use-keep = &No cerrar los programas (habrá que reiniciar el ordenador)
msi-disk-title = No hay suficiente espacio en disco
msi-disk-description = La instalación necesita más espacio en disco del que hay libre.
msi-disk-cost-title = Espacio en disco
msi-disk-cost-description = El espacio en disco que Vibefork necesita para instalarse.
msi-disk-text = Las unidades resaltadas no tienen suficiente espacio libre. Libera espacio en ellas o elige otra unidad.
msi-disk-rollback = También puedes desactivar la reversión: así la instalación no podrá restaurar el ordenador si se interrumpe. Haz clic en Sí para asumir ese riesgo.
msi-costing-text = Espera mientras se comprueba el espacio en disco.
