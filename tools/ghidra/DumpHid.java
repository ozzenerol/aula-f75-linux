import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.address.*;
import java.util.*;
import java.io.*;

public class DumpHid extends GhidraScript {
  public void run() throws Exception {
    String outDir = getScriptArgs()[0];
    int depth = Integer.parseInt(getScriptArgs()[1]);
    String[] targets = {"HidD_SetFeature","HidD_GetFeature","WriteFile","ReadFile","GetOverlappedResult","HidP_GetCaps","HidD_GetAttributes","CreateFileW","CreateFileA","SetupDiGetClassDevsW"};
    FunctionManager fm = currentProgram.getFunctionManager();
    Map<Function,Integer> seen = new LinkedHashMap<>();
    Map<Function,Set<String>> why = new HashMap<>();
    Deque<Function> q = new ArrayDeque<>();
    SymbolTable st = currentProgram.getSymbolTable();
    for (String t : targets) {
      for (Symbol s : st.getSymbols(t)) {
        Set<Address> addrs = new HashSet<>();
        addrs.add(s.getAddress());
        for (Reference r : getReferencesTo(s.getAddress())) addrs.add(r.getFromAddress());
        // follow thunks/pointers
        for (Address a : new ArrayList<>(addrs)) for (Reference r : getReferencesTo(a)) addrs.add(r.getFromAddress());
        for (Address a : addrs) {
          Function f = fm.getFunctionContaining(a);
          if (f != null && !f.isThunk() && !f.isExternal()) {
            if (!seen.containsKey(f)) { seen.put(f,0); q.add(f); }
            why.computeIfAbsent(f,k->new TreeSet<>()).add(t);
          }
        }
      }
    }
    while (!q.isEmpty()) {
      Function f = q.poll(); int d = seen.get(f);
      if (d >= depth) continue;
      for (Function c : f.getCallingFunctions(monitor)) {
        if (!seen.containsKey(c)) { seen.put(c,d+1); q.add(c); why.computeIfAbsent(c,k->new TreeSet<>()).add("caller-of:"+f.getName()); }
      }
    }
    DecompInterface di = new DecompInterface(); di.openProgram(currentProgram);
    PrintWriter idx = new PrintWriter(new FileWriter(outDir+"/index.txt"));
    PrintWriter all = new PrintWriter(new FileWriter(outDir+"/decomp.c"));
    for (Map.Entry<Function,Integer> e : seen.entrySet()) {
      Function f = e.getKey();
      idx.println(f.getEntryPoint()+"\t"+f.getName()+"\tdepth="+e.getValue()+"\t"+why.get(f));
      DecompileResults r = di.decompileFunction(f, 60, monitor);
      all.println("// ===== "+f.getName()+" @ "+f.getEntryPoint()+" depth="+e.getValue()+" "+why.get(f));
      if (r.decompileCompleted()) all.println(r.getDecompiledFunction().getC()); else all.println("// FAILED "+r.getErrorMessage());
    }
    idx.close(); all.close();
    println("dumped "+seen.size()+" functions");
  }
}
