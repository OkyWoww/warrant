# Evidence Bundle: evb-89972f8abae64f179ac61d699b042fe3

- **Candidate ID**: `c-0001`
- **Verdict**: `PROVEN_PUBLIC`
- **Toolchain**: `nightly-2026-10-20`
- **Harness SHA-256**: `31d168b9119cb87bd31abd9a5c9b4c15e060bda1957b69113a0968494dbb4bfd`
- **Target Symbol**: `CompactBuffer<T>::insert_slice`
- **Verified in Miri Trace**: `true`

## Reproduction Instructions

To reproduce this verdict independently from a clean terminal:

### On Linux / macOS / WSL / Docker:
```bash
./reproduce.sh
```

### On Windows CMD:
```cmd
reproduce.bat
```

### On Windows PowerShell:
```powershell
.\reproduce.ps1
```
