# Release-Ablauf

Jeder Schritt mit „Mr. M“ macht der Betreuer selbst. Alles andere läuft auf
GitHub oder dem Linux-Rechner.

## Einmalig: Update-Schlüssel anlegen

1. minisign installieren: `winget install jedisct1.minisign`
2. Im Projektordner: `.\scripts\make-update-key.ps1`
   * minisign fragt nach einem Passwort (Mr. M tippt es selbst).
   * Der geheime Schlüssel liegt danach unter `%USERPROFILE%\.minisign\mrsizeme.key`.
     Er kommt nie ins Projekt, nie in die CI und nie in eine Cloud.
     Am besten zusätzlich auf einem USB-Stick sichern.
   * Der öffentliche Schlüssel steht danach in `assets/update-key.pub` und wird
     mit eingecheckt. Ohne ihn bricht der Release-Workflow ab.
3. Den Schlüssel nie austauschen. Ältere Programmversionen kennen nur diesen
   einen und lehnen Updates mit einem anderen Schlüssel ab.

## Für jede Version

1. **Vorbereiten** (Linux-Rechner):
   * Updates und Sicherheitsmeldungen prüfen. `cargo audit`, dann
     `scripts/check-release-age.py`. Bei Bedarf innerhalb der 14-Tage-Regel
     aktualisieren (`scripts/pin-old-versions.py`).
   * Die Version in `Cargo.toml` setzen.
   * In `CHANGELOG.md` die Überschrift `## [1.2.3]` anlegen. Ihr Text wird zur
     Release-Beschreibung.
   * Alle Tests und Clippy für alle drei Systeme laufen lassen.
2. **Bauen lassen** (nur nach Freigabe von Mr. M): Den Tag `v1.2.3` pushen.
   Der Workflow „Release“ baut:
   * `MrSizeMe-1.2.3-windows-x64-setup.exe`: Installer
   * `MrSizeMe-1.2.3-windows-x64-portable.zip`: portabel
   * `MrSizeMe-1.2.3-windows-x64.exe`: Programmdatei für das Update der portablen Version
   * `MrSizeMe-1.2.3-macos.dmg`: Apple-Chip und Intel
   * `MrSizeMe-1.2.3-linux-x86_64.tar.gz`
   * `SHA256SUMS.txt`

   Alles landet als **Entwurf** auf GitHub.
3. **Signieren** (Mr. M, eigener PC): `.\scripts\sign-release.ps1 v1.2.3`.
   * minisign fragt einmal nach dem Passwort.
   * Die `.minisig`-Dateien werden zum Entwurf hochgeladen.
4. **Veröffentlichen** (Mr. M): Entwurf auf GitHub ansehen, dann
   „Publish release“ klicken oder `gh release edit v1.2.3 --draft=false`.
5. **Prüfen** (Linux-Rechner): `scripts/verify-release.sh v1.2.3`. Das Skript prüft:
   * Prüfsummen
   * jede Signatur mit dem eingebauten Schlüssel
   * ob „latest“ auf die neue Version zeigt
6. **Website** (nur nach Freigabe): Neue Bilder nach `website/img`, dann auf
   GitHub „Actions → Website → Run workflow“.
7. Test-Branches löschen, falls es welche gab.

## Hinweise

* **Windows:** Die Programme haben keine Authenticode-Signatur, weil es kein
  gekauftes Zertifikat gibt. SmartScreen warnt deshalb beim ersten Start
  („Weitere Informationen → Trotzdem ausführen“). Die minisign-Signatur prüft
  MrSizeMe selbst bei jedem Update.
* **macOS:** Ohne Apple-Entwicklerkonto ist die App nur ad-hoc signiert und
  nicht notarisiert. Beim ersten Start: Rechtsklick → „Öffnen“.
* **Portable Version:** Die Datei `portable.txt` neben dem Programm schaltet
  den portablen Modus ein. Einstellungen, Verlauf und Protokoll liegen dann im
  Ordner `MrSizeMe-Daten` daneben.
