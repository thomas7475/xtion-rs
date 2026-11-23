# ForceThumb.py - Ghidra headless PRE-script (runs after import, BEFORE analysis).
# Forces the TMode register to 1 (Thumb) over the whole raw image so that both
# the auto-analysis and the post-script disassemble ARM Thumb, not ARM/32.
# @category Firmware
# @runtime Jython

from java.math import BigInteger
from ghidra.program.model.lang import RegisterValue

block = None
mem = currentProgram.getMemory()
for b in mem.getBlocks():
    if block is None or b.getSize() > block.getSize():
        block = b

tmode = currentProgram.getLanguage().getRegister('TMode')
if tmode is not None:
    currentProgram.getProgramContext().setRegisterValue(
        block.getStart(), block.getEnd(), RegisterValue(tmode, BigInteger.ONE))
    println('ForceThumb: TMode=1 over ' + str(block.getStart()) + '-' + str(block.getEnd()))
else:
    println('ForceThumb: TMode register not found')
