Unicode true
!include "MUI2.nsh"
!ifndef BUILD_ID
  !error "BUILD_ID is required"
!endif
!ifndef STAGE
  !error "STAGE is required"
!endif
!ifndef OUTFILE
  !error "OUTFILE is required"
!endif
Name "Remote EX (${BUILD_ID})"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Remote EX\${BUILD_ID}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"

; Version-specific, current-user, no service, auto-start, or elevation.
Function .onInit
  SetShellVarContext current
  StrCpy $INSTDIR "$LOCALAPPDATA\Programs\Remote EX\${BUILD_ID}"
  IfFileExists "$INSTDIR\*.*" exists
  Return
  exists:
    MessageBox MB_OK|MB_ICONSTOP "This build already exists. Close Remote EX and uninstall it first." /SD IDOK
    SetErrorLevel 1
    Abort
FunctionEnd
Section "Remote EX"
  SetOutPath "$INSTDIR"
  SetOverwrite off
  File "${STAGE}\remote-ex-desktop.exe"
  File "${STAGE}\lan_view.exe"
  File "${STAGE}\lan_diagnostic.exe"
  File "${STAGE}\lan_control.exe"
  File "${STAGE}\routed_diagnostic.exe"
  File "${STAGE}\routed_control.exe"
  File "${STAGE}\remote-ex-rendezvous.exe"
  File "${STAGE}\sidecars.json"
  File "${STAGE}\BUILD_INFO.txt"
  File "${STAGE}\README.md"
  File "${STAGE}\INSTALL.md"
  File "${STAGE}\DEPENDENCIES.json"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Remote EX"
  CreateShortcut "$SMPROGRAMS\Remote EX\Remote EX ${BUILD_ID}.lnk" "$INSTDIR\remote-ex-desktop.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RemoteEX-${BUILD_ID}" "DisplayName" "Remote EX (${BUILD_ID})"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RemoteEX-${BUILD_ID}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RemoteEX-${BUILD_ID}" "UninstallString" '$"$INSTDIR\Uninstall.exe$"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RemoteEX-${BUILD_ID}" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RemoteEX-${BUILD_ID}" "NoRepair" 1
SectionEnd
Function un.onInit
  SetShellVarContext current
  StrCpy $INSTDIR "$LOCALAPPDATA\Programs\Remote EX\${BUILD_ID}"
FunctionEnd
Section "Uninstall"
  ; Exact owned files only; preserve user data and credentials.
  ClearErrors
  Delete "$INSTDIR\remote-ex-desktop.exe"
  Delete "$INSTDIR\lan_view.exe"
  Delete "$INSTDIR\lan_diagnostic.exe"
  Delete "$INSTDIR\lan_control.exe"
  Delete "$INSTDIR\routed_diagnostic.exe"
  Delete "$INSTDIR\routed_control.exe"
  Delete "$INSTDIR\remote-ex-rendezvous.exe"
  Delete "$INSTDIR\sidecars.json"
  Delete "$INSTDIR\BUILD_INFO.txt"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\INSTALL.md"
  Delete "$INSTDIR\DEPENDENCIES.json"
  IfErrors failed
  Delete "$SMPROGRAMS\Remote EX\Remote EX ${BUILD_ID}.lnk"
  RMDir "$SMPROGRAMS\Remote EX"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RemoteEX-${BUILD_ID}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Goto done
  failed:
    MessageBox MB_OK|MB_ICONSTOP "Some files could not be removed. Close Remote EX and retry. User data is retained." /SD IDOK
    SetErrorLevel 1
  done:
SectionEnd
