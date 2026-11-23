# FirmwareDump.py - Ghidra headless post-script (Jython 2.7 / PyGhidra compatible)
# v2: full LINEAR Thumb sweep. Pre-script ForceThumb.py already set TMode=1
# over the whole raw block. This script then:
#   1. seeds disassembly at the block start and at exception-vector targets
#      (flow-following on),
#   2. sweeps every remaining undefined gap with linear (no-flow) disassembly,
#      up to 4 passes, so the entire image decodes as Thumb exactly like the
#      capstone linear decode of the first analysis,
#   3. creates functions at BL/BLX call targets,
#   4. dumps stats, the function list and the full instruction listing.
#
# Usage (analyzeHeadless): -postScript FirmwareDump.py <outputPath>
# @category Firmware
# @runtime Jython

from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.app.cmd.function import CreateFunctionCmd

args = getScriptArgs()
outPath = args[0]

mem = currentProgram.getMemory()
listing = currentProgram.getListing()
fm = currentProgram.getFunctionManager()

block = None
for b in mem.getBlocks():
    if block is None or b.getSize() > block.getSize():
        block = b

start = block.getStart()
end = block.getEnd()
blockSize = block.getSize()

out = open(outPath, 'w')


def w(s):
    out.write(s + '\n')


w('# PROGRAM\t' + currentProgram.getName())
w('# IMAGE_BASE\t' + str(currentProgram.getImageBase().getOffset()))
w('# BLOCK\t' + block.getName() + '\t' + str(start.getOffset()) + '\t' + str(end.getOffset()) + '\t' + str(blockSize))


def disasm(addr, follow):
    try:
        cmd = DisassembleCommand(addr, None, follow)
        cmd.applyTo(currentProgram, monitor)
    except Exception:
        pass


# 1. seed: block start (flow on) + exception-vector targets
disasm(start, True)
vec_seeds = 0
try:
    for i in range(0, 32):
        a = start.add(i * 4)
        v = mem.getInt(a)
        if v > 0 and v < blockSize and (v & 1) == 1:
            disasm(start.add(v & 0xFFFFFFFE), True)
            vec_seeds += 1
except Exception:
    pass
w('# VEC_SEEDS\t' + str(vec_seeds))

# 2. sweep over undefined gaps: flow-following disassembly RESTRICTED to each
#    gap's address set, so every gap decodes linearly as Thumb
from ghidra.program.model.address import AddressSet
sweep_cmds = 0
for _pass in range(0, 4):
    intervals = []
    it = listing.getInstructions(True)
    while it.hasNext():
        ins = it.next()
        a = ins.getAddress().getOffset()
        intervals.append([a, a + ins.getLength()])
    intervals.sort()
    gaps = []
    cursor = start.getOffset()
    for s, e in intervals:
        if s > cursor:
            gaps.append((cursor, s))
        if e > cursor:
            cursor = e
    endOff = end.getOffset()
    if cursor <= endOff:
        gaps.append((cursor, endOff + 1))
    big = [g for g in gaps if g[1] - g[0] >= 2]
    if not big:
        break
    before = listing.getNumInstructions()
    for gs, ge in big[:5000]:
        ge = min(ge, endOff + 1)
        a = gs
        while a < ge - 1:
            a0 = start.add(a - start.getOffset())
            # skip only REAL instructions; Ghidra "?? undefined" data units must
            # be disassembled over, not skipped
            cu = listing.getInstructionAt(a0)
            if cu is not None:
                nxt = cu.getMaxAddress().getOffset() + 1
                a = nxt if nxt > a else a + 2
                continue
            a1 = start.add(ge - 1 - start.getOffset())
            try:
                cmd = DisassembleCommand(a0, AddressSet(a0, a1), True)
                cmd.applyTo(currentProgram, monitor)
                sweep_cmds += 1
            except Exception:
                pass
            a += 2
    if listing.getNumInstructions() == before:
        break
w('# SWEEP_CMDS\t' + str(sweep_cmds))

# 3. functions at call targets
targets = {}
it = listing.getInstructions(True)
while it.hasNext():
    ins = it.next()
    m = ins.getMnemonicString()
    if m == 'bl' or m.startswith('blx'):
        for r in ins.getReferencesFrom():
            if r.getReferenceType().isCall():
                t = r.getToAddress()
                if t is not None:
                    targets[t] = 1

created = 0
for t in sorted(targets.keys()):
    if t.compareTo(start) >= 0 and t.compareTo(end) <= 0:
        if fm.getFunctionAt(t) is None:
            try:
                f = createFunction(t, None)
                if f is not None:
                    created = created + 1
            except Exception:
                try:
                    if CreateFunctionCmd(t).applyTo(currentProgram, monitor):
                        created = created + 1
                except Exception:
                    pass
w('# CALL_TARGETS\t' + str(len(targets)))
w('# FUNCS_CREATED_FROM_CALLS\t' + str(created))

# 4. stats + dumps
w('# N_INSTRUCTIONS\t' + str(listing.getNumInstructions()))
w('# N_FUNCTIONS\t' + str(fm.getFunctionCount()))
w('# N_DEFINED_DATA\t' + str(listing.getNumDefinedData()))

fit = fm.getFunctions(True)
while fit.hasNext():
    f = fit.next()
    ep = f.getEntryPoint().getOffset()
    size = f.getBody().getNumAddresses()
    w('# FUNC\t%x\t%d\t%s' % (ep, size, f.getName()))

iit = listing.getInstructions(True)
while iit.hasNext():
    ins = iit.next()
    txt = str(ins).replace('\t', ' ')
    w('# INSTR\t%x\t%d\t%s\t%s' % (ins.getAddress().getOffset(), ins.getLength(),
                                   ins.getMnemonicString(), txt))

out.close()
print('FirmwareDump v2: wrote ' + outPath)
