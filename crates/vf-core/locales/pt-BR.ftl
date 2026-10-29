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

## Asking permission to act on the computer: what, why, the commands to check

permission-title = Vibefork: ação no seu computador
permission-from = Pedido de { $origin }.
permission-explained = Veja o que será feito e por quê.
permission-check = Você pode conferir cada comando em uma fonte independente antes de responder.
permission-what = O que
permission-why = Por que
permission-commands = Comandos
permission-verify = Verificar
permission-build-what = Compilar o projeto { $project }
permission-build-why = Para verificar se o projeto compila depois das mudanças. Os scripts de compilação dele vão rodar no seu computador.
permission-run-what = Executar o programa do projeto { $project }
permission-run-why = Você pediu para executar o programa compilado. Ele vai rodar no seu computador.
permission-rollback-what = Voltar o projeto { $project } para { $target }
permission-rollback-why = Você escolheu voltar os arquivos do projeto para este ponto. As mudanças posteriores serão descartadas.
permission-rollback-part-what = Deixar a parte “{ $part }” do projeto { $project } como estava
permission-rollback-part-why = Você escolheu deixar esta parte como estava antes da última tarefa que a mudou. As outras partes ficam como estão.
permission-save-what = Salvar as mudanças do projeto { $project } na sua cópia no GitHub
permission-save-why = Você escolheu salvar seu trabalho. Sua cópia no GitHub pode ser pública.
permission-ai-subscription-what = Instalar o { $provider } (o app oficial) e entrar pelo navegador
permission-ai-subscription-why = Para que o agente faça suas tarefas pela sua assinatura do { $provider }.
permission-ai-key-what = Conectar o { $provider } com uma chave de API
permission-ai-key-why = Para que o agente faça suas tarefas pelo { $provider }.
permission-ai-user-home-what = Usar seu login do { $provider } de { $home }
permission-ai-user-home-why = Para que o agente use sua assinatura sem chave de API. As sessões do agente vão aparecer no seu histórico do { $provider }.
permission-custom-ai-what = Conectar seu próprio servidor de IA { $endpoint }
permission-custom-ai-why = O agente vai enviar o código do projeto e suas tarefas para este endereço.
permission-tools-what = Instalar as ferramentas de que o projeto { $project } precisa: { $tools }
permission-tools-why = O projeto precisa delas para compilar e rodar. Elas vão para a pasta do próprio agente, não para o sistema.

## Answers in the task's conversation

note-taken = Entendi, vou levar isso em conta no próximo passo.
thought-rating = Avaliando o tamanho da tarefa
thought-planning = Dividindo a tarefa em passos
thought-task = Trabalhando na tarefa
thought-step = Passo { $n } de { $of }: { $title }

## The map of the project's parts

# Why the map of parts could not be drawn; `error` is the agent's diagnostics, in English.
map-no-files = O projeto ainda não tem arquivos, então não há o que dividir em partes
map-no-models = Conecte uma IA para dividir o projeto em partes
map-failed = Não foi possível dividir o projeto em partes: { $error }

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
custom-openai-key-prompt = digite a chave de API deste servidor (deixe em branco se o seu servidor local não pedir autenticação):
custom-openai-model-prompt = digite o identificador do modelo (ex.: llama3, qwen2.5-coder):

## Buttons of native dialogs

btn-allow = Permitir
btn-block = Não permitir
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

tray-stage-download = baixando
tray-stage-parts = olhando as partes
tray-stage-links = ligando as partes
tray-stage-tools = preparando ferramentas
tray-stage-summary = resumindo
tray-stage-understand = entendendo a tarefa
tray-stage-plan = planejando
tray-stage-change = alterando
tray-stage-check = verificando
tray-stage-done = finalizando
tray-activity = “{ $project }”
tray-activity-stage = “{ $project }” — { $stage }
tray-activity-eta = { $line }, faltam ≈ { $minutes } min
tray-waiting = aguardando
tray-needs-you = “{ $project }” — sua decisão é necessária
tray-failed = “{ $project }” — a compilação falhou
tray-recent-projects = Projetos recentes
tray-open-log = Abrir o log
notify-done = Pronto: { $project }
notify-needs-you = O agente precisa da sua decisão
notify-build-failed = A compilação falhou
tray-agent-status = Status do agente…

## Agent windows

status-window-title = Vibefork — status do agente
window-close = Fechar
first-run-title = Boas-vindas ao Vibefork
first-run-intro = O agente trabalha neste computador: prepara as ferramentas de que um projeto precisa, executa o seu modelo de IA e compila o resultado.
first-run-folder = Pasta das ferramentas
first-run-folder-hint = As ferramentas dos seus projetos ficam aqui. Escolha um disco com espaço de sobra.
first-run-space-free = { $free } GB livres neste disco
first-run-space = Até { $needed } GB para todas as ferramentas · { $free } GB livres
first-run-tools = Ferramentas que o agente instala quando um projeto precisa delas
first-run-tool-installed = instalada
size-mb = { $size } MB
size-gb = { $size } GB
first-run-connect-ai = Conectar um modelo de IA
first-run-connect-ai-hint = Sua assinatura (Claude Code, Codex, GitHub Copilot) ou uma chave de API — no editor.
first-run-start = Continuar
first-run-later = Depois
first-run-folder-failed = A pasta das ferramentas continua onde estava: { $error }

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
        [0] { $files } arquivos
        [one] { $files } arquivo
       *[other] { $files } arquivos
    }) para { $path }

## Installer (install.ps1, install.sh)

install-downloading = Baixando o Vibefork { $version }…
install-installing = Instalando…
install-done = O Vibefork { $version } foi instalado: { $path }
install-done-check = Para conferir, abra um novo terminal e execute: vibefork doctor
install-failed-system = Não é possível instalar o Vibefork aqui: este sistema ({ $system }) não é compatível. O Vibefork funciona no Windows, macOS e Linux.
install-failed-processor = Não é possível instalar o Vibefork neste computador: o processador dele ({ $cpu }) não é compatível.
install-failed-no-build = O Vibefork { $version } ainda não está disponível para este computador ({ $platform }). Tente novamente mais tarde.
install-failed-download = Não foi possível baixar { $asset }. Verifique sua conexão com a internet e tente novamente.
install-failed-hash = O arquivo baixado { $asset } não é o esperado: ele pode estar danificado ou ter sido substituído. Nada foi instalado. Tente novamente mais tarde.
install-failed-install = Não foi possível instalar o Vibefork. Execute o comando de instalação novamente.
install-failed-start = O Vibefork foi instalado, mas o agente não iniciou. Inicie-o com: vibefork start

## Windows installer wizard (scripts/package.py)

msi-setup = Instalação do Vibefork
msi-newer-installed = Uma versão mais recente do Vibefork já está instalada.
msi-feature-desktop = Atalho na área de trabalho
msi-feature-autostart = Iniciar ao entrar no Windows
msi-start = Iniciar o Vibefork
msi-welcome-title = Boas-vindas à instalação do Vibefork
msi-welcome-text = O Vibefork será instalado neste computador só para você: não é preciso ser administrador. Clique em Avançar para continuar ou em Cancelar para sair.
msi-welcome-update-text = O Vibefork será atualizado neste computador. Clique em Avançar para continuar ou em Cancelar para sair.
msi-maintenance-text = Você pode reparar o Vibefork ou removê-lo do computador. Clique em Avançar para continuar ou em Cancelar para sair.
msi-prepare-text = Aguarde enquanto a instalação é preparada.
msi-folder-title = Pasta de destino
msi-folder-text = Clique em Avançar para instalar nesta pasta ou em Alterar para escolher outra.
msi-folder-label = Instalar o Vibefork em:
msi-folder-browse-title = Alterar a pasta de destino
msi-folder-browse-text = Escolha a pasta do Vibefork.
msi-folder-invalid = A pasta precisa estar em um disco local deste computador.
msi-shortcuts-title = Atalhos
msi-shortcuts-text = Escolha como abrir o Vibefork. Ele sempre aparece no menu Iniciar.
msi-shortcut-desktop = Criar um atalho na área de trabalho
msi-shortcut-sign-in = Iniciar o Vibefork quando eu entrar no Windows
msi-ready-install-title = Tudo pronto para instalar o Vibefork
msi-ready-install-text = Clique em Instalar para começar. Clique em Voltar para revisar suas escolhas ou em Cancelar para sair.
msi-ready-change-title = Tudo pronto para alterar o Vibefork
msi-ready-change-text = Clique em Alterar para começar. Clique em Voltar para revisar suas escolhas ou em Cancelar para sair.
msi-ready-repair-title = Tudo pronto para reparar o Vibefork
msi-ready-repair-text = Clique em Reparar para restaurar arquivos, atalhos e configurações do Vibefork que estejam faltando ou danificados. Clique em Voltar para revisar suas escolhas ou em Cancelar para sair.
msi-ready-remove-title = Tudo pronto para remover o Vibefork
msi-ready-remove-text = Clique em Remover para remover o Vibefork do computador. Seus dados e projetos continuam lá. Clique em Voltar para revisar suas escolhas ou em Cancelar para sair.
msi-ready-update-title = Tudo pronto para atualizar o Vibefork
msi-ready-update-text = Clique em Atualizar para atualizar o Vibefork. Clique em Voltar para revisar suas escolhas ou em Cancelar para sair.
msi-progress-installing-title = Instalando o Vibefork
msi-progress-installing-text = Aguarde enquanto o Vibefork é instalado.
msi-progress-changing-title = Alterando o Vibefork
msi-progress-changing-text = Aguarde enquanto o Vibefork é alterado.
msi-progress-repairing-title = Reparando o Vibefork
msi-progress-repairing-text = Aguarde enquanto o Vibefork é reparado.
msi-progress-removing-title = Removendo o Vibefork
msi-progress-removing-text = Aguarde enquanto o Vibefork é removido.
msi-progress-updating-title = Atualizando o Vibefork
msi-progress-updating-text = Aguarde enquanto o Vibefork é atualizado.
msi-maintenance-type-title = Alterar, reparar ou remover o Vibefork
msi-maintenance-type-text = Escolha o que fazer.
msi-maintenance-change-text = Altera as opções da instalação.
msi-maintenance-change-disabled = O Vibefork não tem opções para alterar.
msi-maintenance-repair-text = Restaura arquivos, atalhos e configurações que estejam faltando ou danificados.
msi-maintenance-repair-disabled = O Vibefork não pode ser reparado.
msi-maintenance-remove-text = Remove o Vibefork do computador. Seus dados e projetos continuam lá.
msi-maintenance-remove-disabled = O Vibefork não pode ser removido.
msi-resume-title = Retomando a instalação do Vibefork
msi-resume-text = A instalação do Vibefork neste computador será concluída. Clique em Instalar para continuar ou em Cancelar para sair.
msi-exit-title = Concluído
msi-exit-text = Clique em Concluir para fechar a instalação.
msi-user-exit-title = A instalação foi interrompida
msi-user-exit-text = Nada mudou no computador. Para instalar o Vibefork, execute a instalação de novo.
msi-fatal-title = A instalação parou por causa de um erro
msi-fatal-text = Nada mudou no computador. Para instalar o Vibefork, execute a instalação de novo mais tarde.
msi-cancel-question = Cancelar a instalação do Vibefork?
msi-files-in-use-title = Arquivos em uso
msi-files-in-use-description = Alguns arquivos que precisam ser atualizados estão em uso.
msi-files-in-use-text = Estes programas estão usando arquivos que precisam ser atualizados. Feche-os e clique em Tentar Novamente para continuar, ou clique em Sair.
msi-files-in-use-close-text = Estes programas estão usando arquivos que precisam ser atualizados. A instalação pode fechá-los e iniciá-los de novo, ou você pode reiniciar o computador depois.
msi-files-in-use-close = &Fechar os programas e iniciá-los de novo
msi-files-in-use-keep = &Não fechar os programas (será preciso reiniciar o computador)
msi-disk-title = Espaço em disco insuficiente
msi-disk-description = A instalação precisa de mais espaço em disco do que o disponível.
msi-disk-cost-title = Espaço em disco
msi-disk-cost-description = O espaço em disco de que o Vibefork precisa para ser instalado.
msi-disk-text = As unidades destacadas não têm espaço livre suficiente. Libere espaço nelas ou escolha outra unidade.
msi-disk-rollback = Você também pode desativar a reversão: assim a instalação não consegue restaurar o computador se for interrompida. Clique em Sim para assumir esse risco.
msi-costing-text = Aguarde enquanto a instalação verifica o espaço em disco.
