SoundBimba — портативная версия / portable version
==================================================

РУССКИЙ

Установка не нужна: распакуйте папку куда угодно и запустите SoundBimba.exe.

Что нужно на компьютере:
  • Windows 10 или 11 (64-бит).
  • WebView2 — движок, которым рисуется окно. Обычно уже есть (ставится вместе с Edge
    и обновлениями Windows). Если окно не открывается — поставьте «WebView2 Runtime»
    с сайта Microsoft. Сама программа ничего не скачивает.
  • Чтобы звук шёл в Discord, OBS и игры — виртуальный кабель (например, VB-CABLE).
    Его ставите вы сами, программа его только находит.

Где хранятся настройки и схемы: %APPDATA%\SoundBimba — те же, что у установленной версии.
Если что-то работает не так: в программе «Проект → Сохранить диагностику» и пришлите файл.

Сборка без цифровой подписи: Windows может предупредить — «Подробнее» → «Выполнить в любом
случае». Подлинность можно сверить с SHA256SUMS.txt (PowerShell: Get-FileHash .\SoundBimba.exe).

Лицензия: GNU AGPL-3.0-or-later (файл LICENSE). Исходный код:
https://github.com/SpiceLNR/SoundBimba . Сторонние компоненты — THIRD_PARTY_NOTICES.md.


ENGLISH

No installation: unpack the folder anywhere and run SoundBimba.exe.

Requirements:
  • Windows 10 or 11 (64-bit).
  • WebView2 (draws the window). Usually already present (comes with Edge and Windows
    updates). If the window does not open, install "WebView2 Runtime" from Microsoft.
    The app itself never downloads anything.
  • For sound into Discord, OBS and games — a virtual cable (e.g. VB-CABLE), installed by you.

Settings and setups live in %APPDATA%\SoundBimba — shared with the installed version.
If something goes wrong: "Project → Save diagnostics" in the app, and send the file.

Not digitally signed: Windows may warn — "More info" → "Run anyway". Check the file against
SHA256SUMS.txt (PowerShell: Get-FileHash .\SoundBimba.exe).

License: GNU AGPL-3.0-or-later (LICENSE). Source code: https://github.com/SpiceLNR/SoundBimba .
Third-party components: THIRD_PARTY_NOTICES.md.
