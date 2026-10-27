# Strings of the agent's own windows, tray and command line, in Brazilian Portuguese.
# Same messages as en.ftl; a message missing here is shown in English.
# The person is addressed as "você"; product terms follow the editor's glossary (src/i18n/glossary.json).

## Pairing and project access dialogs

pair-title = Vibefork: conectar um navegador?
pair-text =
    Site: { $origin }
    Navegador: { $name }
    Código: { $code }

    Permita só se o mesmo código aparecer na página.
project-access-title = Vibefork: dar acesso a um projeto?
project-access-text =
    { $origin } ({ $name }) pede para trabalhar no projeto:

    { $project }

    Permita só se você estiver fazendo isso no navegador agora.

## Confirming an action

confirm-action-title = Vibefork: confirme uma ação
confirm-action-text =
    { $origin } pede para:

    { $action }
confirm-action-text-target =
    { $origin } pede para:

    { $action }
    Destino: { $target }
action-build = compilar o projeto (executa os scripts de compilação dele)
action-run = executar o programa do projeto
action-rollback = voltar o projeto a uma versão anterior (as mudanças posteriores são descartadas)
action-rollback-part = deixar esta parte do projeto como estava antes da última tarefa que a mudou (as outras partes ficam como estão)
action-save = salvar as mudanças do projeto na sua cópia no GitHub (ela pode ser pública)
action-connect-ai-subscription = configurar { $provider } (app oficial) e entrar pelo navegador
action-connect-ai-key = conectar { $provider } com uma chave de API
action-connect-ai-user-home = Permitir que o Vibefork use o seu login do { $provider }? As sessões do agente vão aparecer no seu histórico do { $provider }.
action-disconnect-ai = desconectar { $provider }

## Answers in the task's conversation

note-taken = Entendi, vou levar isso em conta no próximo passo.
thought-rating = Avaliando o tamanho da tarefa
thought-planning = Dividindo a tarefa em passos
thought-task = Trabalhando na tarefa
thought-step = Passo { $n } de { $of }: { $title }

## Entering a key or a value

ask-secret-title = Vibefork: digite a chave
ask-secret-text =
    { $origin } pede:

    { $prompt }
openrouter-key-prompt = digite a chave de API do OpenRouter (consiga uma em openrouter.ai)
gemini-key-prompt =
    digite a chave de API do Google AI Studio (Gemini) (consiga uma em aistudio.google.com):

    Atenção: nos termos do plano gratuito, o Google pode usar as solicitações para treinar modelos.
ask-text-title = Vibefork: digite um valor
ask-text-text =
    { $origin } pede:

    { $prompt }
custom-openai-endpoint-prompt = digite o endereço base da API compatível com OpenAI (ex.: http://localhost:11434/v1):
custom-openai-confirm-text =
    { $origin } pede para conectar uma API própria compatível com OpenAI:

    { $endpoint }

    O código do projeto e as solicitações serão enviados para este endereço.

    Permitir a conexão com este servidor?
custom-openai-key-prompt = digite a chave de API deste servidor (deixe em branco se o seu servidor local não pedir autenticação):
custom-openai-model-prompt = digite o identificador do modelo (ex.: llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Permitir
btn-block = Bloquear
btn-ok = OK
btn-cancel = Cancelar

## Tray

tray-checking = Vibefork: verificando…
tray-install-update = Instalar atualização
tray-install-update-version = Instalar atualização { $update }
tray-automatic-updates = Atualizações automáticas
tray-auto-updates-toggle =
    { $enabled ->
        [true] Desativar atualizações automáticas
       *[false] Ativar atualizações automáticas
    }
tray-open-vibefork = Abrir o Vibefork
tray-stop-agent = Parar o agente
tray-start-agent = Iniciar o agente
tray-quit = Fechar a bandeja
tray-status-running = Vibefork { $version }: em execução na porta { $port }
tray-status-running-update = Vibefork { $version }: em execução na porta { $port } · atualização { $update } disponível
tray-status-stopped = Vibefork: parado
tray-status-stopped-update = Vibefork: parado · atualização { $update } disponível

## Command line

cli-status-running = em execução · versão { $version } · porta { $port } · pid { $pid }
cli-status-not-running = não está em execução (porta { $port })
cli-status-auto-update =
    atualizações automáticas: { $enabled ->
        [true] ativadas
       *[false] desativadas
    }
cli-status-stopped = parado
cli-status-update-notice = { " " }· atualização { $update } disponível (execute `vibefork update` para instalar)
cli-status-update-available = a atualização { $version } está disponível — `vibefork update`
cli-github-connected = conectado
cli-github-connected-login = conectado como { $login }
cli-github-not-connected = não conectado — `vibefork github connect`
cli-github-disconnected = desconectado (o app continua instalado nas suas cópias; remova-o nas configurações do GitHub se quiser)
cli-storage-header-files = arquivos
cli-storage-unknown-folder = vibefork: pasta desconhecida `{ $kind }` (projects, tools, caches, artifacts)
cli-storage-stop-agent-first = vibefork: pare o agente primeiro (ele está escutando na porta { $port })
cli-storage-moved =
    { $kind } movido ({ $size }, { $files ->
        [one] { $files } arquivo
       *[other] { $files } arquivos
    }) para { $path }
