!macro NSIS_HOOK_POSTUNINSTALL
  RMDir /r "$LOCALAPPDATA\WhisperWindows"
  RMDir /r "$LOCALAPPDATA\com.sejeonglee.whisperwindows"
  RMDir /r "$APPDATA\com.sejeonglee.whisperwindows"
!macroend
