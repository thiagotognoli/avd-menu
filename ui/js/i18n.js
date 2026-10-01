// Textos da interface: cada chave tem [português, inglês]; os demais idiomas ficam em
// lang/<código>.js (chave → texto) e, sem a chave, caem para o inglês.
import { emit } from './store.js';
import zh from './lang/zh.js';
import hi from './lang/hi.js';
import es from './lang/es.js';
import fr from './lang/fr.js';
import ar from './lang/ar.js';
import bn from './lang/bn.js';
import ru from './lang/ru.js';
import ur from './lang/ur.js';
import id from './lang/id.js';

const PACKS = { zh, hi, es, fr, ar, bn, ru, ur, id };

/** Idiomas da interface, cada um no próprio nome. `rtl`: escrita da direita para a esquerda. */
export const LANGUAGES = [
  { code: 'pt', name: 'Português (Brasil)', html: 'pt-BR' },
  { code: 'en', name: 'English', html: 'en' },
  { code: 'zh', name: '中文（简体）', html: 'zh-CN' },
  { code: 'hi', name: 'हिन्दी', html: 'hi' },
  { code: 'es', name: 'Español', html: 'es' },
  { code: 'fr', name: 'Français', html: 'fr' },
  { code: 'ar', name: 'العربية', html: 'ar', rtl: true },
  { code: 'bn', name: 'বাংলা', html: 'bn' },
  { code: 'ru', name: 'Русский', html: 'ru' },
  { code: 'ur', name: 'اردو', html: 'ur', rtl: true },
  { code: 'id', name: 'Bahasa Indonesia', html: 'id' },
];

const S = {
  // gerais
  ok: ['OK', 'OK'], cancel: ['Cancelar', 'Cancel'], close: ['Fechar', 'Close'], save: ['Salvar', 'Save'],
  back: ['Voltar', 'Back'], next: ['Avançar', 'Next'], finish: ['Concluir', 'Finish'], delete: ['Apagar', 'Delete'],
  remove: ['Remover', 'Remove'], refresh: ['Atualizar', 'Refresh'], more: ['Mais ações', 'More actions'],
  start: ['Iniciar', 'Start'], stop: ['Parar', 'Stop'], install: ['Instalar', 'Install'], apply: ['Aplicar', 'Apply'],
  discard: ['Descartar', 'Discard'], copy: ['Copiar', 'Copy'], copied: ['Copiado!', 'Copied!'],
  'already.installed': ['Tudo já está instalado.', 'Everything is already installed.'],
  'nav.devices': ['Dispositivos', 'Devices'], 'nav.sdk': ['SDK Manager', 'SDK Manager'], 'nav.health': ['Diagnóstico', 'Health'],
  'nav.settings': ['Configurações', 'Settings'],
  'conn.lost': ['Conexão com o AVD Menu perdida — o programa foi encerrado?', 'Lost connection to AVD Menu — was the program closed?'],

  // lista de dispositivos
  'devices.title': ['Dispositivos virtuais', 'Virtual devices'], 'devices.create': ['Criar dispositivo', 'Create device'],
  'devices.search': ['Buscar…', 'Search…'], 'devices.nomatch': ['Nenhum dispositivo corresponde à busca.', 'No device matches your search.'],
  'devices.empty.title': ['Nenhum dispositivo virtual ainda', 'No virtual devices yet'],
  'devices.empty.text': ['Crie um emulador Android escolhendo o aparelho e a versão do sistema. O AVD Menu baixa o que for preciso.', 'Create an Android emulator by picking a device and a system version. AVD Menu downloads whatever is needed.'],
  'status.running': ['Em execução', 'Running'], 'status.stopped': ['Parado', 'Stopped'], 'status.starting': ['Iniciando…', 'Starting…'],
  'status.stopping': ['Parando…', 'Stopping…'], 'status.noimage': ['Sem imagem', 'No image'], 'status.problem': ['Problema', 'Problem'],
  'avd.download.image': ['Baixar imagem', 'Download image'],
  'shortcut.exists': ['Tem atalho no menu', 'Has a menu shortcut'],
  'menu.cold': ['Iniciar com cold boot', 'Cold boot'], 'menu.wipestart': ['Apagar dados e iniciar', 'Wipe data and start'],
  'menu.dgpu': ['Iniciar com GPU dedicada', 'Start with dedicated GPU'], 'menu.headless': ['Iniciar sem janela', 'Start without a window'],
  'menu.edit': ['Editar…', 'Edit…'], 'menu.duplicate': ['Duplicar…', 'Duplicate…'], 'menu.show': ['Mostrar no disco', 'Show on disk'],
  'menu.log': ['Ver log', 'View log'], 'menu.wipe': ['Apagar dados…', 'Wipe data…'], 'menu.delete': ['Excluir…', 'Delete…'],
  'menu.shortcut.create': ['Criar atalho no menu…', 'Create menu shortcut…'], 'menu.shortcut.edit': ['Editar atalho do menu…', 'Edit menu shortcut…'],
  'menu.shortcut.remove': ['Remover atalho do menu', 'Remove menu shortcut'],
  'avd.started': ['Emulador “{name}” iniciado.', 'Emulator “{name}” started.'],
  'avd.start.failed': ['Não foi possível iniciar “{name}”', 'Could not start “{name}”'], 'avd.stop.failed': ['Não foi possível parar', 'Could not stop'],
  'avd.exited': ['O emulador “{name}” terminou inesperadamente', 'Emulator “{name}” exited unexpectedly'],
  'avd.delete.title': ['Excluir dispositivo', 'Delete device'], 'avd.delete.msg': ['Excluir “{name}” e todos os seus dados? Isso não pode ser desfeito.', 'Delete “{name}” and all its data? This cannot be undone.'],
  'avd.deleted': ['Dispositivo excluído.', 'Device deleted.'],
  'avd.wipe': ['Apagar dados', 'Wipe data'], 'avd.wipe.title': ['Apagar dados', 'Wipe data'],
  'avd.wipe.msg': ['Restaurar “{name}” ao estado de fábrica? Apps e arquivos serão perdidos (o cartão SD é mantido).', 'Reset “{name}” to factory state? Apps and files will be lost (the SD card is kept).'],
  'avd.wiped': ['Dados apagados.', 'Data wiped.'],
  'avd.duplicate': ['Duplicar', 'Duplicate'], 'avd.duplicate.title': ['Duplicar dispositivo', 'Duplicate device'], 'avd.duplicate.label': ['Nome da cópia', 'Name of the copy'],
  'avd.duplicate.hint': ['Letras, números, ponto, hífen e sublinhado.', 'Letters, digits, dot, dash and underscore.'], 'avd.duplicated': ['Dispositivo duplicado.', 'Device duplicated.'],
  'avd.name.invalid': ['Use só letras, números, ponto, hífen e sublinhado.', 'Use only letters, digits, dot, dash and underscore.'],
  'avd.name.exists': ['Já existe um dispositivo com esse nome.', 'A device with this name already exists.'], 'avd.name.required': ['Informe um nome.', 'Enter a name.'],
  'log.title': ['Log de {name}', 'Log of {name}'], 'log.empty': ['(o log está vazio)', '(log is empty)'],

  // atalhos
  'shortcut.create': ['Criar atalho no menu', 'Create menu shortcut'], 'shortcut.create.btn': ['Criar atalho', 'Create shortcut'], 'shortcut.edit': ['Editar atalho do menu', 'Edit menu shortcut'],
  'shortcut.intro': ['Adiciona o emulador ao menu de aplicativos (GNOME, KDE…) para iniciar com um clique:', 'Adds the emulator to the application menu (GNOME, KDE…) so you can start it with one click:'],
  'shortcut.intro.mac': ['Cria um app em ~/Applications para iniciar o emulador:', 'Creates an app in ~/Applications to start the emulator:'],
  'shortcut.name': ['Nome no menu', 'Name in the menu'], 'shortcut.icon': ['Ícone', 'Icon'], 'shortcut.icon.choose': ['Escolher imagem…', 'Choose image…'],
  'shortcut.icon.default': ['Padrão', 'Default'], 'shortcut.pin': ['Fixar na dock do GNOME', 'Pin to the GNOME dock'],
  'shortcut.created': ['Atalho criado.', 'Shortcut created.'], 'shortcut.removed': ['Atalho removido.', 'Shortcut removed.'],

  // configuração inicial
  'setup.title': ['Vamos preparar o Android SDK', "Let's set up the Android SDK"],
  'setup.intro': ['O AVD Menu baixa e instala direto do Google o que o emulador precisa — não é preciso instalar Java nem o Android Studio.', 'AVD Menu downloads and installs what the emulator needs straight from Google — no Java or Android Studio required.'],
  'setup.sdk': ['Pasta do SDK', 'SDK folder'], 'setup.emulator': ['Android Emulator', 'Android Emulator'], 'setup.emulator.sub': ['Roda os dispositivos virtuais (~350 MB)', 'Runs the virtual devices (~350 MB)'],
  'setup.adb': ['Platform-Tools (adb)', 'Platform-Tools (adb)'], 'setup.adb.sub': ['Ponte de comunicação com os dispositivos (~10 MB)', 'Bridge to talk to devices (~10 MB)'],
  'setup.install': ['Instalar componentes', 'Install components'], 'setup.change': ['Mudar pasta do SDK…', 'Change SDK folder…'],

  // assistente
  'wiz.title': ['Criar dispositivo virtual', 'Create Virtual Device'], 'wiz.step.hw': ['Hardware', 'Hardware'], 'wiz.step.image': ['Imagem do sistema', 'System image'], 'wiz.step.config': ['Configuração', 'Configuration'],
  'wiz.hw.intro': ['Escolha o aparelho que será emulado.', 'Choose the device to emulate.'], 'wiz.search': ['Buscar aparelho…', 'Search device…'],
  'wiz.showold': ['Mostrar aparelhos antigos', 'Show old devices'], 'wiz.newprofile': ['Novo perfil de hardware', 'New hardware profile'],
  'wiz.col.name': ['Nome', 'Name'], 'wiz.col.size': ['Tela', 'Screen'], 'wiz.col.res': ['Resolução', 'Resolution'], 'wiz.col.density': ['Densidade', 'Density'],
  'wiz.custom': ['meu perfil', 'custom'], 'wiz.old': ['antigo', 'old'], 'wiz.nodevices': ['Nenhum aparelho nesta categoria.', 'No devices in this category.'],
  'cat.phone': ['Celular', 'Phone'], 'cat.tablet': ['Tablet', 'Tablet'], 'cat.wear': ['Wear OS', 'Wear OS'], 'cat.tv': ['TV', 'TV'], 'cat.automotive': ['Automotivo', 'Automotive'], 'cat.desktop': ['Desktop', 'Desktop'], 'cat.xr': ['XR', 'XR'],
  'profile.title': ['Novo perfil de hardware', 'New hardware profile'], 'profile.name': ['Nome', 'Name'], 'profile.category': ['Categoria', 'Category'],
  'profile.width': ['Largura (px)', 'Width (px)'], 'profile.height': ['Altura (px)', 'Height (px)'], 'profile.density': ['Densidade (dpi)', 'Density (dpi)'], 'profile.play': ['Tem Google Play', 'Has Google Play'],
  'wiz.image.intro': ['Escolha a versão do Android para {device}. Imagens ainda não instaladas são baixadas do Google.', 'Choose the Android version for {device}. Images not installed yet are downloaded from Google.'],
  'wiz.image.loading': ['Consultando as imagens disponíveis…', 'Looking up available images…'],
  'wiz.tab.rec': ['Recomendadas', 'Recommended'], 'wiz.tab.all': ['Todas compatíveis', 'All compatible'], 'wiz.tab.other': ['Outras', 'Other'],
  'wiz.col.release': ['Versão', 'Release'], 'wiz.col.target': ['Tipo', 'Target'], 'wiz.col.dl': ['Tamanho', 'Size'],
  'wiz.download': ['Baixar', 'Download'], 'wiz.installed': ['Instalada', 'Installed'], 'wiz.unavailable': ['Indisponível', 'Unavailable'],
  'wiz.slow': ['lenta', 'slow'], 'wiz.slow.tip': ['Esta arquitetura é emulada neste computador e fica bem mais lenta.', 'This architecture is emulated on this computer and is much slower.'],
  'wiz.incompat': ['incompatível', 'incompatible'], 'wiz.noimages': ['Nenhuma imagem nesta aba.', 'No images in this tab.'],
  'wiz.offline.cache': ['Sem internet: mostrando a última lista salva. Downloads novos não vão funcionar.', 'Offline: showing the last saved list. New downloads will not work.'],
  'wiz.offline.none': ['Sem acesso à lista de imagens do Google. Só as imagens já instaladas aparecem.', 'Cannot reach Google’s image list. Only installed images are shown.'],
  'wiz.sum.device': ['Aparelho', 'Device'], 'wiz.sum.image': ['Sistema', 'System'], 'wiz.change.device': ['Trocar aparelho', 'Change device'], 'wiz.change.image': ['Trocar imagem', 'Change image'],
  'wiz.shortcut': ['Criar atalho no menu de aplicativos', 'Create an application-menu shortcut'], 'wiz.shortcut.sub': ['Inicie este emulador direto do menu, com ícone próprio.', 'Start this emulator straight from the menu with its own icon.'],
  'wiz.startafter': ['Iniciar o emulador ao concluir', 'Start the emulator when done'],
  'wiz.created': ['Dispositivo “{name}” criado.', 'Device “{name}” created.'], 'wiz.create.failed': ['Não foi possível criar', 'Could not create'],

  // formulário
  'form.name': ['Nome do dispositivo', 'Device name'], 'form.orientation': ['Orientação inicial', 'Initial orientation'], 'form.portrait': ['Retrato', 'Portrait'], 'form.landscape': ['Paisagem', 'Landscape'],
  'form.boot': ['Inicialização', 'Startup'], 'form.boot.quick': ['Quick boot', 'Quick boot'], 'form.boot.cold': ['Cold boot', 'Cold boot'], 'form.boot.hint': ['Quick boot retoma o estado salvo; cold boot liga do zero.', 'Quick boot resumes the saved state; cold boot starts from scratch.'],
  'form.gpu': ['Gráficos', 'Graphics'], 'form.gpu.auto': ['Automático', 'Automatic'], 'form.gpu.host': ['Hardware', 'Hardware'], 'form.gpu.soft': ['Software', 'Software'],
  'form.gpu.hint': ['Automático escolhe a melhor opção; Software funciona em qualquer máquina, porém mais lento.', 'Automatic picks the best option; Software works anywhere but is slower.'],
  'form.gpu.hint.smart': ['Automático: neste computador o AVD Menu usa a placa de vídeo (-gpu host), porque o modo automático do emulador cairia para software (lento). Escolha Software se a tela ficar preta.', 'Automatic: on this computer AVD Menu uses the video card (-gpu host), because the emulator’s own automatic mode would fall back to slow software rendering. Pick Software if the screen goes black.'],
  'form.advanced': ['Configurações avançadas', 'Advanced settings'],
  'form.cam.front': ['Câmera frontal', 'Front camera'], 'form.cam.back': ['Câmera traseira', 'Back camera'], 'form.cam.none': ['Nenhuma', 'None'], 'form.cam.emulated': ['Emulada', 'Emulated'], 'form.cam.virtual': ['Cena virtual', 'Virtual scene'], 'form.cam.webcam': ['Webcam do computador', 'Computer webcam'],
  'form.net.speed': ['Velocidade da rede', 'Network speed'], 'form.net.latency': ['Latência da rede', 'Network latency'], 'form.net.full': ['Completa', 'Full'], 'form.lat.none': ['Nenhuma', 'None'],
  'form.ram': ['Memória RAM', 'RAM'], 'form.heap': ['Heap da VM', 'VM heap'], 'form.cores': ['Núcleos de CPU', 'CPU cores'],
  'form.storage': ['Armazenamento interno', 'Internal storage'], 'form.storage.hint': ['Só vale para dados novos; não encolhe um AVD existente.', 'Applies to fresh data only; does not shrink an existing AVD.'],
  'form.sd': ['Cartão SD', 'SD card'], 'form.sd.hint': ['0 = sem cartão SD', '0 = no SD card'],
  'form.keyboard': ['Usar o teclado do computador', 'Use the computer keyboard'], 'form.frame': ['Mostrar moldura do aparelho', 'Show device frame'],
  'form.id': ['ID do AVD', 'AVD ID'], 'form.id.hint': ['Nome usado em arquivos e na linha de comando (sem espaços).', 'Name used in files and on the command line (no spaces).'],

  // editor
  'edit.title': ['Editar {name}', 'Edit {name}'], 'edit.tab.settings': ['Configurações', 'Settings'], 'edit.tab.launch': ['Inicialização', 'Launch'],
  'edit.saved': ['Alterações salvas.', 'Changes saved.'], 'edit.noconfig': ['Não foi possível ler a configuração deste AVD.', 'Could not read this AVD’s configuration.'],
  'edit.raw.warn': ['Edição manual do config.ini. Um backup do arquivo anterior fica em config.ini.bak.', 'Manual config.ini editing. A backup of the previous file is kept as config.ini.bak.'],
  'edit.args': ['Argumentos extras do emulador', 'Extra emulator arguments'], 'edit.args.hint': ['Adicionados sempre que este dispositivo é iniciado pelo AVD Menu.', 'Added every time this device is started from AVD Menu.'],
  'edit.args.ex': ['Exemplos: -no-audio · -gpu host · -dns-server 8.8.8.8 · -memory 4096', 'Examples: -no-audio · -gpu host · -dns-server 8.8.8.8 · -memory 4096'],

  // SDK manager
  'sdk.title': ['SDK Manager', 'SDK Manager'], 'sdk.location': ['Local do Android SDK', 'Android SDK location'], 'sdk.change': ['Alterar', 'Change'],
  'sdk.tab.platforms': ['Plataformas do SDK', 'SDK Platforms'], 'sdk.tab.tools': ['Ferramentas do SDK', 'SDK Tools'],
  'sdk.showall': ['Mostrar pacotes antigos/incompatíveis', 'Show obsolete/incompatible packages'], 'sdk.preview': ['Canais beta/canary', 'Beta/canary channels'],
  'sdk.col.name': ['Nome', 'Name'], 'sdk.col.rev': ['Revisão', 'Revision'], 'sdk.col.status': ['Situação', 'Status'],
  'sdk.installed': ['Instalado', 'Installed'], 'sdk.notinstalled': ['Não instalado', 'Not installed'], 'sdk.update': ['Atualizar', 'Update'], 'sdk.update.avail': ['Atualização disponível', 'Update available'],
  'sdk.will.install': ['Será instalado', 'Will install'], 'sdk.will.update': ['Será atualizado', 'Will update'], 'sdk.will.remove': ['Será removido', 'Will remove'],
  'sdk.n.installed': ['{n} instalado(s)', '{n} installed'], 'sdk.n.versions': ['{n} versões', '{n} versions'], 'sdk.empty': ['Nada para mostrar.', 'Nothing to show.'],
  'sdk.offline.cache': ['Sem internet: mostrando a última lista salva.', 'Offline: showing the last saved list.'], 'sdk.offline.none': ['Sem acesso à lista de pacotes do Google. Só o que já está instalado aparece.', 'Cannot reach Google’s package list. Only installed items are shown.'],
  'sdk.bar.install': ['{n} para instalar ({size})', '{n} to install ({size})'], 'sdk.bar.remove': ['{n} para remover', '{n} to remove'],
  'sdk.remove.title': ['Remover pacotes', 'Remove packages'], 'sdk.remove.msg': ['Remover {n} pacote(s) do SDK?', 'Remove {n} SDK package(s)?'],
  'sdk.group.emulator': ['Android Emulator', 'Android Emulator'], 'sdk.group.platform-tools': ['Platform-Tools', 'Platform-Tools'], 'sdk.group.cmdline-tools': ['Command-line Tools', 'Command-line Tools'],
  'sdk.group.build-tools': ['Build-Tools', 'Build-Tools'], 'sdk.group.ndk': ['NDK', 'NDK'], 'sdk.group.cmake': ['CMake', 'CMake'], 'sdk.group.extras': ['Extras', 'Extras'], 'sdk.group.sources': ['Fontes', 'Sources'], 'sdk.group.other': ['Outros', 'Other'],
  'review.title': ['Revisar e instalar', 'Review and install'], 'review.intro': ['Estes pacotes serão baixados diretamente do Google:', 'These packages will be downloaded straight from Google:'],
  'review.total': ['Total: {size}', 'Total: {size}'], 'license.title': ['Licença: {id}', 'License: {id}'], 'license.accept': ['Li e aceito os termos da licença {id}', 'I have read and accept the terms of license {id}'],
  'install.failed': ['Falha ao instalar', 'Install failed'], 'uninstall.failed': ['Falha ao remover', 'Uninstall failed'],
  'uninstall.inuse.title': ['Imagem em uso', 'Image in use'], 'uninstall.inuse.msg': ['Estes dispositivos usam a imagem: {avds}. Eles deixarão de iniciar até você reinstalá-la.', 'These devices use the image: {avds}. They will not start until you reinstall it.'],
  'uninstall.anyway': ['Remover mesmo assim', 'Remove anyway'],
  'sdkloc.title': ['Local do Android SDK', 'Android SDK location'], 'sdkloc.intro': ['Escolha a pasta do SDK. Se ela não existir, será criada. Você pode reaproveitar o SDK do Android Studio.', 'Choose the SDK folder. It is created if missing. You can reuse Android Studio’s SDK.'],
  'sdkloc.path': ['Pasta', 'Folder'], 'sdkloc.found': ['SDKs encontrados', 'SDKs found'], 'sdkloc.browse': ['Navegar', 'Browse'], 'sdkloc.detected': ['SDK do Android detectado nesta pasta', 'Android SDK detected in this folder'],
  'sdkloc.default': ['Usar o padrão ({path})', 'Use the default ({path})'], 'sdkloc.use': ['Usar esta pasta', 'Use this folder'], 'sdkloc.saved': ['Local do SDK atualizado.', 'SDK location updated.'],

  // tarefas
  'tasks.title': ['Tarefas', 'Tasks'], 'tasks.active': ['{n} em andamento', '{n} running'], 'tasks.clear': ['Limpar', 'Clear'],
  'task.installing': ['Instalando', 'Installing'], 'task.removing': ['Removendo', 'Removing'], 'task.queued': ['Na fila…', 'Queued…'], 'task.done': ['Concluído', 'Done'], 'task.canceled': ['Cancelado', 'Canceled'], 'task.failed': ['Falhou', 'Failed'],
  'phase.download': ['Baixando', 'Downloading'], 'phase.verify': ['Verificando', 'Verifying'], 'phase.extract': ['Extraindo', 'Extracting'], 'phase.finalize': ['Finalizando', 'Finishing'], 'phase.remove': ['Removendo', 'Removing'],
  'task.finished.install': ['Instalação concluída: {title}', 'Install finished: {title}'], 'task.finished.uninstall': ['Removido: {title}', 'Removed: {title}'], 'task.finished.failed': ['Falhou: {title}', 'Failed: {title}'],

  // configurações
  'settings.title': ['Configurações', 'Settings'], 'settings.sdk': ['Android SDK', 'Android SDK'], 'settings.avdhome': ['Pasta dos AVDs', 'AVD folder'], 'settings.lang': ['Idioma', 'Language'], 'settings.theme': ['Tema', 'Theme'],
  'settings.auto': ['Automático', 'Automatic'], 'settings.light': ['Claro', 'Light'], 'settings.dark': ['Escuro', 'Dark'],
  'settings.self': ['AVD Menu no menu de aplicativos', 'AVD Menu in the application menu'], 'settings.self.on': ['O AVD Menu já aparece no menu de aplicativos.', 'AVD Menu already shows up in the application menu.'], 'settings.self.off': ['Adicione para abrir o AVD Menu como qualquer outro aplicativo.', 'Add it to open AVD Menu like any other application.'],
  'settings.self.add': ['Adicionar ao menu', 'Add to menu'], 'settings.self.remove': ['Remover do menu', 'Remove from menu'], 'settings.self.added': ['AVD Menu adicionado ao menu de aplicativos.', 'AVD Menu added to the application menu.'], 'settings.self.removed': ['AVD Menu removido do menu.', 'AVD Menu removed from the menu.'],
  'settings.self.appimage': ['Este AppImage deve continuar no mesmo lugar para o atalho funcionar.', 'Keep this AppImage where it is, otherwise the shortcut breaks.'],
  'settings.quit': ['Encerrar o AVD Menu', 'Quit AVD Menu'],

  // diagnóstico
  'health.title': ['Diagnóstico', 'Health check'], 'health.recheck': ['Verificar de novo', 'Check again'],
  'health.intro': ['O que o emulador precisa para rodar bem neste computador.', 'What the emulator needs to run well on this computer.'],
  'diag.sdk.ok': ['Android SDK encontrado', 'Android SDK found'], 'diag.sdk.ok.d': ['{path}', '{path}'],
  'diag.sdk.warn': ['A pasta do SDK ainda não existe', 'The SDK folder does not exist yet'], 'diag.sdk.warn.d': ['{path} será criada ao instalar os componentes.', '{path} will be created when you install the components.'],
  'diag.emulator.ok': ['Android Emulator {version}', 'Android Emulator {version}'], 'diag.emulator.warn': ['Android Emulator não instalado', 'Android Emulator not installed'], 'diag.emulator.warn.d': ['Instale-o no SDK Manager (aba Ferramentas).', 'Install it from the SDK Manager (Tools tab).'],
  'diag.adb.ok': ['Platform-Tools (adb) {version}', 'Platform-Tools (adb) {version}'], 'diag.adb.warn': ['Platform-Tools (adb) não instalado', 'Platform-Tools (adb) not installed'], 'diag.adb.warn.d': ['Necessário para parar emuladores com segurança e instalar apps.', 'Needed to stop emulators cleanly and install apps.'],
  'diag.arch.error': ['O emulador não existe para Linux {arch}', 'The emulator is not available for Linux {arch}'], 'diag.arch.error.d': ['O Google só publica o Android Emulator para Linux x86_64.', 'Google only ships the Android Emulator for Linux x86_64.'],
  'diag.accel.ok': ['Aceleração por hardware ativa', 'Hardware acceleration enabled'], 'diag.accel.ok.d': ['{info}', '{info}'],
  'diag.accel.error': ['Aceleração por hardware indisponível', 'Hardware acceleration unavailable'], 'diag.accel.error.d': ['{info}. Sem ela o emulador não inicia imagens x86.', '{info}. Without it the emulator cannot run x86 images.'],
  'diag.libs.ok': ['Bibliotecas do sistema completas', 'System libraries complete'], 'diag.libs.error': ['Faltam bibliotecas do sistema', 'System libraries are missing'], 'diag.libs.error.d': ['{libs}', '{libs}'],
  'diag.gpu.info': ['{count} GPU(s) detectada(s)', '{count} GPU(s) detected'], 'diag.gpu.info.d': ['{gpus}', '{gpus}'],
  'diag.render_host.info': ['Gráficos “Automático”: usando a placa de vídeo (-gpu host)', 'Graphics “Automatic”: using the video card (-gpu host)'],
  'diag.render_host.info.d': ['O modo automático do próprio emulador costuma cair para renderização por software (lenta — a janela “não responde”) mesmo com GPU boa. Nesta máquina o AVD Menu inicia com -gpu host.', 'The emulator’s own automatic mode often falls back to software rendering (slow — the window “stops responding”) even with a good GPU. On this machine AVD Menu starts with -gpu host.'],
  'diag.render_auto.info': ['Gráficos “Automático”: decidido pelo emulador', 'Graphics “Automatic”: decided by the emulator'],
  'diag.session.info': ['Sessão gráfica: {type}', 'Display session: {type}'], 'diag.session.info.d': ['O emulador roda via XWayland/X11; no Wayland o atalho se vincula pela classe da janela.', 'The emulator runs through XWayland/X11; on Wayland the shortcut binds via the window class.'],
  'diag.disk.ok': ['Espaço em disco: {free} livres', 'Disk space: {free} free'], 'diag.disk.warn': ['Pouco espaço em disco: {free} livres', 'Low disk space: {free} free'], 'diag.disk.warn.d': ['Imagens de sistema ocupam de 2 a 6 GB cada.', 'System images take 2–6 GB each.'],
};

let lang = 'pt';

export function currentLang() { return lang; }

/** Idioma pedido (`pref`, vazio = automático) ou, na falta, o do navegador/sistema; senão inglês. */
export function detectLang(pref) {
  const known = (c) => LANGUAGES.find((l) => l.code === String(c || '').toLowerCase().split(/[-_]/)[0]);
  if (pref && known(pref)) return known(pref).code;
  for (const c of navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language]) {
    if (known(c)) return known(c).code;
  }
  return 'en';
}

export function initLang(pref) {
  lang = detectLang(pref);
  const l = LANGUAGES.find((x) => x.code === lang);
  document.documentElement.lang = l.html;
  document.documentElement.dir = l.rtl ? 'rtl' : 'ltr';
}

export async function setLang(pref) {
  const { post } = await import('./api.js');
  await post('/api/settings', { lang: pref });
  initLang(pref);
  post('/api/lang', { lang: lang }).catch(() => {});
  emit('lang');
}

/** Traduz a chave, trocando {var} pelos valores de vars. Chave ausente = a própria chave. */
export function t(key, vars) {
  const e = S[key];
  let s = !e ? key : lang === 'pt' ? e[0] : lang === 'en' ? e[1] : (PACKS[lang] && PACKS[lang][key]) || e[1];
  if (vars) s = s.replace(/\{(\w+)\}/g, (_, k) => (vars[k] !== undefined ? vars[k] : ''));
  return s;
}
