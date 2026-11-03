# Заявка на бесплатную подпись в SignPath Foundation

Подпись нужна, чтобы Windows не пугала при установке («Windows защитила ваш компьютер»).
Подаёт заявку автор; Claude готовит репозиторий и потом подключает подпись к автосборке.
Условия фонда: https://signpath.org/terms (прочитаны 2026-11-03).

## Перед подачей

1. **Двухфакторная защита на GitHub** — обязательна для всех участников.
   GitHub → фото профиля → Settings → Password and authentication → «Enable two-factor
   authentication». Подойдёт приложение-аутентификатор на телефоне. Сохраните коды восстановления.
2. **Раздел «Code signing policy» в README** — уже есть (с пометкой «заявка подана»); он должен
   быть на GitHub до подачи (отправить изменения).
3. *(по желанию)* В последнем релизе (v0.3.0) нажать «Edit» и дописать внизу строку
   `Code signing policy: https://github.com/SpiceLNR/SoundBimba/blob/main/README.en.md#code-signing-policy`
   — у следующих релизов она появится сама.

## Подача

Откройте https://signpath.org/apply. Форма на английском; готовые ответы:

| Поле | Что вписать |
|---|---|
| Project Name | `SoundBimba` |
| Repository URL | `https://github.com/SpiceLNR/SoundBimba` |
| Homepage URL | `https://github.com/SpiceLNR/SoundBimba` |
| Download URL | `https://github.com/SpiceLNR/SoundBimba/releases` |
| Описание проекта (если спросят) | см. ниже |
| Лицензия (если спросят) | `AGPL-3.0-or-later` |
| Что подписывать (если спросят) | `Windows installer (NSIS .exe) and the application .exe, built by GitHub Actions` |
| Сборка (если спросят) | `GitHub Actions, workflow .github/workflows/build.yml` |

Описание (скопировать):

> SoundBimba is a free, offline, node-based audio routing app for Windows 11: capture the sound of
> individual apps and the microphone, add sound buttons (soundpad), volume, compressor, voice
> morpher and noise suppression (RNNoise), and send the result to headphones or to a virtual
> microphone (a user-installed virtual cable such as VB-CABLE; no drivers are bundled). UI in
> Russian, English and Ukrainian. License AGPL-3.0-or-later; all dependencies are open source
> (MIT/Apache/BSD/MPL/OFL). No network access, no telemetry.

Остальные поля (имя, e-mail и т.п.) — ваши данные. Галочки согласия с условиями — только если вы
с ними согласны (это ваше решение).

## Что может помешать (честно)

- **Репутация.** Фонд пишет, что для программ, которые скачивают и запускают, нужна «проверяемая
  репутация». Новый проект с небольшим числом пользователей могут не принять — это их решение,
  спорить с ними они просят не надо.
- **WebView2 в установщике.** Условия запрещают несвободные компоненты, кроме «системных
  библиотек». Внутри нашего установщика лежит офлайн-установщик WebView2 (Microsoft, часть
  Windows). Если фонд посчитает это помехой — уберём его из подписываемого установщика (тогда
  WebView2 ставится из Windows или отдельно).

## После одобрения (делает Claude вместе с автором)

1. SignPath создаст организацию и проект; автор заходит в SignPath.io (тоже с двухфакторной защитой).
2. Автор создаёт там токен для автосборки и кладёт его в GitHub → Settings → Secrets and
   variables → Actions (имя скажет Claude). **Токен никому не пересылать и не вставлять в чат.**
3. Claude добавляет в `.github/workflows/build.yml` шаг подписи (только для меток `vX.Y.Z` в этом
   репозитории; для pull request и чужих форков — нет).
4. Каждую подпись релиза автор подтверждает вручную в SignPath.io (условие фонда).
5. Из README убирается пометка «заявка подана».
