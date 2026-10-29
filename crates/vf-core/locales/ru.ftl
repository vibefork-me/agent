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

## Asking permission to act on the computer: what, why, the commands to check

permission-title = Vibefork: действие на вашем компьютере
permission-from = Запрос от { $origin }.
permission-explained = Ниже — что будет сделано и зачем.
permission-check = Каждую команду можно проверить в независимом источнике, прежде чем ответить.
permission-what = Что
permission-why = Зачем
permission-commands = Команды
permission-verify = Проверить
permission-build-what = Собрать проект { $project }
permission-build-why = Чтобы проверить, что проект собирается после изменений. Его скрипты сборки запустятся на вашем компьютере.
permission-run-what = Запустить программу проекта { $project }
permission-run-why = Вы попросили запустить собранную программу. Она запустится на вашем компьютере.
permission-rollback-what = Откатить проект { $project } к точке { $target }
permission-rollback-why = Вы выбрали вернуть файлы проекта к этой точке. Последующие изменения будут удалены.
permission-rollback-part-what = Вернуть часть «{ $part }» проекта { $project } как было
permission-rollback-part-why = Вы выбрали вернуть эту часть к виду до последней задачи, которая её меняла. Остальные части не меняются.
permission-save-what = Сохранить изменения проекта { $project } в вашу копию на GitHub
permission-save-why = Вы выбрали сохранить работу. Ваша копия на GitHub может быть публичной.
permission-ai-subscription-what = Установить { $provider } (официальное приложение) и войти в браузере
permission-ai-subscription-why = Чтобы агент выполнял ваши задачи через вашу подписку { $provider }.
permission-ai-key-what = Подключить { $provider } через API-ключ
permission-ai-key-why = Чтобы агент выполнял ваши задачи через { $provider }.
permission-ai-user-home-what = Использовать ваш вход в { $provider } из { $home }
permission-ai-user-home-why = Чтобы агент работал через вашу подписку без API-ключа. Сессии агента появятся в вашей истории { $provider }.
permission-custom-ai-what = Подключить ваш сервер ИИ { $endpoint }
permission-custom-ai-why = Агент будет отправлять на этот адрес код проекта и ваши задачи.
permission-tools-what = Установить инструменты, нужные проекту { $project }: { $tools }
permission-tools-why = Без них проект не собрать и не запустить. Они ставятся в собственную папку агента, а не в систему.

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
custom-openai-key-prompt = введите API-ключ для этого сервера (оставьте пустым, если локальный сервер не требует авторизации):
custom-openai-model-prompt = введите идентификатор модели (например, llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Разрешить
btn-block = Не разрешать
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
first-run-space = До { $needed } ГБ на все инструменты · свободно { $free } ГБ
first-run-tools = Инструменты, которые агент ставит, когда они нужны проекту
first-run-tool-installed = установлен
size-mb = { $size } МБ
size-gb = { $size } ГБ
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

## Installer (install.ps1, install.sh)

install-downloading = Загрузка Vibefork { $version }…
install-installing = Установка…
install-done = Vibefork { $version } установлен: { $path }
install-done-check = Чтобы проверить, откройте новый терминал и выполните: vibefork doctor
install-failed-system = Vibefork нельзя установить здесь: эта система ({ $system }) не поддерживается. Vibefork работает на Windows, macOS и Linux.
install-failed-processor = Vibefork нельзя установить на этот компьютер: его процессор ({ $cpu }) не поддерживается.
install-failed-no-build = Vibefork { $version } пока недоступен для этого компьютера ({ $platform }). Попробуйте позже.
install-failed-download = Не удалось скачать { $asset }. Проверьте подключение к интернету и попробуйте снова.
install-failed-hash = Скачанный файл { $asset } не совпадает с ожидаемым: он мог повредиться или быть подменён. Ничего не установлено. Попробуйте позже.
install-failed-install = Установить Vibefork не удалось. Запустите команду установки ещё раз.
install-failed-start = Vibefork установлен, но агент не запустился. Запустите его командой: vibefork start
