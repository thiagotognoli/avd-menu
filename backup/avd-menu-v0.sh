mkdir -p ~/.local/share/applications && cat > ~/.local/share/applications/android-emulator-dev.desktop <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Android Emulator DEV
Comment=Inicia o emulador Android DEV
Exec=$ANDROID_SDK_ROOT/emulator/emulator -avd DEV
Icon=$ANDROID_SDK_ROOT/emulator/lib64/qt/lib/images/android-emulator.png
Terminal=false
Categories=Development;
EOF