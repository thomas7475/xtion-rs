#!/usr/bin/env python3
"""Cross-version comparison of PS1080 firmware 5.7.9 vs 5.8.22.

Consumes: the extracted 5.7.9 hotfix (FLA/ + UsbUpdate/), the repository's
5.8.22 mirror, and the two Ghidra dumps produced by FirmwareDump.py.
Produces: work/firmware-analysis/results/summary.md + data.json + string
diff files - the evidence base for the updated firmware-analysis PDF.
"""
import hashlib
import json
import re
import struct
import subprocess
from pathlib import Path

ROOT = Path('/home/z/my-project/work/firmware-analysis')
FW579_FLADIR = ROOT / 'fw579' / 'FLA'
FW579_USB = ROOT / 'fw579' / 'UsbUpdate'
FW5822_FLADIR = Path('/home/z/my-project/work/xtion-rs/firmware/5.8.22')
FW5822_ZIP = Path('/home/z/my-project/download/firmware/asus-cdn/5.8.22-cdn-verification/FWUpdate_5_8_22.zip')
RESULTS = ROOT / 'results'
RESULTS.mkdir(parents=True, exist_ok=True)

D = {}  # data.json payload


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def u16_words(p, n=16):
    b = Path(p).read_bytes()[:2 * n]
    return list(struct.unpack('<%dH' % (len(b) // 2), b))


# ---------------------------------------------------------------- 1. headers
D['headers'] = {}
for tag, p in (('fw579', FW579_FLADIR / 'source.out.bin.fla'),
               ('fw5822', FW5822_FLADIR / 'source.out.bin.fla'),
               ('boot579', FW579_FLADIR / 'BootManager.out.bin.fla'),
               ('boot5822', FW5822_FLADIR / 'BootManager.out.bin.fla')):
    D['headers'][tag] = {
        'size': Path(p).stat().st_size,
        'sha256': sha(p),
        'u16': u16_words(p),
    }

D['bootmanager_identical'] = (D['headers']['boot579']['sha256'] ==
                              D['headers']['boot5822']['sha256'])
_b9 = (FW579_FLADIR / 'BootManager.out.bin.fla').read_bytes()
_b22 = (FW5822_FLADIR / 'BootManager.out.bin.fla').read_bytes()
D['bootmanager_body_identical'] = _b9[6:] == _b22[6:]
D['bootmanager_first_diff'] = next(
    (i for i in range(min(len(_b9), len(_b22))) if _b9[i] != _b22[i]), None)

# ------------------------------------------------------- 2. image size table
rows = []
all_names = sorted(set(os_name.name for os_name in list(FW579_FLADIR.iterdir()) +
                       list(FW5822_FLADIR.iterdir()))
                   - {'checksums.txt', 'README.md', 'UsbUpdate_RD108.ini'})
for name in all_names:
    p9, p22 = FW579_FLADIR / name, FW5822_FLADIR / name
    e = {'name': name}
    if p9.exists():
        e['size579'] = p9.stat().st_size
        e['sha579'] = sha(p9)
    if p22.exists():
        e['size5822'] = p22.stat().st_size
        e['sha5822'] = sha(p22)
    if p9.exists() and p22.exists():
        if e['sha579'] == e['sha5822']:
            e['status'] = 'identical'
        else:
            b9, b22 = p9.read_bytes(), p22.read_bytes()
            # bytes 0-1: image type; 2-3: build; 4-5: version word (Packer stamp)
            if e['size579'] == e['size5822'] and b9[6:] == b22[6:]:
                e['status'] = 'stamp-only (body identical)'
            else:
                e['first_diff'] = next(
                    (i for i in range(min(len(b9), len(b22))) if b9[i] != b22[i]), None)
                e['status'] = 'changed'
    else:
        e['status'] = 'only-5.8.22' if p22.exists() else 'only-5.7.9'
    rows.append(e)
D['images'] = rows

# first-bytes peek of the interesting parameter images
D['param_peek'] = {}
for name in ('AlgorithmParams.xml.bin.fla', 'FixedParamsRD108.xml.bin.fla',
             'ImageCmos.xml.bin.fla', 'DepthCmos.xml.bin.fla'):
    ent = {}
    for tag, d in (('579', FW579_FLADIR), ('5822', FW5822_FLADIR)):
        p = d / name
        if p.exists():
            ent[tag] = Path(p).read_bytes()[:16].hex()
    D['param_peek'][name] = ent

# ------------------------------------------------------------- 3. ini files
def parse_ini(p):
    txt = Path(p).read_text(errors='replace')
    files, kv = [], {}
    for line in txt.splitlines():
        m = re.match(r'\s*\[File(\d+)\]\s*', line)
        if m:
            files.append({'idx': int(m.group(1))})
            continue
        m = re.match(r'\s*([A-Za-z0-9_]+)\s*=\s*(.*)', line)
        if m:
            if files:
                files[-1][m.group(1)] = m.group(2).strip()
            else:
                kv[m.group(1)] = m.group(2).strip()
    return {'kv': kv, 'files': files, 'raw': txt}

D['ini'] = {}
for tag, p in (('579', FW579_USB / 'UsbUpdate_RD108.ini'),
               ('5822', FW5822_FLADIR / 'UsbUpdate_RD108.ini')):
    if p.exists():
        D['ini'][tag] = parse_ini(p)
for tag, p in (('swonly579', FW579_USB / 'UsbUpdate_RD108_SWOnly.ini'),
               ('factory579', FW579_USB / 'UsbUpdate_Factory.ini')):
    if p.exists():
        D['ini'][tag] = {'raw': p.read_text(errors='replace')}

# 5.8.22 zip listing (does its updater carry the SWOnly/Factory ini variants?)
D['zip5822'] = {'entries': []}
try:
    import zipfile
    with zipfile.ZipFile(FW5822_ZIP) as z:
        D['zip5822']['entries'] = z.namelist()
except Exception as e:
    D['zip5822']['error'] = str(e)

# ------------------------------------------------------------- 4. strings
def strings_of(path, encoding=None):
    cmd = ['strings'] + (['-e', encoding] if encoding else []) + ['-n', '4', str(path)]
    r = subprocess.run(cmd, capture_output=True, text=True, errors='replace')
    return set(s.strip() for s in r.stdout.splitlines() if len(s.strip()) >= 4)

D['strings'] = {}
for enc_tag, enc in (('utf16', 'l'), ('ascii', None)):
    s9 = strings_of(FW579_FLADIR / 'source.out.bin.fla', enc)
    s22 = strings_of(FW5822_FLADIR / 'source.out.bin.fla', enc)
    only9 = sorted(s9 - s22)
    only22 = sorted(s22 - s9)
    (RESULTS / ('strings_%s_only579.txt' % enc_tag)).write_text('\n'.join(only9))
    (RESULTS / ('strings_%s_only5822.txt' % enc_tag)).write_text('\n'.join(only22))
    D['strings'][enc_tag] = {
        'count579': len(s9), 'count5822': len(s22),
        'shared': len(s9 & s22),
        'only579': len(only9), 'only5822': len(only22),
        'only579_list': only9, 'only5822_list': only22,
    }

# markers of interest
def find_in_file(path, needle_bytes):
    data = Path(path).read_bytes()
    out, start = [], 0
    while True:
        i = data.find(needle_bytes, start)
        if i < 0:
            return out
        out.append(i)
        start = i + 1

markers = {}
for tag, p in (('579', FW579_FLADIR / 'source.out.bin.fla'),
               ('5822', FW5822_FLADIR / 'source.out.bin.fla')):
    data = Path(p).read_bytes()
    m = {}
    # MG/BR adjacent u16 magic pair
    for i in range(0, len(data) - 4, 2):
        if data[i] == 0x47 and data[i+1] == 0x4d and data[i+2] == 0x52 and data[i+3] == 0x42:
            m.setdefault('MG_BR_pair', []).append(i)
    m['dead_u32'] = find_in_file(p, struct.pack('<I', 0xdead))
    m['dead_u16'] = find_in_file(p, struct.pack('<H', 0xdead))
    m['f31f_u16'] = find_in_file(p, struct.pack('<H', 0xf31f))
    m['f31f_u32'] = find_in_file(p, struct.pack('<I', 0xf31f))
    m['task_list_ascii'] = (find_in_file(p, b'c,T,DC,TD') or
                            find_in_file(p, 'c,T,DC,TD'.encode('utf-16-le')))
    m['packer_ascii'] = (find_in_file(p, b'Packer') or
                         find_in_file(p, 'Packer'.encode('utf-16-le')))
    m.setdefault('MG_BR_pair', [])
    markers[tag] = m
D['markers'] = markers

# ------------------------------------------------- 5. Ghidra dump analysis
def parse_dump(path):
    meta, funcs, instrs = {}, [], []
    for line in Path(path).read_text(errors='replace').splitlines():
        parts = line.split('\t')
        if parts[0] == '# FUNC':
            funcs.append((int(parts[1], 16), int(parts[2]), parts[3]))
        elif parts[0] == '# INSTR':
            instrs.append((int(parts[1], 16), int(parts[2]), parts[3],
                           parts[4] if len(parts) > 4 else ''))
        elif parts[0].startswith('# '):
            meta[parts[0][2:].split('\t')[0]] = parts[1] if len(parts) > 1 else ''
    return meta, funcs, instrs


CMP_IMM = re.compile(r'#\s*(?:0x)?([0-9a-fA-F]+)')
OPCODE_MAX = 42  # protocol opcodes are 0..42 (+ KILL 999, never an imm8)


def dispatcher_scan(instrs):
    """Find dense cmp-imm chains over the real opcode range 0..42."""
    cmps = []
    for off, ln, mnem, full in instrs:
        if mnem != 'cmp':
            continue
        m = CMP_IMM.search(full)
        if not m:
            continue
        try:
            imm = int(m.group(1), 16)
        except ValueError:
            continue
        if 0 <= imm <= OPCODE_MAX:
            cmps.append((off, imm))
    cmps.sort()
    best = None
    i = 0
    while i < len(cmps):
        j = i
        while j < len(cmps) and cmps[j][0] - cmps[i][0] <= 0x1000:
            j += 1
        window = cmps[i:j]
        imms = sorted(set(x[1] for x in window))
        score = (len(imms), len(window))
        if len(window) >= 20 and len(imms) >= 15:
            if best is None or score > best[0]:
                best = (score, cmps[i][0], cmps[j-1][0], imms, len(window))
        i = j
    return cmps, best


BL_TARGET = re.compile(r'\bblx?\s+#(-?0x[0-9a-f]+)')


def call_graph(instrs, size):
    """Distinct in-range BL/BLX call targets = function starts under this method."""
    n_bl = 0
    targets = set()
    for off, ln, mnem, full in instrs:
        if mnem == 'bl' or mnem.startswith('blx'):
            n_bl += 1
            m = BL_TARGET.search(full)
            if m:
                t = int(m.group(1), 16)
                if 0 <= t < size:
                    targets.add(t)
    return n_bl, sorted(targets)

D['ghidra'] = {}
for tag, dump in (('5822', RESULTS / 'ghidra_5822.txt'),
                  ('579', RESULTS / 'ghidra_579.txt')):
    if not dump.exists():
        D['ghidra'][tag] = {'missing': True}
        continue
    meta, funcs, instrs = parse_dump(dump)
    cmps, best = dispatcher_scan(instrs)
    size = D['headers'].get('fw' + tag, {}).get('size', 1)
    n_bl, targets = call_graph(instrs, size)
    ent = {
        'meta': meta,
        'n_functions': len(funcs),
        'func_total_bytes': sum(f[1] for f in funcs),
        'largest_functions': sorted(funcs, key=lambda f: -f[1])[:10],
        'n_cmp_imm_le62': len(cmps),
        'bl_instructions': n_bl,
        'call_target_functions': len(targets),
    }
    if best:
        ent['dispatcher'] = {
            'range': '0x%x-0x%x' % (best[1], best[2]),
            'cmp_count': best[4],
            'distinct_opcodes': best[3],
            'n_distinct': len(best[3]),
        }
    D['ghidra'][tag] = ent

# dispatcher diff
if all('dispatcher' in D['ghidra'].get(t, {}) for t in ('579', '5822')):
    a = set(D['ghidra']['579']['dispatcher']['distinct_opcodes'])
    b = set(D['ghidra']['5822']['dispatcher']['distinct_opcodes'])
    D['opcode_diff'] = {
        'in_5822_only': sorted(b - a),
        'in_579_only': sorted(a - b),
        'shared': sorted(a & b),
    }

# ---------------------------------------------------------------- 6. write
(RESULTS / 'data.json').write_text(json.dumps(D, indent=1)[:2_000_000])

L = []
A = L.append
A('# PS1080 firmware 5.7.9 vs 5.8.22 - comparison evidence (2026-11-23)')
A('')
A('## 1. Main image headers (first 16 u16 words, little-endian)')
A('')
A('| Image | Size | u16[0..7] | u16[8..15] |')
A('|---|---:|---|---|')
for tag in ('fw579', 'fw5822', 'boot579', 'boot5822'):
    h = D['headers'][tag]
    w1 = ' '.join('%04x' % x for x in h['u16'][:8])
    w2 = ' '.join('%04x' % x for x in h['u16'][8:16])
    A('| %s | %d | %s | %s |' % (tag, h['size'], w1, w2))
A('')
A('BootManager identical across versions: **%s**' % D['bootmanager_identical'])
A('')
A('## 2. Flash image inventory')
A('')
A('| Image | 5.7.9 (B) | 5.8.22 (B) | Status |')
A('|---|---:|---:|---|')
for e in rows:
    A('| %s | %s | %s | %s |' % (
        e['name'],
        str(e.get('size579', '-')), str(e.get('size5822', '-')), e['status']))
A('')
A('## 3. Parameter image first-16-bytes peek')
A('')
for name, ent in D['param_peek'].items():
    A('- %s: 5.7.9=%s 5.8.22=%s' % (name, ent.get('579', '-'), ent.get('5822', '-')))
A('')
A('## 4. Updater ini (flash map)')
A('')
for tag in ('579', '5822'):
    if tag in D['ini'] and 'kv' in D['ini'][tag]:
        ini = D['ini'][tag]
        A('### %s: %d file entries; kv=%s' % (tag, len(ini['files']),
           json.dumps(ini['kv'], ensure_ascii=False)))
        for f in ini['files']:
            A('  - %s' % json.dumps(f, ensure_ascii=False))
A('')
A('## 5. Constant markers')
A('')
A('| Marker | 5.7.9 | 5.8.22 |')
A('|---|---|---|')
for key in ('MG_BR_pair', 'dead_u32', 'dead_u16', 'f31f_u16', 'f31f_u32', 'task_list_ascii', 'packer_ascii'):
    m9 = D['markers']['579'].get(key, [])
    m22 = D['markers']['5822'].get(key, [])
    f9 = ('0x%x' % m9[0] + (' (+%d)' % (len(m9)-1) if len(m9) > 1 else '')) if m9 else '-'
    f22 = ('0x%x' % m22[0] + (' (+%d)' % (len(m22)-1) if len(m22) > 1 else '')) if m22 else '-'
    A('| %s | %s | %s |' % (key, f9, f22))
A('')
A('## 6. Strings')
A('')
A('| Encoding | 5.7.9 count | 5.8.22 count | shared | only-5.7.9 | only-5.8.22 |')
A('|---|---:|---:|---:|---:|---:|')
for enc in ('utf16', 'ascii'):
    s = D['strings'][enc]
    A('| %s | %d | %d | %d | %d | %d |' % (enc, s['count579'], s['count5822'], s['shared'], s['only579'], s['only5822']))
A('')
A('Full lists: results/strings_*_only*.txt')
A('')
A('## 7. Ghidra (ARM:LE:32:v5t, Thumb forced, AIF re-analysis)')
A('')
for tag, label in (('579', '5.7.9'), ('5822', '5.8.22')):
    g = D['ghidra'].get(tag, {})
    if g.get('missing'):
        A('%s: dump missing' % label)
        continue
    A('### %s - instructions=%s functions=%s definedData=%s (meta=%s)' % (
        label, g['meta'].get('N_INSTRUCTIONS'), g['meta'].get('N_FUNCTIONS'),
        g['meta'].get('N_DEFINED_DATA'),
        {k: g['meta'][k] for k in ('TMODE', 'AIF', 'VEC_SEEDS') if k in g['meta']}))
    A('- call graph: %d bl/blx instructions, %d distinct in-range call targets (function starts)'
      % (g.get('bl_instructions', 0), g.get('call_target_functions', 0)))
    if 'dispatcher' in g:
        d = g['dispatcher']
        A('- dispatcher: %s, %d cmp-imm, %d distinct opcodes: %s' % (
            d['range'], d['cmp_count'], d['n_distinct'], d['distinct_opcodes']))
A('')
if 'opcode_diff' in D:
    A('## 8. Opcode dispatch diff')
    A('')
    A('- in 5.8.22 only: %s' % D['opcode_diff']['in_5822_only'])
    A('- in 5.7.9 only: %s' % D['opcode_diff']['in_579_only'])
    A('- shared count: %d' % len(D['opcode_diff']['shared']))
    A('')
A('## 9. 5.8.22 package zip entries')
A('')
A(', '.join(D['zip5822'].get('entries', [])) or str(D['zip5822'].get('error', '')))
A('')
(RESULTS / 'summary.md').write_text('\n'.join(L))
print('comparison done ->', RESULTS / 'summary.md')
print('bootmanager_identical =', D['bootmanager_identical'])
for tag in ('579', '5822'):
    g = D['ghidra'].get(tag, {})
    if not g.get('missing'):
        print(tag, 'instr=', g['meta'].get('N_INSTRUCTIONS'), 'funcs=', g['n_functions'],
              'dispatcher=', g.get('dispatcher', {}).get('range'))
