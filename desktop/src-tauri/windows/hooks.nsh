; Klipt registers klipt:// for the current user at start-up (src/copy_link.rs); uninstalling
; removes it. Updates don't run this, and Klipt registers it again on every start anyway.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\klipt"
!macroend
