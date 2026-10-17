import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.data.*;
import java.io.*; import java.util.*;
public class FindStr extends GhidraScript {
  public void run() throws Exception {
    String[] a = getScriptArgs();
    PrintWriter w = new PrintWriter(new FileWriter(a[0]));
    List<String> needles = Arrays.asList(a).subList(1,a.length);
    DecompInterface di = new DecompInterface(); di.openProgram(currentProgram);
    Set<Function> done = new HashSet<>();
    DataIterator it = currentProgram.getListing().getDefinedData(true);
    while (it.hasNext()) {
      Data d = it.next();
      if (!(d.getValue() instanceof String)) continue;
      String s = (String)d.getValue();
      for (String n : needles) if (s.contains(n)) {
        for (Reference r : getReferencesTo(d.getAddress())) {
          Function f = getFunctionContaining(r.getFromAddress());
          if (f==null || !done.add(f)) continue;
          DecompileResults res = di.decompileFunction(f,90,monitor);
          w.println("// ===== "+f.getName()+" @ "+f.getEntryPoint()+" str=\""+s+"\"");
          w.println(res.decompileCompleted()? res.getDecompiledFunction().getC() : "// FAILED");
        }
      }
    }
    w.close();
  }
}
