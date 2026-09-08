; Windows Firewall rule for LAN/mDNS discovery — mirrors the runtime fix in
; src/commands/discover/firewall.rs (program-scoped inbound allow, any port:
; the local server binds a dynamic TCP port).
;
; Best effort: with the default per-user install the installer runs
; unelevated and netsh silently fails — the in-app "fix now" card covers
; that case via UAC.

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name=all dir=in program="$INSTDIR\PlainApp.exe"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="PlainApp" dir=in action=allow program="$INSTDIR\PlainApp.exe" enable=yes profile=any'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name=all dir=in program="$INSTDIR\PlainApp.exe"'
!macroend
