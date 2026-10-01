# AVD Menu

Gerenciador de emuladores Android para Linux e macOS — o **Device Manager + SDK Manager do Android Studio** num aplicativo leve, **sem Java e sem instalar dependências**.

Lista seus emuladores, inicia/para com um clique, cria dispositivos novos (escolhendo aparelho e versão do Android), baixa as imagens direto do Google e cria o **atalho no menu de aplicativos** (GNOME, KDE…) com ícone e ações de clique-direito.

Feito em **Rust + Tauri** (janela nativa com o WebView do sistema) e **JS puro** só na interface. Não há Go, Python nem Node no projeto: scripts em **bash**.

> *English TL;DR:* a dependency-free Android emulator manager for Linux & macOS. Rust core + Tauri shell (native window, native title bar), plain-JS front-end, bash scripts. Talks to Google's SDK repository directly (no Java, no `sdkmanager`), creates AVDs the way Android Studio does, and adds launcher shortcuts to your desktop menu. The UI, CLI and menu shortcuts are localized in 11 languages (see *Idiomas*).

## Instalação

**Linux (qualquer distribuição x86_64)** — uma linha:

```sh
curl -fsSL https://raw.githubusercontent.com/thiagotognoli/avd-menu/main/install.sh | sh
```

Ou baixe em [Releases](https://github.com/thiagotognoli/avd-menu/releases):

| Arquivo | Para quê |
|---|---|
| `AVD-Menu-v*-linux-x86_64.AppImage` | roda em qualquer distro; traz o WebKitGTK dentro (não instala nada) |
| `AVD-Menu-v*-linux-amd64.deb` | Debian/Ubuntu (usa o `libwebkit2gtk-4.1` do sistema) |
| `avd-menu-v*-linux-x86_64.tar.gz` | binário solto (usa o WebKitGTK do sistema) |
| `AVD-Menu-v*-macos.dmg` | macOS 11+, Intel e Apple Silicon (binário universal) |

No macOS o app não é notarizado pela Apple: na primeira vez use **botão direito → Abrir** (ou `xattr -dr com.apple.quarantine "/Applications/AVD Menu.app"`; o `install.sh` já faz isso). O AppImage precisa de FUSE (`libfuse2`/`fuse3`); sem ele rode com `--appimage-extract-and-run`.

Para aparecer no menu de aplicativos do Linux: *Configurações (engrenagem) → Adicionar ao menu*, ou `avd-menu install`.

## Primeiro uso

1. Abra o AVD Menu. Sem Android SDK ele mostra um cartão **“Vamos preparar o Android SDK”**: um clique baixa o *Android Emulator* e o *Platform-Tools* (≈ 330 MB) para `~/Applications/AndroidSDK` (ou use o SDK do Android Studio — ele é detectado sozinho).
2. **Criar dispositivo** → escolha o aparelho (Pixel 10, Pixel Fold, tablets, Wear OS, TV, Automotive…) → a versão do Android (imagens não instaladas têm botão *Baixar*) → ajuste as opções e *Concluir*.
3. **Iniciar**. No menu `⋮`: *cold boot*, *apagar dados*, *GPU dedicada*, *sem janela*, editar, duplicar, log, **criar atalho no menu**, excluir.

## O que tem

| | |
|---|---|
| **Device Manager** | lista com estado em tempo real, iniciar/parar, cold boot, wipe data, duplicar, editar (RAM, heap, CPUs, armazenamento, cartão SD, câmeras, rede, gráficos, teclado, moldura), editor de `config.ini`, argumentos extras do emulador por AVD, log da execução |
| **Criar dispositivo** | ~95 perfis de hardware oficiais (as mesmas definições do Android Studio) + perfis próprios; imagens *Recomendadas / Todas compatíveis / Outras* filtradas pelo tipo de aparelho e pela arquitetura do computador |
| **SDK Manager** | plataformas, imagens de sistema, emulador, platform-tools, build-tools, cmdline-tools, NDK, CMake… instalar/atualizar/remover com download retomável, checagem SHA-1 e licenças |
| **Atalhos** | Linux: `.desktop` com `StartupWMClass` (a janela do emulador se vincula ao ícone na dock), ações *cold boot / apagar dados / GPU dedicada*, ícone próprio, fixar na dock do GNOME. macOS: `.app` em `~/Applications` |
| **Gráficos “Automático” de verdade** | o modo `-gpu auto` do emulador costuma cair para renderização por **software** em Linux com GPU boa (“Your GPU drivers may have a bug. Switching to software rendering”) — a janela do emulador fica lenta e “não responde”. O AVD Menu detecta driver de hardware (Mesa radv/anv, NVIDIA…) e inicia com `-gpu host`; a aba Diagnóstico mostra o que está sendo usado |
| **Moldura do aparelho** | o emulador só desenha a moldura (Pixel, Nexus, Wear OS, TV…) se o AVD aponta uma *skin* em `skin.path`; `showDeviceFrame` ele ignora. O AVD Menu baixa a skin do aparelho (espelho do Studio no AOSP, revisão fixa, cada arquivo conferido pelo hash do git) para `<sdk>/skins/<nome>`, onde o Studio também as guarda, e liga/desliga a moldura de verdade (`skin.path=_no_skin`). Aparelhos genéricos (*Medium Phone*, *Resizable*…) não têm moldura. `AVD_MENU_SKINS_URL` troca a origem do download |
| **Diagnóstico** | KVM/Hypervisor, bibliotecas faltando (com o comando do `apt`/`dnf`/`pacman`), GPUs, modo gráfico, espaço em disco |
| **Compatível com o Android Studio** | usa `~/.android/avd` e grava os mesmos `package.xml` e `licenses/` — AVDs e SDK criados aqui aparecem no Studio e no `sdkmanager`, e vice-versa |

## Idiomas

A interface, as mensagens, a ajuda da linha de comando e os itens dos atalhos do menu estão traduzidos em 11 idiomas: **Português (Brasil)**, **English**, **中文（简体）**, **हिन्दी**, **Español**, **Français**, **العربية**, **বাংলা**, **Русский**, **اردو** e **Bahasa Indonesia**. Árabe e urdu são exibidos da direita para a esquerda.

O idioma segue o do sistema (`LANG`/navegador); para trocar, use *Configurações → Idioma*. Texto sem tradução cai para o inglês.

Para **acrescentar ou corrigir um idioma** (`xx` = código ISO 639-1):

1. `ui/js/lang/xx.js` — copie um existente e traduza os valores (uma entrada por linha; mantenha as chaves e as `{variáveis}`); registre o idioma em `LANGUAGES` e no `import` de `ui/js/i18n.js` (`rtl: true` se for escrito da direita para a esquerda).
2. `crates/core/data/i18n/xx.json` — mensagens do núcleo e da linha de comando: *texto em inglês → tradução* (mantenha `{}`, `{:?}` e `{v}`; para mudar a ordem use `{0}`, `{1}`…); registre em `LANGS` e em `raw_catalog` (`crates/core/src/lang.rs`).
3. `./x test` confere se não falta chave e se as variáveis batem com o original.

## Linha de comando

O mesmo executável:

```sh
avd-menu                         # abre a interface
avd-menu list                    # lista os AVDs
avd-menu start Pixel_9 --cold    # inicia (--wipe, --dgpu, --headless, --arg ...)
avd-menu stop  Pixel_9
avd-menu create Meu_Pixel --device pixel_9 --image "system-images;android-36;google_apis;x86_64"
avd-menu shortcut Pixel_9 --name "Meu Pixel" --icon ~/icone.png --pin
avd-menu sdk list --all          # tudo que o Google publica
avd-menu sdk install "system-images;android-36;google_apis_playstore;x86_64"
avd-menu doctor                  # diagnóstico
```

Compatível com o antigo `avd-menu.sh`: `avd-menu NOME` cria o atalho; `--ui`, `--list`, `--remove`, `install` continuam funcionando. Em servidores/CI há também o `avd-menu-cli`, sem janela e sem dependência de GTK/WebKit.

## Como funciona

- **Sem Java**: o núcleo em Rust lê os manifestos públicos do Google (`dl.google.com/android/repository`), baixa os `.zip` (retomável, confere SHA-1), extrai e grava o `package.xml` — exatamente o que o `sdkmanager` faz. O hash das licenças usa o mesmo algoritmo do sdklib, então o aceite vale para o Studio também.
- **AVDs sem `avdmanager`**: o `config.ini` é montado com as mesmas chaves que o `avdmanager` gera para cada aparelho (tabela extraída do próprio `avdmanager` por `cargo xtask gen-devices`).
- **Janela nativa**: Tauri usa o WebView do sistema (WebKitGTK no Linux, WKWebView no macOS), então a janela tem a barra de título do sistema — nada de “janela do Chrome”. A interface conversa com o núcleo por IPC do Tauri (sem servidor HTTP, sem porta aberta); fechar a janela encerra o programa na hora e os emuladores seguem rodando.

### Requisitos

- Linux x86_64 (o Google não publica o emulador para Linux ARM) ou macOS 11+.
- Linux: acesso ao KVM (`ls -l /dev/kvm`; se faltar: `sudo usermod -aG kvm $USER` e relogar). A aba **Diagnóstico** diz o que falta.

## Desenvolvimento

Tudo é instalado **dentro do projeto** (`.tools/`), sem sudo:

```sh
./x setup          # Rust, cargo-tauri e (se faltarem) as bibliotecas -dev do WebKitGTK
./x run            # compila e abre o app (modo debug)
./x ui             # só a interface num navegador, com o núcleo real por trás (devserver)
./x test           # cargo fmt --check + clippy + testes
./x build          # release em target/release/avd-menu
./x dist [versão]  # AppImage + .deb (Linux) ou .dmg (macOS) em dist/
./x gen-devices    # regenera a lista de aparelhos a partir de um SDK (precisa de JDK)
```

Para testar sem mexer no seu SDK nem nos seus AVDs, aponte tudo para uma pasta temporária (e use `--headless` para não abrir janela de emulador):

```sh
T=$(mktemp -d)
export HOME=$T XDG_CONFIG_HOME=$T/cfg XDG_CACHE_HOME=$T/cache XDG_STATE_HOME=$T/state XDG_DATA_HOME=$T/data
export ANDROID_AVD_HOME=$T/avd ANDROID_ADB_SERVER_PORT=5097   # outra porta: não derruba o adb que já está rodando
target/debug/avd-menu --sdk $T/sdk create Teste --device pixel_9 --image "system-images;android-36;default;x86_64"
target/debug/avd-menu --sdk $T/sdk start Teste --headless
```

```
crates/core/         núcleo (avdcore): sdk/ (substitui o sdkmanager), avd/ (avdmanager), emu/, shortcut/, api/, cli.rs
  data/devices.json  perfis de hardware (gerado por `cargo xtask gen-devices`)
src-tauri/           shell do Tauri: janela + comando `api` + eventos
ui/                  interface (HTML/CSS/JS puro, sem build)
xtask/               tarefas de manutenção em Rust (gen-devices)
scripts/ + x         bash: setup, bibliotecas -dev, versão, empacotamento
```

### Publicar uma release

O workflow `.github/workflows/release.yml` compila tudo e cria a release:

```sh
git tag v1.0.0 && git push origin v1.0.0
```

## Problemas comuns

| Sintoma | Solução |
|---|---|
| “Aceleração por hardware indisponível” | `sudo usermod -aG kvm $USER` e relogar; confira VT-x/AMD-V na BIOS |
| O emulador fecha logo ao iniciar | *Ver log* no menu `⋮` do dispositivo (ou aba Diagnóstico → bibliotecas faltando) |
| O emulador está lento / “não está respondendo” | Diagnóstico → *Gráficos*. Se o modo estiver em software, edite o AVD → Gráficos → **Hardware** |
| Atalho criado por outra versão continua lento | recrie-o (menu `⋮` → *Criar atalho no menu*): só os atalhos novos levam o `-gpu host` automático |
| Notebook com 2 GPUs lento | menu `⋮` → *Iniciar com GPU dedicada* (o atalho do menu tem a mesma ação) |
| Janela em branco no Linux (NVIDIA) | o app já define `WEBKIT_DISABLE_DMABUF_RENDERER=1`; se persistir, abra uma issue com `avd-menu` rodando no terminal |

## Histórico

A primeira versão era um script `avd-menu.sh` (zenity); depois uma versão em Go. Ambas continuam em `backup/` (`avd-menu-go-v1.tar.gz` guarda a implementação em Go inteira). O `avd-menu.sh` segue funcionando e os atalhos que ele cria são reconhecidos pelo app. Quem usou a versão em Go pode apagar `~/.cache/avd-menu/browser-profile` (perfil do Chrome que ela usava).

## Créditos e licenças

- As definições de dispositivos vêm do AOSP/Android Studio (Apache License 2.0). “Android” e o robô são marcas do Google; o ícone do projeto é do SVG Repo.
- O AVD Menu não redistribui nada do SDK: tudo é baixado do Google na hora, mediante aceite das licenças.
