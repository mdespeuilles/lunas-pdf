#!/usr/bin/env bash
# Construit Lunas PDF (version de production) et l'installe sur cette machine. Repris de
# Lunas Mail. `bun tauri dev` et l'app installée ont le même identifiant (fr.lunas.feuillet) :
# ils partagent préférences, récents et informations mémorisées.
#
#   macOS : /Applications/Lunas PDF.app, signée si un certificat « Developer ID
#           Application » est dans le trousseau (sinon signature ad hoc).
#   Linux : AppImage dans ~/.local/bin, entrée de lanceur (fichiers PDF) et icône.
set -euo pipefail
cd "$(dirname "$0")/.."

CONFIG=src-tauri/tauri.release.conf.json
# Pas de fichiers de mise à jour pour une installation locale (ils demandent la clé de
# signature du projet, réservée à la CI).
LOCAL='{"bundle":{"createUpdaterArtifacts":false}}'
# Workspace Cargo : les paquets sont dans target/ à la racine.
TARGET_DIR=target/release/bundle

# Bibliothèque PDFium de la plateforme (rien à faire si elle est déjà là).
bun pdfium

case "$(uname -s)" in
  Darwin)
    if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
      identity=$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -n 1)
      if [[ -n "$identity" ]]; then
        export APPLE_SIGNING_IDENTITY="$identity"
        echo "Signature : $identity"
      else
        echo "Pas de certificat Developer ID dans le trousseau : signature ad hoc."
      fi
    fi
    bun tauri build --config "$CONFIG" --config "$LOCAL" --bundles app
    app="$TARGET_DIR/macos/Lunas PDF.app"
    # Seulement l'app installée : une session `bun tauri dev` porte un autre chemin.
    if pgrep -f "/Applications/Lunas PDF.app/" >/dev/null 2>&1; then
      echo "Fermeture de Lunas PDF…"
      osascript -e 'quit app "Lunas PDF"' >/dev/null 2>&1 || true
      sleep 2
    fi
    rm -rf "/Applications/Lunas PDF.app"
    cp -R "$app" /Applications/
    echo "Installé : /Applications/Lunas PDF.app"
    ;;
  Linux)
    bun tauri build --config "$CONFIG" --config "$LOCAL" --bundles appimage
    image=$(ls -t "$TARGET_DIR"/appimage/*.AppImage | head -n 1)
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/128x128/apps"
    install -m 755 "$image" "$HOME/.local/bin/lunas-pdf.AppImage"
    install -m 644 src-tauri/icons/128x128.png "$HOME/.local/share/icons/hicolor/128x128/apps/lunas-pdf.png"
    cat > "$HOME/.local/share/applications/lunas-pdf.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Lunas PDF
Comment=Lecteur et éditeur PDF
Exec=$HOME/.local/bin/lunas-pdf.AppImage %F
Icon=lunas-pdf
Categories=Office;Viewer;
MimeType=application/pdf;
StartupWMClass=lunas-pdf
Terminal=false
DESKTOP
    command -v update-desktop-database >/dev/null && update-desktop-database "$HOME/.local/share/applications" || true
    echo "Installé : ~/.local/bin/lunas-pdf.AppImage (entrée « Lunas PDF » dans le lanceur)"
    echo "Pour en faire le lecteur PDF par défaut : xdg-mime default lunas-pdf.desktop application/pdf"
    ;;
  *)
    echo "Système non pris en charge par ce script ; utilisez : bun tauri build --config $CONFIG" >&2
    exit 1
    ;;
esac
