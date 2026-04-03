# Enabling mod-script-pipe in Audacity

vripr communicates with Audacity through a named pipe provided by the
**mod-script-pipe** module. This is a one-time setup step.

---

## Step-by-step

### Windows / macOS / Linux

1. Open **Audacity**.
2. Navigate to **Edit → Preferences → Modules**  
   *(macOS: **Audacity → Preferences → Modules**)*
3. Find **mod-script-pipe** in the list.  
   The status column will show **New** on first launch.
4. Change the value to **Enabled** using the drop-down.
5. Click **OK**.
6. **Restart Audacity completely** — the pipe is only created at startup.
7. Re-open Preferences → Modules and confirm the status now shows **Enabled**.

---

## Verifying the pipe exists

### macOS / Linux
```bash
ls /tmp/audacity_script_pipe.to.$(id -u)
# should print the path without error
```

### Windows
```powershell
# In PowerShell — pipe shows up as a named pipe
[System.IO.Directory]::GetFiles('\\.\pipe\') | Select-String 'Srv'
```

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| Status stuck on **New** | Restart Audacity; check it has write access to its config dir |
| Pipe file missing after restart | Confirm status is **Enabled** (not just saved) then restart again |
| Permission denied on pipe | On Linux, run Audacity as the same user as vripr |
| Audacity 2.x | mod-script-pipe is only reliable from Audacity 3.x onwards — upgrade |
