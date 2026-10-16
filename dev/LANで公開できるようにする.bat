@echo off
chcp 65001 > nul
rem Alpedia の LAN マルチプレイ：ほかの PC から、この PC で公開したワールドに参加できるように、
rem Windows のファイアウォールで TCP 47620（ゲーム）と UDP 47621（LAN のワールド探し）の受信を許可する。
rem 管理者の権限が要る（確認が出たら「はい」）。会社の PC などで管理者になれないときは、PC の管理者に頼んでください。
net session >nul 2>&1
if errorlevel 1 (
  echo 管理者として実行し直します...
  powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  exit /b
)
netsh advfirewall firewall delete rule name="Alpedia LAN (TCP 47620)" >nul 2>&1
netsh advfirewall firewall delete rule name="Alpedia LAN (UDP 47621)" >nul 2>&1
netsh advfirewall firewall add rule name="Alpedia LAN (TCP 47620)" dir=in action=allow protocol=TCP localport=47620 profile=any
netsh advfirewall firewall add rule name="Alpedia LAN (UDP 47621)" dir=in action=allow protocol=UDP localport=47621 profile=any
if errorlevel 1 (
  echo.
  echo 許可できませんでした。上のメッセージを確認してください。
) else (
  echo.
  echo 許可しました。ほかの PC から参加できるか試してください。
  echo （元に戻すには、Windows の「セキュリティが強化された Windows Defender ファイアウォール」の受信の規則から
  echo  「Alpedia LAN」の2つを削除します）
)
pause
