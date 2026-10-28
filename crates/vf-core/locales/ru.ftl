# Strings of the agent's own windows, tray and command line, in Russian.
# Same messages as en.ftl; a message missing here is shown in English.

## Pairing and project access dialogs

pair-title = Vibefork: подключить браузер?
pair-text =
    Сайт: { $origin }
    Браузер: { $name }
    Код: { $code }

    Разрешайте, только если этот же код показан на странице.
project-access-title = Vibefork: дать доступ к проекту?
project-access-text =
    { $origin } ({ $name }) запрашивает доступ к проекту:

    { $project }

    Разрешайте, только если вы сейчас работаете в браузере.

## Confirming an action

confirm-action-title = Vibefork: подтверждение действия
confirm-action-text =
    { $origin } запрашивает действие:

    { $action }
confirm-action-text-target =
    { $origin } запрашивает действие:

    { $action }
    Цель: { $target }
action-build = собрать и запустить проект
action-run = запустить программу проекта
action-rollback = откатить проект (последующие изменения будут удалены)
action-rollback-part = вернуть эту часть проекта к виду до последней задачи, которая её меняла (остальные части не меняются)
action-save = сохранить изменения проекта в вашу копию на GitHub (она может быть публичной)
action-connect-ai-subscription = установить { $provider } (официальное приложение) и войти в браузере
action-connect-ai-key = подключить { $provider } через API-ключ
action-connect-ai-user-home = Разрешить Vibefork работать с вашим входом в { $provider }? Сессии агента появятся в вашей истории { $provider }.
action-disconnect-ai = отключить { $provider }

## Answers in the task's conversation

note-taken = Принято, учту на следующем шаге.
thought-rating = Оцениваю, насколько велика задача
thought-planning = Разбиваю задачу на шаги
thought-task = Делаю задачу
thought-step = Шаг { $n } из { $of }: { $title }

## Entering a key or a value

ask-secret-title = Vibefork: ввод ключа
ask-secret-text =
    { $origin } запрашивает:

    { $prompt }
openrouter-key-prompt = введите API-ключ OpenRouter (получите на openrouter.ai)
gemini-key-prompt =
    введите API-ключ Google AI Studio (получите на aistudio.google.com):

    Внимание: по условиям Google бесплатный уровень может использовать запросы для обучения моделей.
ask-text-title = Vibefork: ввод значения
ask-text-text =
    { $origin } запрашивает:

    { $prompt }
custom-openai-endpoint-prompt = введите адрес OpenAI-совместимого API (например, http://localhost:11434/v1):
custom-openai-confirm-text =
    { $origin } запрашивает подключение своего OpenAI-совместимого API:

    { $endpoint }

    Код проекта и запросы будут отправляться на этот адрес.

    Разрешить подключение к этому серверу?
custom-openai-key-prompt = введите API-ключ для этого сервера (оставьте пустым, если локальный сервер не требует авторизации):
custom-openai-model-prompt = введите идентификатор модели (например, llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Разрешить
btn-block = Заблокировать
btn-ok = OK
btn-cancel = Отмена

## Tray

tray-checking = Vibefork: проверка…
tray-install-update = Установить обновление
tray-install-update-version = Установить обновление { $update }
tray-automatic-updates = Автообновление
tray-auto-updates-toggle =
    { $enabled ->
        [true] Отключить автообновление
       *[false] Включить автообновление
    }
tray-open-vibefork = Открыть Vibefork
tray-stop-agent = Остановить агент
tray-start-agent = Запустить агент
tray-quit = Выйти
tray-status-running = Vibefork { $version }: работает на порту { $port }
tray-status-running-update = Vibefork { $version }: работает на порту { $port } · доступно обновление { $update }
tray-status-stopped = Vibefork: остановлен
tray-status-stopped-update = Vibefork: остановлен · доступно обновление { $update }

tray-stage-download = загрузка
tray-stage-parts = разбор частей
tray-stage-links = связи частей
tray-stage-tools = подготовка инструментов
tray-stage-summary = итоги
tray-stage-understand = понимание задачи
tray-stage-plan = план
tray-stage-change = изменения
tray-stage-check = проверка
tray-stage-done = завершение
tray-activity = «{ $project }»
tray-activity-stage = «{ $project }» — { $stage }
tray-activity-eta = { $line }, ещё ≈ { $minutes } мин
tray-waiting = ждёт
tray-needs-you = «{ $project }» — нужно твоё решение
tray-failed = «{ $project }» — сборка не прошла
tray-recent-projects = Недавние проекты
tray-open-log = Открыть журнал
notify-done = Готово: { $project }
notify-needs-you = Агенту нужно твоё решение
notify-build-failed = Сборка не прошла
tray-agent-status = Состояние агента…

## Agent windows

status-window-title = Vibefork — состояние агента
window-close = Закрыть
first-run-title = Добро пожаловать в Vibefork
first-run-intro = Агент работает на этом компьютере: готовит инструменты, нужные проекту, запускает твою ИИ-модель и собирает результат.
first-run-folder = Папка для инструментов
first-run-folder-hint = Сюда ставятся инструменты сборки твоих проектов. Выбери диск, где хватает места.
first-run-space-free = Свободно на этом диске: { $free } ГБ
first-run-space = Нужно около { $needed } ГБ · свободно { $free } ГБ
first-run-connect-ai = Подключить ИИ-модель
first-run-connect-ai-hint = Подписка (Claude Code, Codex) или API-ключ — в редакторе.
first-run-start = Продолжить
first-run-later = Позже
first-run-folder-failed = Папка инструментов осталась на месте: { $error }

## Command line

cli-status-running = работает · версия { $version } · порт { $port } · pid { $pid }
cli-status-not-running = не запущен (порт { $port })
cli-status-auto-update =
    автообновление: { $enabled ->
        [true] вкл
       *[false] выкл
    }
cli-status-stopped = остановлен
cli-status-update-notice = { " " }· доступно обновление { $update } (запустите `vibefork update` для установки)
cli-status-update-available = доступно обновление { $version } — `vibefork update`
cli-github-connected = подключен
cli-github-connected-login = подключен как { $login }
cli-github-not-connected = не подключен — `vibefork github connect`
cli-github-disconnected = отключено (приложение остаётся установленным в ваших копиях; при желании удалите его в настройках GitHub)
cli-storage-header-files = файлов
cli-storage-unknown-folder = vibefork: неизвестная папка `{ $kind }` (projects, tools, caches, artifacts)
cli-storage-stop-agent-first = vibefork: сначала остановите агент (он слушает порт { $port })
cli-storage-moved = папка { $kind } ({ $size }, файлов: { $files }) перемещена в { $path }
