; 气质花按键助手 NSIS installer hooks
;
; productName 为中文（构建产物、Programs & Features 显示名均带中文），
; 但安装目录强制为英文 FlairBloom，避免 Unicode 路径影响
; interception.dll、ddc.exe 等原生组件加载。
;
; interception.dll 通过 bundle.resources 打入安装包，落地在 $INSTDIR\resources\，
; 但 EXE 隐式链接 interception.dll 需要它与 EXE 同级（或在系统 DLL 搜索路径上）。
; PostInstall 阶段把它移动到 $INSTDIR\，卸载时一并清理。

!macro NSIS_HOOK_PREINSTALL
  ; 用户接受默认路径时改写为英文目录；用户自定义路径则保持不动。
  ; 用 Tauri NSIS 模板预定义的 ${PRODUCTNAME} 比较，避免硬编码中文字面量
  ; 在 .nsh 中的编码风险，并跟随 productName 改动。Tauri 模板在 PREINSTALL
  ; 前已 SetOutPath $INSTDIR，修改 $INSTDIR 后必须再 SetOutPath 一次。
  StrCpy $0 ""
  StrCpy $1 "${PRODUCTNAME}"
  ${If} $INSTDIR == "$LOCALAPPDATA\$1"
    StrCpy $0 "$LOCALAPPDATA\FlairBloom"
  ${ElseIf} $INSTDIR == "$PROGRAMFILES64\$1"
    StrCpy $0 "$PROGRAMFILES64\FlairBloom"
  ${ElseIf} $INSTDIR == "$PROGRAMFILES\$1"
    StrCpy $0 "$PROGRAMFILES\FlairBloom"
  ${EndIf}
  ${If} $0 != ""
    StrCpy $INSTDIR $0
    SetOutPath $INSTDIR
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} ${FileExists} "$INSTDIR\resources\interception.dll"
    Rename "$INSTDIR\resources\interception.dll" "$INSTDIR\interception.dll"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\interception.dll"
!macroend
