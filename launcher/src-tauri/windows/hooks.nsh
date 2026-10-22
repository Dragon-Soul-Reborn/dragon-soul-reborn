; Dragon Soul Reborn: when the launcher is uninstalled, offer to delete the game folder it installed.
; The launcher records that folder in HKCU\Software\Dragon Soul Reborn\GameFolder.
; Silent runs (updates) never ask and never delete.
!macro NSIS_HOOK_PREUNINSTALL
  IfSilent dsr_done
  ReadRegStr $R0 HKCU "Software\Dragon Soul Reborn" "GameFolder"
  StrCmp $R0 "" dsr_forget
  StrLen $R1 $R0
  IntCmp $R1 3 dsr_forget dsr_forget
  IfFileExists "$R0\gameclient_release.exe" 0 dsr_forget
  MessageBox MB_YESNO|MB_ICONQUESTION "Also delete the game files?$\r$\n$\r$\n$R0" /SD IDNO IDNO dsr_forget
  RMDir /r "$R0"
  dsr_forget:
  DeleteRegKey HKCU "Software\Dragon Soul Reborn"
  dsr_done:
!macroend
