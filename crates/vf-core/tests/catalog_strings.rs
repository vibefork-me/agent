//! Every catalog string in English and Russian, written out by hand: moving the catalog to Fluent
//! must not change a single character of what people see.

use vf_core::catalog::*;

#[track_caller]
fn check(en: impl AsRef<str>, ru: impl AsRef<str>, want_en: &str, want_ru: &str) {
    assert_eq!(en.as_ref(), want_en);
    assert_eq!(ru.as_ref(), want_ru);
}

macro_rules! same {
    (|$l:ident| $e:expr, $en:expr, $ru:expr) => {{
        let f = |$l: Language| $e;
        check(f(Language::En), f(Language::Ru), $en, $ru);
    }};
}

#[test]
fn english_and_russian_strings_are_unchanged() {
    same!(
        |l| pair_title(l),
        "Vibefork: connect a browser?",
        "Vibefork: подключить браузер?"
    );
    same!(
        |l| pair_text(l, "http://localhost:5173", "Firefox", "123456"),
        "Site: http://localhost:5173\nBrowser: Firefox\nCode: 123456\n\nAllow only if the same code is shown on the page.",
        "Сайт: http://localhost:5173\nБраузер: Firefox\nКод: 123456\n\nРазрешайте, только если этот же код показан на странице."
    );
    same!(
        |l| project_access_title(l),
        "Vibefork: give access to a project?",
        "Vibefork: дать доступ к проекту?"
    );
    same!(
        |l| project_access_text(l, "http://localhost:5173", "Firefox", "notes-app"),
        "http://localhost:5173 (Firefox) asks to work in the project:\n\nnotes-app\n\nAllow only if you are doing this in your browser right now.",
        "http://localhost:5173 (Firefox) запрашивает доступ к проекту:\n\nnotes-app\n\nРазрешайте, только если вы сейчас работаете в браузере."
    );
    same!(
        |l| note_taken(l),
        "Got it, I'll take it into account in the next step.",
        "Принято, учту на следующем шаге."
    );
    same!(
        |l| ask_secret_title(l),
        "Vibefork: enter secret",
        "Vibefork: ввод ключа"
    );
    same!(
        |l| ask_secret_text(l, "http://localhost:5173", "API key"),
        "http://localhost:5173 asks for:\n\nAPI key",
        "http://localhost:5173 запрашивает:\n\nAPI key"
    );
    same!(
        |l| openrouter_key_prompt(l),
        "enter OpenRouter API key (get one at openrouter.ai)",
        "введите API-ключ OpenRouter (получите на openrouter.ai)"
    );
    same!(
        |l| gemini_key_prompt(l),
        "enter Google AI Studio (Gemini) API key (get one at aistudio.google.com):\n\nNote: Google free tier terms allow using inputs to train models.",
        "введите API-ключ Google AI Studio (получите на aistudio.google.com):\n\nВнимание: по условиям Google бесплатный уровень может использовать запросы для обучения моделей."
    );
    same!(
        |l| ask_text_title(l),
        "Vibefork: enter value",
        "Vibefork: ввод значения"
    );
    same!(
        |l| ask_text_text(l, "http://localhost:5173", "model"),
        "http://localhost:5173 asks for:\n\nmodel",
        "http://localhost:5173 запрашивает:\n\nmodel"
    );
    same!(
        |l| custom_openai_endpoint_prompt(l),
        "enter OpenAI-compatible API base URL (e.g. http://localhost:11434/v1):",
        "введите адрес OpenAI-совместимого API (например, http://localhost:11434/v1):"
    );
    same!(
        |l| custom_openai_key_prompt(l),
        "enter API key for this server (leave empty if your local server does not require authentication):",
        "введите API-ключ для этого сервера (оставьте пустым, если локальный сервер не требует авторизации):"
    );
    same!(
        |l| custom_openai_model_prompt(l),
        "enter model identifier (e.g. llama3, qwen2.5-coder):",
        "введите идентификатор модели (например, llama3, qwen2.5-coder):"
    );
    same!(|l| btn_allow(l), "Allow", "Разрешить");
    same!(|l| btn_block(l), "Don't allow", "Не разрешать");
    same!(|l| btn_ok(l), "OK", "OK");
    same!(|l| btn_cancel(l), "Cancel", "Отмена");
    same!(
        |l| tray_checking(l),
        "Vibefork: checking…",
        "Vibefork: проверка…"
    );
    same!(
        |l| tray_install_update(l, None),
        "Install update",
        "Установить обновление"
    );
    same!(
        |l| tray_install_update(l, Some("1.2.3")),
        "Install update 1.2.3",
        "Установить обновление 1.2.3"
    );
    same!(
        |l| tray_automatic_updates(l),
        "Automatic updates",
        "Автообновление"
    );
    same!(
        |l| tray_auto_updates_toggle(l, true),
        "Turn automatic updates off",
        "Отключить автообновление"
    );
    same!(
        |l| tray_auto_updates_toggle(l, false),
        "Turn automatic updates on",
        "Включить автообновление"
    );
    same!(
        |l| tray_open_vibefork(l),
        "Open Vibefork",
        "Открыть Vibefork"
    );
    same!(|l| tray_stop_agent(l), "Stop the agent", "Остановить агент");
    same!(
        |l| tray_start_agent(l),
        "Start the agent",
        "Запустить агент"
    );
    same!(|l| tray_quit(l), "Quit tray", "Выйти");
    same!(
        |l| tray_status_running(l, "1.0.0", 47821, None),
        "Vibefork 1.0.0: running on port 47821",
        "Vibefork 1.0.0: работает на порту 47821"
    );
    same!(
        |l| tray_status_running(l, "1.0.0", 47821, Some("1.1.0")),
        "Vibefork 1.0.0: running on port 47821 · update 1.1.0 available",
        "Vibefork 1.0.0: работает на порту 47821 · доступно обновление 1.1.0"
    );
    same!(
        |l| tray_status_stopped(l, None),
        "Vibefork: stopped",
        "Vibefork: остановлен"
    );
    same!(
        |l| tray_status_stopped(l, Some("1.1.0")),
        "Vibefork: stopped · update 1.1.0 available",
        "Vibefork: остановлен · доступно обновление 1.1.0"
    );
    same!(
        |l| cli_status_running(l, "1.0.0", 47821, "4242"),
        "running · version 1.0.0 · port 47821 · pid 4242",
        "работает · версия 1.0.0 · порт 47821 · pid 4242"
    );
    same!(
        |l| cli_status_not_running(l, 47821),
        "not running (port 47821)",
        "не запущен (порт 47821)"
    );
    same!(
        |l| cli_status_auto_update(l, true),
        "automatic updates: on",
        "автообновление: вкл"
    );
    same!(
        |l| cli_status_auto_update(l, false),
        "automatic updates: off",
        "автообновление: выкл"
    );
    same!(|l| cli_status_stopped(l), "stopped", "остановлен");
    same!(
        |l| cli_status_update_notice(l, "1.1.0"),
        " · update 1.1.0 available (run `vibefork update` to install)",
        " · доступно обновление 1.1.0 (запустите `vibefork update` для установки)"
    );
    same!(
        |l| cli_status_update_available(l, "1.1.0"),
        "update 1.1.0 is available — `vibefork update`",
        "доступно обновление 1.1.0 — `vibefork update`"
    );
    same!(|l| cli_github_connected(l, None), "connected", "подключен");
    same!(
        |l| cli_github_connected(l, Some("alice")),
        "connected as alice",
        "подключен как alice"
    );
    same!(
        |l| cli_github_not_connected(l),
        "not connected — `vibefork github connect`",
        "не подключен — `vibefork github connect`"
    );
    same!(
        |l| cli_github_disconnected(l),
        "disconnected (the app stays installed on your copies; remove it in GitHub settings if you want)",
        "отключено (приложение остаётся установленным в ваших копиях; при желании удалите его в настройках GitHub)"
    );
    same!(|l| cli_storage_header_files(l), "files", "файлов");
    same!(
        |l| cli_storage_unknown_folder(l, "music"),
        "vibefork: unknown folder `music` (projects, tools, caches, artifacts)",
        "vibefork: неизвестная папка `music` (projects, tools, caches, artifacts)"
    );
    same!(
        |l| cli_storage_stop_agent_first(l, 47821),
        "vibefork: stop the agent first (it is listening on port 47821)",
        "vibefork: сначала остановите агент (он слушает порт 47821)"
    );
    same!(
        |l| cli_storage_moved(l, "tools", "1.5 MB", 42, "D:/vibefork/tools"),
        "moved tools (1.5 MB, 42 files) to D:/vibefork/tools",
        "папка tools (1.5 MB, файлов: 42) перемещена в D:/vibefork/tools"
    );
    same!(
        |l| first_run_title(l),
        "Welcome to Vibefork",
        "Добро пожаловать в Vibefork"
    );
    same!(
        |l| first_run_space(l, Some(3 * 1_073_741_824), 64 * 1_073_741_824),
        "Up to 3.0 GB for all tools · 64.0 GB free",
        "До 3,0 ГБ на все инструменты · свободно 64,0 ГБ"
    );
    same!(
        |l| first_run_tools(l),
        "Tools the agent installs when a project needs them",
        "Инструменты, которые агент ставит, когда они нужны проекту"
    );
    same!(
        |l| first_run_tool_size(l, false, 343_823_876, true),
        "328 MB",
        "328 МБ"
    );
    same!(
        |l| first_run_tool_size(l, true, 0, true),
        "installed",
        "установлен"
    );
}
