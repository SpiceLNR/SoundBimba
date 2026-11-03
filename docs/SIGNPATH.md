# Заявка на бесплатную подпись в SignPath Foundation

Подпись нужна, чтобы Windows не пугала при установке («Windows защитила ваш компьютер»).

**Решение автора 2026-11-03: пока не подаём.** Фонд требует «проверяемой репутации» для программ,
а у проекта на тот момент 0 звёзд и по одному скачиванию релизов — отказ был бы очень вероятен.
Подать, когда появятся пользователи (звёзды, скачивания, упоминания). Перед подачей вернуть в
раздел «Code signing policy» README (все три языка) обязательную фразу фонда:
«Free code signing provided by SignPath.io, certificate by SignPath Foundation» — фонд проверяет,
что она уже есть на странице проекта и загрузки.
Подаёт заявку автор; Claude готовит репозиторий и потом подключает подпись к автосборке.
Условия фонда: https://signpath.org/terms (прочитаны 2026-11-03).

## Перед подачей

1. **Двухфакторная защита на GitHub** — обязательна для всех участников.
   GitHub → фото профиля → Settings → Password and authentication → «Enable two-factor
   authentication». Подойдёт приложение-аутентификатор на телефоне. Сохраните коды восстановления.
2. **Раздел «Code signing policy» в README** — есть (роли, приватность); перед подачей вписать в
   него фразу фонда (см. выше) и отправить на GitHub.
3. *(по желанию)* В последнем релизе (v0.3.0) нажать «Edit» и дописать внизу строку
   `Code signing policy: https://github.com/SpiceLNR/SoundBimba/blob/main/README.en.md#code-signing-policy`
   — у следующих релизов она появится сама.

## Подача

Откройте https://signpath.org/apply. Форма на английском; готовые ответы:

| Поле | Что вписать |
|---|---|
| Project Name* | `SoundBimba` |
| Repository URL* | `https://github.com/SpiceLNR/SoundBimba` |
| Homepage URL* | `https://github.com/SpiceLNR/SoundBimba` |
| Download URL | `https://github.com/SpiceLNR/SoundBimba/releases` |
| Privacy Policy URL | `https://github.com/SpiceLNR/SoundBimba/blob/main/README.en.md#code-signing-policy` |
| Wikipedia URL (optional) | пусто |
| Tagline* | `Free offline node-based audio routing for Windows: app sound, microphone, sound buttons and voice effects to headphones or a virtual microphone.` |
| Category* | ближайшее из списка (Multimedia / Audio, иначе Utilities) |
| Description* | см. ниже (короткий абзац, без версий и библиотек) |
| Reputation | честно: звёзды, скачивания, упоминания на момент подачи |
| Описание проекта (если спросят) | см. ниже |
| Лицензия (если спросят) | `AGPL-3.0-or-later` |
| Что подписывать (если спросят) | `Windows installer (NSIS .exe) and the application .exe, built by GitHub Actions` |
| Сборка (если спросят) | `GitHub Actions, workflow .github/workflows/build.yml` |

Description (скопировать; форма просит коротко и без версий):

> SoundBimba lets you build your sound like a diagram: capture the audio of individual apps and the
> microphone, add sound buttons, volume, compression, voice effects and noise suppression, and send
> the result to headphones or to a virtual microphone for Discord, OBS or games. It works fully
> offline, has a simple mode for beginners, and is available in Russian, English and Ukrainian.

Поля ниже «Description» в форме на 2026-11-03 не рассмотрены (не загружались) — смотреть на месте.

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
