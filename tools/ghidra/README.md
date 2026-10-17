# Ghidra scripts used for the protocol analysis

Run against `app/OemDrv.exe` from the unpacked installer (`innoextract`):

```bash
analyzeHeadless proj aula -import OemDrv.exe -scriptPath . -postScript DumpHid.java out 3
analyzeHeadless proj aula -process OemDrv.exe -noanalysis -readOnly -scriptPath . -postScript DumpRange.java out/hidregion.c 480000 492000
analyzeHeadless proj aula -process OemDrv.exe -noanalysis -readOnly -scriptPath . -postScript FindStr.java out/str.c "CDev3632::ApplySetting" "SetLedRgbTab error"
analyzeHeadless proj aula -process OemDrv.exe -noanalysis -readOnly -scriptPath . -postScript VTN.java
```

* `DumpHid`: decompiles every function that reaches the HID / file I/O imports, plus callers.
* `DumpRange`: decompiles an address range (the device classes live in 0x480000-0x492000).
* `FindStr`: decompiles functions that reference given log strings.
* `VTN`: prints the `CDev*` vtables recovered from RTTI.
* `MsgMap`: dumps the MFC message maps (which UI handler calls which device method).

The AULA binaries themselves are not included in this repository.
