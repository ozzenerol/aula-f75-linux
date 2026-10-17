import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import java.io.*;
public class DumpRange extends GhidraScript {
  public void run() throws Exception {
    String[] a = getScriptArgs();
    long lo = Long.parseLong(a[1],16), hi = Long.parseLong(a[2],16);
    DecompInterface di = new DecompInterface(); di.openProgram(currentProgram);
    PrintWriter w = new PrintWriter(new FileWriter(a[0]));
    int n=0;
    for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
      long e = f.getEntryPoint().getOffset();
      if (e < lo || e > hi) continue;
      DecompileResults r = di.decompileFunction(f, 60, monitor);
      w.println("// ===== "+f.getName()+" @ "+f.getEntryPoint());
      w.println(r.decompileCompleted()? r.getDecompiledFunction().getC() : "// FAILED");
      n++;
    }
    w.close(); println("range dumped "+n);
  }
}
