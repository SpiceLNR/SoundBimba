; SPDX-License-Identifier: AGPL-3.0-or-later
; Свои шаги установщика (Tauri NSIS hooks): ярлык «Удалить SoundBimba» в меню «Пуск» рядом с
; ярлыком программы. Удаляется вместе с программой.

!macro SB_UNINSTALL_LINK_NAME OUT
  ${If} $LANGUAGE == ${LANG_RUSSIAN}
    StrCpy ${OUT} "Удалить SoundBimba"
  ${ElseIf} $LANGUAGE == ${LANG_UKRAINIAN}
    StrCpy ${OUT} "Видалити SoundBimba"
  ${Else}
    StrCpy ${OUT} "Uninstall SoundBimba"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Только если человек не отказался от ярлыков на странице «Папка в меню Пуск» и папка задана.
  !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
    ${If} $AppStartMenuFolder != ""
      CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
      !insertmacro SB_UNINSTALL_LINK_NAME $R9
      CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\$R9.lnk" "$INSTDIR\uninstall.exe"
    ${EndIf}
  !insertmacro MUI_STARTMENU_WRITE_END
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Язык при удалении мог быть другим — убираем ярлык под любым из трёх названий.
  Delete "$SMPROGRAMS\$AppStartMenuFolder\Удалить SoundBimba.lnk"
  Delete "$SMPROGRAMS\$AppStartMenuFolder\Видалити SoundBimba.lnk"
  Delete "$SMPROGRAMS\$AppStartMenuFolder\Uninstall SoundBimba.lnk"
  RMDir "$SMPROGRAMS\$AppStartMenuFolder"
!macroend
