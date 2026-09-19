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
  ; 「以管理员模式启动」写在当前用户的兼容性标志里，卸载时清掉，
  ; 免得注册表留一条指向已不存在的 EXE 的记录。
  DeleteRegValue HKCU "Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers" "$INSTDIR\FlairBloom.exe"
!macroend
