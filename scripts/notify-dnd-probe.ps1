<#
  Which Windows toast "scenarios" show a banner while Do Not Disturb is ON? Sends one test toast per scenario under the
  NEXUS notification identity, 6 seconds apart, then lists them so you can read off the result.

  Run in WINDOWS PowerShell (not pwsh 7; WinRT is not available there), with Do Not Disturb ON:
      powershell -NoProfile -ExecutionPolicy Bypass -File scripts\notify-dnd-probe.ps1

  Writes only the per-user notification identity key the app also writes (HKCU\Software\Classes\AppUserModelId\com.nexus.assistant).
  Test toasts can be removed with "Clear all" in the notification panel.
#>
param([string]$Aumid = "com.nexus.assistant")
$ErrorActionPreference = "Stop"
$k = "HKCU:\Software\Classes\AppUserModelId\$Aumid"
if (-not (Test-Path $k)) {
  New-Item -Path $k -Force | Out-Null
  Set-ItemProperty -Path $k -Name DisplayName -Value "NEXUS"
  Set-ItemProperty -Path $k -Name ShowInSettings -Value 1 -Type DWord
}
[void][Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
[void][Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime]
$notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($Aumid)

$variants = @(
  @{ name = "default";      attrs = "" },
  @{ name = "reminder";     attrs = ' scenario="reminder"' },
  @{ name = "urgent";       attrs = ' scenario="urgent"' },
  @{ name = "alarm";        attrs = ' scenario="alarm"' },
  @{ name = "incomingCall"; attrs = ' scenario="incomingCall"' }
)
$i = 0
foreach ($v in $variants) {
  $i++
  $xml = "<toast$($v.attrs)><visual><binding template=`"ToastGeneric`"><text>NEXUS probe $i of $($variants.Count): $($v.name)</text><text>If you can read this as a banner, '$($v.name)' passes Do Not Disturb.</text></binding></visual><audio silent=`"true`"/></toast>"
  $doc = New-Object Windows.Data.Xml.Dom.XmlDocument
  $doc.LoadXml($xml)
  $notifier.Show([Windows.UI.Notifications.ToastNotification]::new($doc))
  "{0}  sent: {1}" -f (Get-Date -Format HH:mm:ss), $v.name
  Start-Sleep -Seconds 6
}
"Done. Tell me which of: default, reminder, urgent, alarm, incomingCall showed a BANNER (not just an entry in the panel)."
