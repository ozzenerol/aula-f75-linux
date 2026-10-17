import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
public class VTN extends GhidraScript {
  public void run() throws Exception {
    for (Symbol s : currentProgram.getSymbolTable().getAllSymbols(true)) {
      if (!s.getName().equals("vftable")) continue;
      String ns = s.getParentNamespace().getName();
      if (!ns.startsWith("CDev")) continue;
      Address a = s.getAddress();
      println("VTABLE "+ns+" @ "+a);
      for (int i=0;i<40;i++) {
        long p = getInt(a.add(i*4)) & 0xffffffffL;
        Function f = getFunctionAt(toAddr(p));
        if (f==null) break;
        println(String.format("  +0x%02x -> %08x %s", i*4, p, f.getName()));
      }
    }
  }
}
