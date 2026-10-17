import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import java.io.*; import java.util.*;
public class MsgMap extends GhidraScript {
  public void run() throws Exception {
    PrintWriter w = new PrintWriter(new FileWriter(getScriptArgs()[0]));
    int msg = Integer.parseInt(getScriptArgs()[1],16);
    Memory mem = currentProgram.getMemory();
    DecompInterface di = new DecompInterface(); di.openProgram(currentProgram);
    Set<Function> done = new HashSet<>();
    byte[] pat = new byte[]{(byte)(msg&0xff),(byte)((msg>>8)&0xff),0,0};
    MemoryBlock rd = mem.getBlock(".rdata");
    Address a = rd.getStart();
    while (true) {
      a = mem.findBytes(a, rd.getEnd(), pat, null, true, monitor);
      if (a == null) break;
      // AFX_MSGMAP_ENTRY: nMessage, nCode, nID, nLastID, nSig, pfn (6 dwords)
      if (a.getOffset() % 4 == 0) {
        long pfn = getInt(a.add(20)) & 0xffffffffL;
        Function f = getFunctionAt(toAddr(pfn));
        if (f != null && done.add(f)) {
          w.println("// ===== entry @ "+a+" -> "+f.getName());
          DecompileResults r = di.decompileFunction(f, 60, monitor);
          w.println(r.decompileCompleted()? r.getDecompiledFunction().getC() : "// FAILED");
        }
      }
      a = a.add(1);
    }
    w.close();
  }
}
