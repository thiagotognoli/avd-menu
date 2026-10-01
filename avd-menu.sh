#!/usr/bin/env bash
#
# avd-menu.sh — cria um atalho (.desktop) no menu de aplicativos do Ubuntu
#               para iniciar um emulador Android (AVD).
#
# Uso:
#   ./avd-menu.sh [OPÇÕES] NOME_DO_AVD
#   ./avd-menu.sh ui                      # interface gráfica (zenity/GTK)
#   ./avd-menu.sh install                 # coloca o próprio AVD Menu no menu
#
# O nome do AVD é obrigatório no modo linha de comando.
# Se o ícone não for informado, usa ./android-icon.svg (ao lado do script).
#
# NOTA: este script foi substituído pelo AVD Menu (app em Rust + Tauri, sem dependências,
# com interface própria, SDK Manager e criação de emuladores) — veja o README.md.
# Ele continua funcionando e os atalhos que ele cria são reconhecidos pelo app.

set -euo pipefail

# --------------------------------------------------------------------------
# Localização do próprio script (resolve symlinks)
# --------------------------------------------------------------------------
SCRIPT_PATH="$(readlink -f "${BASH_SOURCE[0]}")"
SCRIPT_DIR="$(dirname "$SCRIPT_PATH")"
SCRIPT_NAME="$(basename "$SCRIPT_PATH")"

# --------------------------------------------------------------------------
# Padrões
# --------------------------------------------------------------------------
DEFAULT_ICON="$SCRIPT_DIR/android-icon.svg"
MENU_DESKTOP_ID="avd-menu"
MENU_NAME="AVD Menu"

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="$DATA_HOME/applications"
ICONS_DIR="$DATA_HOME/icons/hicolor"

UI_MODE=0
ACTION="install"
AVD_NAME=""
ICON_PATH=""
DISPLAY_NAME=""

# --------------------------------------------------------------------------
# Saída / mensagens (adaptam-se ao modo gráfico)
# --------------------------------------------------------------------------
info() { printf '%s\n' "$*"; }

die() {
    local msg="$*"
    if [ "$UI_MODE" -eq 1 ] && command -v zenity >/dev/null 2>&1; then
        zenity --error --width=420 \
               --title="AVD Menu" \
               --text="$msg" 2>/dev/null || true
    fi
    printf 'Erro: %s\n' "$msg" >&2
    exit 1
}

usage() {
    cat <<EOF
$SCRIPT_NAME — cria um atalho no menu de aplicativos para um emulador Android.

USO:
  $SCRIPT_NAME [OPÇÕES] NOME_DO_AVD     Cria o atalho de um emulador
  $SCRIPT_NAME ui                       Abre a interface gráfica
  $SCRIPT_NAME install                  Adiciona o "$MENU_NAME" ao menu do Linux

OPÇÕES:
  -a, --avd NOME        Nome do AVD (obrigatório na linha de comando)
  -i, --icon CAMINHO    Ícone do atalho (.svg/.png/... — padrão: android-icon.svg)
  -n, --name TEXTO      Nome exibido no menu (padrão: "Android Emulator NOME")
  -u, --ui              Interface gráfica (GTK via zenity)
  -l, --list            Lista os AVDs disponíveis e sai
  -r, --remove          Remove o atalho do AVD informado
      --install         Cria um atalho para este script com --ui
      --uninstall       Remove o atalho do "$MENU_NAME"
  -h, --help            Mostra esta ajuda

EXEMPLOS:
  $SCRIPT_NAME DEV
  $SCRIPT_NAME Pixel_9_Pro_XL
  $SCRIPT_NAME --icon ~/Imagens/meu.png DEV
  $SCRIPT_NAME ui
  $SCRIPT_NAME install
  $SCRIPT_NAME --remove DEV
EOF
}

# --------------------------------------------------------------------------
# Parse dos argumentos
# --------------------------------------------------------------------------
while [ $# -gt 0 ]; do
    case "$1" in
        ui|-u|--ui)     UI_MODE=1 ;;
        install|--install)   ACTION="install-menu" ;;
        --uninstall)         ACTION="uninstall-menu" ;;
        -a|--avd)       [ $# -ge 2 ] || die "--avd exige um valor."; AVD_NAME="$2"; shift ;;
        -i|--icon)      [ $# -ge 2 ] || die "--icon exige um caminho."; ICON_PATH="$2"; shift ;;
        -n|--name)      [ $# -ge 2 ] || die "--name exige um valor."; DISPLAY_NAME="$2"; shift ;;
        -l|--list)      ACTION="list" ;;
        -r|--remove)    ACTION="remove" ;;
        -h|--help)      usage; exit 0 ;;
        --)             shift; [ $# -gt 0 ] && AVD_NAME="$1" ;;
        -*)             die "Opção desconhecida: $1 (use --help)" ;;
        *)              AVD_NAME="$1" ;;
    esac
    shift
done

# --------------------------------------------------------------------------
# Localiza o Android SDK e o binário do emulador
# --------------------------------------------------------------------------
find_sdk_root() {
    local candidate
    for candidate in \
        "${ANDROID_SDK_ROOT:-}" \
        "${ANDROID_HOME:-}" \
        "$HOME/Android/Sdk" \
        "$HOME/Applications/Android/Sdk" \
        "$HOME/.local/share/Android/Sdk" \
        "/opt/android-sdk" \
        "/usr/lib/android-sdk"
    do
        if [ -n "$candidate" ] && [ -x "$candidate/emulator/emulator" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    # Último recurso: emulator no PATH
    if command -v emulator >/dev/null 2>&1; then
        local bin
        bin="$(readlink -f "$(command -v emulator)")"
        printf '%s\n' "$(dirname "$(dirname "$bin")")"
        return 0
    fi

    return 1
}

SDK_ROOT="$(find_sdk_root || true)"
[ -n "$SDK_ROOT" ] || die "Android SDK não encontrado. Defina ANDROID_SDK_ROOT ou instale o SDK."

EMULATOR_BIN="$SDK_ROOT/emulator/emulator"
[ -x "$EMULATOR_BIN" ] || EMULATOR_BIN="$(command -v emulator || true)"
[ -n "$EMULATOR_BIN" ] && [ -x "$EMULATOR_BIN" ] \
    || die "Binário do emulador não encontrado em $SDK_ROOT/emulator/emulator."

# --------------------------------------------------------------------------
# Lista de AVDs disponíveis
# --------------------------------------------------------------------------
list_avds() {
    # O emulador pode emitir linhas "INFO | ..." — só nomes válidos interessam.
    "$EMULATOR_BIN" -list-avds 2>/dev/null | grep -E '^[A-Za-z0-9._-]+$' || true
}

AVDS="$(list_avds)"

if [ "$ACTION" = "list" ]; then
    [ -n "$AVDS" ] || die "Nenhum AVD encontrado. Crie um pelo Android Studio ou avdmanager."
    info "AVDs disponíveis:"
    printf '  - %s\n' $AVDS
    exit 0
fi

avd_exists() {
    printf '%s\n' "$AVDS" | grep -qxF "$1"
}

# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------
slugify() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed -e 's/[^a-z0-9]\+/-/g' -e 's/^-//' -e 's/-$//'
}

# Variáveis que forçam o uso da GPU dedicada, ou vazio se a máquina só tem uma.
#
# O emulador já escolhe a GPU discreta para o Vulkan, mas o caminho OpenGL ES
# continua na GPU padrão (a integrada, em notebooks híbridos) — daí a diferença
# de desempenho. As variáveis abaixo são as mesmas que o switcheroo-control usa.
#
# Não dá para juntar as duas famílias: em máquina AMD/Intel, definir
# __GLX_VENDOR_LIBRARY_NAME=nvidia faz o libglvnd procurar libGLX_nvidia.so.0 e
# quebrar o GLX. Por isso a escolha é feita aqui, na criação do atalho.
detect_dgpu_env() {
    local nodes
    nodes="$(ls -1d /dev/dri/renderD* 2>/dev/null | wc -l)"
    [ "${nodes:-0}" -gt 1 ] || return 1

    if [ -d /proc/driver/nvidia ] \
       || { command -v lspci >/dev/null 2>&1 && lspci 2>/dev/null | grep -qi nvidia; }; then
        printf '%s' '__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia __VK_LAYER_NV_optimus=NVIDIA_only'
    else
        printf '%s' 'DRI_PRIME=1'
    fi
}

# Instala o ícone no tema hicolor do usuário e devolve o nome a usar em Icon=.
# Se não conseguir instalar, devolve o caminho absoluto do arquivo original.
install_icon() {
    local src="$1" icon_name="$2" ext dest
    src="$(readlink -f "$src")"
    ext="${src##*.}"
    ext="$(printf '%s' "$ext" | tr '[:upper:]' '[:lower:]')"

    case "$ext" in
        svg|svgz)
            dest="$ICONS_DIR/scalable/apps/$icon_name.svg"
            mkdir -p "$(dirname "$dest")"
            cp -f "$src" "$dest"
            ;;
        png)
            dest="$ICONS_DIR/256x256/apps/$icon_name.png"
            mkdir -p "$(dirname "$dest")"
            cp -f "$src" "$dest"
            ;;
        *)
            if command -v convert >/dev/null 2>&1; then
                dest="$ICONS_DIR/256x256/apps/$icon_name.png"
                mkdir -p "$(dirname "$dest")"
                convert "$src" -resize 256x256 "$dest" 2>/dev/null || {
                    printf '%s\n' "$src"; return 0
                }
            else
                printf '%s\n' "$src"
                return 0
            fi
            ;;
    esac

    gtk-update-icon-cache -f -t "$ICONS_DIR" >/dev/null 2>&1 || true
    printf '%s\n' "$icon_name"
}

# --------------------------------------------------------------------------
# Atalho do próprio AVD Menu (opções "install" / "--uninstall")
# --------------------------------------------------------------------------
MENU_DESKTOP_FILE="$APPS_DIR/$MENU_DESKTOP_ID.desktop"

if [ "$ACTION" = "uninstall-menu" ]; then
    [ -f "$MENU_DESKTOP_FILE" ] || die "O \"$MENU_NAME\" não está instalado no menu."
    rm -f "$MENU_DESKTOP_FILE"
    rm -f "$ICONS_DIR/scalable/apps/$MENU_DESKTOP_ID.svg" \
          "$ICONS_DIR/256x256/apps/$MENU_DESKTOP_ID.png" 2>/dev/null || true
    update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
    gtk-update-icon-cache -f -t "$ICONS_DIR" >/dev/null 2>&1 || true
    info "Atalho removido: $MENU_DESKTOP_FILE"
    exit 0
fi

if [ "$ACTION" = "install-menu" ]; then
    command -v zenity >/dev/null 2>&1 \
        || die "zenity não está instalado (necessário para o modo gráfico).
Instale com: sudo apt install zenity"

    MENU_ICON="${ICON_PATH:-$DEFAULT_ICON}"
    [ -f "$MENU_ICON" ] || die "Ícone não encontrado: $MENU_ICON"
    [ -r "$MENU_ICON" ] || die "Ícone sem permissão de leitura: $MENU_ICON"

    # Garante que o lançador consiga executar o script diretamente.
    LAUNCH_CMD="\"$SCRIPT_PATH\" --ui"
    if [ ! -x "$SCRIPT_PATH" ]; then
        chmod +x "$SCRIPT_PATH" 2>/dev/null || LAUNCH_CMD="bash \"$SCRIPT_PATH\" --ui"
    fi

    MENU_ICON_VALUE="$(install_icon "$MENU_ICON" "$MENU_DESKTOP_ID")"

    mkdir -p "$APPS_DIR"
    cat > "$MENU_DESKTOP_FILE" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=${DISPLAY_NAME:-$MENU_NAME}
GenericName=Atalhos de emuladores Android
Comment=Cria atalhos no menu para os emuladores Android (AVD)
Exec=$LAUNCH_CMD
Icon=$MENU_ICON_VALUE
Terminal=false
Categories=Development;IDE;
Keywords=android;emulator;avd;atalho;launcher;
StartupNotify=true
EOF

    chmod 644 "$MENU_DESKTOP_FILE"
    update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true

    if command -v desktop-file-validate >/dev/null 2>&1; then
        desktop-file-validate "$MENU_DESKTOP_FILE" >/dev/null 2>&1 \
            || info "Aviso: desktop-file-validate reportou avisos em $MENU_DESKTOP_FILE"
    fi

    MSG="\"${DISPLAY_NAME:-$MENU_NAME}\" adicionado ao menu de aplicativos.

Comando:  $LAUNCH_CMD
Ícone:    $MENU_ICON_VALUE
Arquivo:  $MENU_DESKTOP_FILE"

    if [ "$UI_MODE" -eq 1 ] && command -v zenity >/dev/null 2>&1; then
        zenity --info --width=460 --title="$MENU_NAME" --text="$MSG" 2>/dev/null || true
    fi
    info "$MSG"
    exit 0
fi

# --------------------------------------------------------------------------
# Interface gráfica (GTK via zenity)
# --------------------------------------------------------------------------
run_ui() {
    command -v zenity >/dev/null 2>&1 \
        || die "zenity não está instalado. Instale com: sudo apt install zenity"

    [ -n "$AVDS" ] || die "Nenhum AVD encontrado. Crie um pelo Android Studio ou avdmanager."

    local combo_values result chosen_avd chosen_name icon_choice
    combo_values="$(printf '%s\n' $AVDS | paste -sd'|' -)"

    local icon_default_label="Padrão ($(basename "$DEFAULT_ICON"))"
    local icon_pick_label="Escolher arquivo..."
    local icon_options="$icon_default_label|$icon_pick_label"
    [ -f "$DEFAULT_ICON" ] || icon_options="$icon_pick_label"

    result="$(zenity --forms \
        --title="AVD Menu" \
        --text="Criar atalho para um emulador Android" \
        --separator="|" \
        --add-combo="Emulador (AVD)" --combo-values="$combo_values" \
        --add-entry="Nome no menu (opcional)" \
        --add-combo="Ícone" --combo-values="$icon_options" \
        2>/dev/null)" || exit 0

    chosen_avd="$(printf '%s' "$result"  | cut -d'|' -f1)"
    chosen_name="$(printf '%s' "$result" | cut -d'|' -f2)"
    icon_choice="$(printf '%s' "$result" | cut -d'|' -f3)"

    [ -n "$chosen_avd" ] || die "Nenhum emulador foi selecionado."

    AVD_NAME="$chosen_avd"
    [ -n "$chosen_name" ] && DISPLAY_NAME="$chosen_name"

    if [ "$icon_choice" = "$icon_pick_label" ]; then
        ICON_PATH="$(zenity --file-selection \
            --title="Selecione o ícone" \
            --file-filter="Imagens | *.svg *.svgz *.png *.jpg *.jpeg *.xpm *.ico" \
            --file-filter="Todos os arquivos | *" \
            2>/dev/null)" || exit 0
    else
        ICON_PATH="$DEFAULT_ICON"
    fi
}

[ "$UI_MODE" -eq 1 ] && [ "$ACTION" = "install" ] && run_ui

# --------------------------------------------------------------------------
# Validações
# --------------------------------------------------------------------------
if [ -z "$AVD_NAME" ]; then
    HINT=""
    if [ -n "$AVDS" ]; then
        HINT="

AVDs disponíveis:
$(printf '  - %s\n' $AVDS)"
    fi
    die "Nenhum emulador informado — o nome do AVD é obrigatório.
Uso: $SCRIPT_NAME [OPÇÕES] NOME_DO_AVD   (ou '$SCRIPT_NAME ui' para escolher na tela)$HINT"
fi

ICON_PATH="${ICON_PATH:-$DEFAULT_ICON}"

SLUG="$(slugify "$AVD_NAME")"
DESKTOP_ID="android-emulator-$SLUG"
DESKTOP_FILE="$APPS_DIR/$DESKTOP_ID.desktop"
ICON_NAME="$DESKTOP_ID"

# O GNOME vincula a janela ao atalho comparando o WM_CLASS com StartupWMClass.
# Sem isso o emulador abre um segundo ícone "genérico" na dock em vez de marcar
# o ícone dos favoritos como em execução.
#
# Por padrão o emulador (Qt6/xcb) usa WM_CLASS = "qemu-system-x86_64","Emulator"
# — igual para todos os AVDs. A variável RESOURCE_NAME, lida pelo plugin xcb do
# Qt, troca a parte "instância" do WM_CLASS, dando um nome único por AVD.
WM_CLASS_NAME="$DESKTOP_ID"

# --- Remoção -------------------------------------------------------------
if [ "$ACTION" = "remove" ]; then
    removed=0
    if [ -f "$DESKTOP_FILE" ]; then
        rm -f "$DESKTOP_FILE"
        info "Atalho removido: $DESKTOP_FILE"
        removed=1
    fi
    rm -f "$ICONS_DIR/scalable/apps/$ICON_NAME.svg" \
          "$ICONS_DIR/256x256/apps/$ICON_NAME.png" 2>/dev/null || true
    update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
    gtk-update-icon-cache -f -t "$ICONS_DIR" >/dev/null 2>&1 || true
    [ "$removed" -eq 1 ] || die "Nenhum atalho encontrado para o AVD '$AVD_NAME'."
    exit 0
fi

# --- AVD existe? ---------------------------------------------------------
[ -n "$AVDS" ] || die "Nenhum AVD encontrado. Crie um pelo Android Studio ou avdmanager."

if ! avd_exists "$AVD_NAME"; then
    die "O emulador '$AVD_NAME' não existe.
AVDs disponíveis:
$(printf '  - %s\n' $AVDS)"
fi

# --- Ícone existe? -------------------------------------------------------
[ -f "$ICON_PATH" ] || die "Ícone não encontrado: $ICON_PATH"
[ -r "$ICON_PATH" ] || die "Ícone sem permissão de leitura: $ICON_PATH"

DISPLAY_NAME="${DISPLAY_NAME:-Android Emulator $AVD_NAME}"

# --------------------------------------------------------------------------
# Instalação
# --------------------------------------------------------------------------
ICON_VALUE="$(install_icon "$ICON_PATH" "$ICON_NAME")"

# O GNOME oferece "Iniciar usando placa de vídeo dedicada" sozinho, mas só
# enquanto considera o app parado — com o atalho agora vinculado à janela, o
# item some assim que o emulador sobe. A ação abaixo fica sempre disponível.
DGPU_ENV="$(detect_dgpu_env || true)"

ACTIONS="cold-boot;wipe-data;"
[ -n "$DGPU_ENV" ] && ACTIONS="gpu-dedicada;$ACTIONS"

mkdir -p "$APPS_DIR"
cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=$DISPLAY_NAME
GenericName=Android Emulator
Comment=Inicia o emulador Android $AVD_NAME
Exec=env RESOURCE_NAME=$WM_CLASS_NAME "$EMULATOR_BIN" -avd $AVD_NAME
Icon=$ICON_VALUE
Terminal=false
Categories=Development;IDE;
Keywords=android;emulator;avd;$AVD_NAME;
StartupNotify=true
StartupWMClass=$WM_CLASS_NAME
Actions=$ACTIONS

[Desktop Action cold-boot]
Name=Iniciar com cold boot
Exec=env RESOURCE_NAME=$WM_CLASS_NAME "$EMULATOR_BIN" -avd $AVD_NAME -no-snapshot-load

[Desktop Action wipe-data]
Name=Iniciar apagando os dados
Exec=env RESOURCE_NAME=$WM_CLASS_NAME "$EMULATOR_BIN" -avd $AVD_NAME -wipe-data
EOF

if [ -n "$DGPU_ENV" ]; then
    cat >> "$DESKTOP_FILE" <<EOF

[Desktop Action gpu-dedicada]
Name=Iniciar com placa de vídeo dedicada
Exec=env RESOURCE_NAME=$WM_CLASS_NAME $DGPU_ENV "$EMULATOR_BIN" -avd $AVD_NAME
EOF
fi

chmod 644 "$DESKTOP_FILE"
update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_FILE" >/dev/null 2>&1 \
        || info "Aviso: desktop-file-validate reportou avisos em $DESKTOP_FILE"
fi

MSG="Atalho criado com sucesso.

AVD:      $AVD_NAME
Nome:     $DISPLAY_NAME
Ícone:    $ICON_VALUE
Arquivo:  $DESKTOP_FILE
GPU:      ${DGPU_ENV:-só uma GPU detectada — sem ação de vídeo dedicado}"

if [ "$UI_MODE" -eq 1 ] && command -v zenity >/dev/null 2>&1; then
    zenity --info --width=460 --title="AVD Menu" --text="$MSG" 2>/dev/null || true
fi
info "$MSG"
