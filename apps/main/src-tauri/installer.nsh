; FlairBloom NSIS installer hooks
;
; interception.dll 通过 bundle.resources 打入安装包，落地在 $INSTDIR\resources\，
; 但 EXE 隐式链接 interception.dll 需要它与 EXE 同级（或在系统 DLL 搜索路径上）。
; PostInstall 阶段把它移动到 $INSTDIR\，卸载时一并清理。

!macro NSIS_HOOK_POSTINSTALL
  ${If} ${FileExists} "$INSTDIR\resources\interception.dll"
    Rename "$INSTDIR\resources\interception.dll" "$INSTDIR\interception.dll"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\interception.dll"
!macroend
