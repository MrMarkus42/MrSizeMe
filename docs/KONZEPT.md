# MrSizeMe – Konzept

Stand: 24.11.2026. Vereinbart im Gespräch vor Beginn der Programmierung.

MrSizeMe zeigt, welche Ordner und Dateien den Platz auf den Laufwerken
belegen, und hilft sicher beim Aufräumen. Es ist in Rust geschrieben
(Oberfläche: egui/eframe) und läuft unter Windows, macOS und Linux.

## Grundsätze

* **Plattformen:** Windows und macOS haben Vorrang, Linux läuft mit.
* **Sprachen:** Deutsch und Englisch. Die Sprache folgt dem System und ist in
  den Einstellungen umstellbar. Alle Texte stehen in Sprachdateien.
* **Sicherheit:** Nur Metadaten werden gelesen (Name, Größe, Datum), nie der
  Inhalt einer Datei – Ausnahme: Prüfsummen für die Duplikatsuche. Hohe
  Rechte bekommt nur ein kleiner Scan-Teil, nie die ganze Oberfläche.
  Gelöscht wird nur in den Papierkorb und nur nach Bestätigung;
  Systemordner sind gesperrt.
* **Datenschutz:** Nichts verlässt den Rechner ohne Zustimmung. Ins Netz geht
  höchstens ein Ordnername mit Größe und Betriebssystem, nie ein voller Pfad
  oder der Benutzername. Ein Schalter „Komplett offline“ sperrt alles, was
  ins Netz geht.
* **Version 1 enthält alles unten Beschriebene.** Intern wird in Etappen
  gebaut (siehe Ende); veröffentlicht wird erst, wenn alles fertig und
  geprüft ist.

## 1. Scannen

* Paralleles Lesen der Ordner; Verknüpfungen und Junctions werden nie
  verfolgt; Hardlinks zählen nur einmal.
* Zwei Größen: Dateigröße und tatsächlich belegter Platz (komprimierte,
  dünn belegte und Cloud-Dateien).
* Abbrechen jederzeit; Fortschritt mit Anzahl, Datenmenge und aktuellem Ordner.
* Mehrere Laufwerke und Ordner, auch Netzlaufwerke und NAS.
* **Live-Aktualisierung:** Nach dem Scan beobachtet das Programm Änderungen
  und passt die Zahlen an, ohne neu zu scannen.
* **Admin-Modus**
  * Windows: Knopf „Mit Admin-Rechten scannen“ startet (nach der
    Windows-Rückfrage) ein kleines Scan-Hilfsprogramm mit Admin-Rechten; die
    Oberfläche bleibt ohne erhöhte Rechte (Entscheidung 24.11.: sicherer als
    „ganz als Administrator neu starten“). Das Hilfsprogramm nutzt das
    Sicherungsrecht (auch geschützte Ordner sichtbar) und den Schnell-Scan über
    das NTFS-Inhaltsverzeichnis (MFT). Ergebnis kommt als Datei in einem
    frischen Temp-Ordner zurück; das Hilfsprogramm darf nur dorthin schreiben.
  * macOS: Erkennen, ob „Festplattenvollzugriff“ fehlt, und Schritt für
    Schritt zeigen, wo man ihn einschaltet.
  * Linux: kleines Scan-Hilfsprogramm, das über die Systemabfrage (pkexec)
    mit root-Rechten läuft; die Oberfläche bleibt beim normalen Benutzer.

## 2. Ansichten

Umschaltbar nach Größe, belegtem Platz oder Anzahl Dateien.

* **Ordnerbaum** mit Balken, Prozent, Dateien, Ordnern, Datum (Hauptansicht).
* **Kachelkarte:** große Dateien = große Flächen, Farben nach Dateiart.
* **Ringdiagramm:** Ordner als Ringe von innen nach außen.
* **Dateitypen** und **Kategorien:** Videos, Bilder, Musik, Dokumente,
  Programme, Spiele (z. B. Spielebibliotheken), virtuelle Maschinen,
  Entwickler-Dateien, Caches.
* **Größte Dateien, älteste Dateien, Altersverteilung.**
* **Programme-Ansicht:** Platz pro Programm = Programm + Daten + Caches
  (auf dem Mac besonders unter `~/Library`).
* **Cloud-Dateien:** OneDrive/iCloud – was nur online liegt und was lokal
  Platz belegt; Knopf „Nur online behalten“.
* **Suche und Filter**, z. B. „Videos über 1 GB, älter als 2 Jahre“.
* **Doppelte Dateien** (erst Größe, dann Prüfsumme).

## 3. Aufräumhilfe

* **Aufräum-Vorschläge auf einen Blick:** „Hier könnten Sie ca. 23 GB
  sparen“ – Caches, alte Installer, Duplikate, Entwickler-Reste, Papierkorb.
* **„Was ist das? Darf das weg?“** per Rechtsklick auf Ordner oder Datei:
  * **Eingebautes Wissen (offline):** geprüfte Liste bekannter Ordner
    (z. B. WinSxS, hiberfil.sys, pagefile.sys, Windows.old, Papierkorb,
    Docker, node_modules, Library/Caches, Xcode-Daten, iPhone-Backups,
    lokale Time-Machine-Schnappschüsse). Ampel: grün = darf weg,
    gelb = mit Werkzeug aufräumen, rot = nicht anfassen. Dazu der richtige
    Weg (z. B. Windows-Bereinigung statt Löschen von Hand).
  * **KI fragen** (nur auf Wunsch, mit Rückfrage, alle Varianten wählbar):
    lokale KI (Ollama), Cloud-KI mit eigenem Schlüssel (Gemini, Claude),
    Google-KI-Modus bzw. Google-Suche im Browser. Immer mit Hinweis, dass die
    KI sich irren kann. Die KI löscht nie selbst.
* **Löschen** nur in den Papierkorb, mit Bestätigung; Systemordner gesperrt.
* **Verschieben-Assistent:** große Ordner auf eine andere Platte verschieben,
  auf Wunsch mit Verknüpfung am alten Ort.

## 4. Verlauf und Vergleich

* Scans speichern (nur Zahlen und Namen, lokal).
* **Vergleich:** „Seit letzter Woche ist Downloads um 12 GB gewachsen“.
* **Prognose:** „Bei diesem Tempo ist C: in etwa 4 Monaten voll“.
* **Warnung bei fast voller Platte** (optional, z. B. unter 10 % frei).

Umsetzung (Etappe 6):

* Nach jedem fertigen Scan wird eine verkleinerte Kopie des Baums gespeichert:
  Einträge ab 1 MiB (bei sehr großen Scans mehr), höchstens 8 Ebenen und
  30.000 Einträge, dazu Gesamtgrößen und freier Platz der Platte. Ablage im
  Einstellungsordner unter `history/`, je Ordner höchstens 100 Scans; ein
  neuer Scan innerhalb von 10 Minuten ersetzt den letzten.
* Der Vergleich nennt die Stelle, die die Änderung wirklich erklärt: Ein
  Ordner wird weggelassen, wenn ein einzelner Eintrag darin 90 % seiner
  Änderung ausmacht.
* Prognose: Gerade durch die Messpunkte (kleinste Quadrate), erst ab Scans an
  zwei verschiedenen Tagen.
* Warnung: nur feste Laufwerke, Prüfung im Hintergrund alle 10 Minuten.
* Live-Aktualisierung: Paket `notify`; bei jeder Meldung wird nur der
  betroffene Ordner neu gelesen, neue Unterordner werden einzeln gescannt.
  Unter Linux braucht jeder Ordner eine eigene Beobachtung, deshalb dort nur
  bis 100.000 Unterordner.

## 5. Oberfläche

* Modernes Aussehen, hell und dunkel, eigene einheitliche Symbole.
* Dialoge und Werkzeugfenster als echte, verschiebbare Fenster (auch auf
  einen zweiten Bildschirm).
* Fehlermeldungen markierbar und kopierbar (Knopf + Strg+C), vollständig im
  Protokoll.
* Volle Tastaturbedienung, Unterstützung für Bildschirmleser.
* Rechtsklick im Explorer bzw. Finder „Mit MrSizeMe analysieren“
  (nur nach Zustimmung, da es das System ändert).

## 6. Export

CSV, Excel-taugliche Datei, HTML-Bericht; Kommandozeilen-Modus (auch JSON).

## 7. Updates

Einstellbar: nie suchen / fragen / automatisch suchen, mit Häufigkeit.
Das Programm fragt beim ersten Start, ob es überhaupt suchen darf. Updates
sind mit minisign signiert; der öffentliche Schlüssel ist eingebaut.

## 8. Lieferformen

* Windows: Installer und portable Version (ohne Installation, z. B. USB-Stick).
* macOS: .dmg.
* Linux: Archiv (später ggf. weitere Pakete).
* Zu jedem Download eine SHA-256-Prüfsummendatei.

## Etappen (intern)

| Nr. | Inhalt | Stand |
| --- | --- | --- |
| 1 | Scanner-Kern, Baumstruktur, Tests | fertig (24.11.) |
| 2 | Oberfläche: Ordnerbaum, Kachelkarte, Dateitypen, Sprachen, Icon | erste Fassung (24.11.); Icon-Auswahl, Feinschliff offen |
| 3 | Weitere Ansichten: Ring, Kategorien, Größte/Älteste, Alter, Filter | fertig (24.11.) |
| 4 | Admin-Modus (Windows, macOS, Linux), MFT-Schnell-Scan, Menüband | fertig (24.11.); auf echtem Windows/Mac noch vom Nutzer zu testen |
| 5 | Aufräumhilfe: Wissensliste, Papierkorb, Vorschläge, KI | fertig (24.11.); KI-Anbieter mit echten Schlüsseln noch vom Nutzer zu testen |
| 6 | Verlauf, Vergleich, Prognose, Warnung, Live-Aktualisierung | fertig (25.11.); Live-Aktualisierung auf echtem Windows/Mac noch vom Nutzer zu testen |
| 7 | Duplikate, Programme-Ansicht, Cloud-Dateien, Verschieben | fertig (25.11.); „Nur online behalten“ und Verknüpfung (Windows) noch vom Nutzer zu testen |
| 8 | Export, Kommandozeile, Kontextmenü im Explorer/Finder | fertig (25.11.); Kontextmenü auf echtem Windows/Mac noch vom Nutzer zu testen |
| 9 | Updates, Installer, portable Version, .dmg, Website | fertig; Version 1.0.0 am 25.11. veröffentlicht und geprüft; Website online (mrmarkus42.github.io/MrSizeMe) |

## Arbeitsweise

* Gebaut und getestet wird auf dem Linux-Rechner. Auf dem Windows-PC werden
  keine selbst gebauten Programme gestartet. Windows und macOS werden über
  GitHub Actions (nur nach Rückfrage) oder durch Tests des Nutzers geprüft.
* Abhängigkeiten: neueste Versionen, aber mindestens 14 Tage alt;
  Lock-Datei wird eingecheckt; nur freie Lizenzen.

## Offene Punkte

* `url` 2.5.9 und `cookie` 0.18.3 (Sicherheitskorrekturen RUSTSEC-2026-0592
  und -0594) einspielen, sobald sie 14 Tage alt sind (ab 4.12. bzw. 7.12.),
  mit Version 1.0.1. Vor 1.0.0 (25.11.) geprüft: Beide Pakete werden in kein
  Programm eingebaut (`cargo tree -i` für Windows, macOS und Linux leer), sie
  stehen nur als ungenutzte Wahlmöglichkeit in Cargo.lock. Alle übrigen
  neueren Paketversionen waren am 25.11. jünger als 14 Tage.
* KI-Anbieter: Gemini-Modell voreingestellt `gemini-3.5-flash-lite`,
  Claude-Modell `claude-opus-5-5` (beide in den KI-Einstellungen änderbar).
  Der Google-KI-Modus wird über die Adresse `…/search?udm=50&q=…` geöffnet;
  ändert Google das, nur `ai::google_url` anpassen.
