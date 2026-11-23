#!/usr/bin/env python3
"""Live end-to-end test: ranged-GET the first slice prefix of the i-dong Sports
King A installer from download.i-dong.com, extract the first files with
inno_bin_extract.py and verify their MD5 checksums."""
import io
import os
import struct
import sys
import zipfile
import zlib
import urllib.request

sys.path.insert(0, '/home/z/my-project/scripts')
from inno_bin_extract import SetupInfo, parse_loader_offsets, parse_setup_header_block, \
    SliceSource, read_chunk, exe_decode_5200
import hashlib

ZIP_URL = 'http://download.i-dong.com/setup/yundongwang-ver/idong_1.9.04_windows_yundongwang_A_setup.zip'
SETUP = '/home/z/my-project/upload/idong_setup.exe'
OUTDIR = '/home/z/my-project/analysis/live_test'
os.makedirs(OUTDIR, exist_ok=True)


def ranged_get(url, start, end):
    req = urllib.request.Request(url, headers={'Range': f'bytes={start}-{end}'})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def main():
    # 1. total size + EOCD
    req = urllib.request.Request(ZIP_URL, method='HEAD')
    with urllib.request.urlopen(req, timeout=60) as r:
        total = int(r.headers['Content-Length'])
    print(f'zip size: {total:,} bytes')
    tail = ranged_get(ZIP_URL, total - 65536, total - 1)
    # find EOCD
    i = tail.rfind(b'PK\x05\x06')
    eocd = tail[i:i + 22]
    _, _, _, _, n_entries, cd_size, cd_off, _ = struct.unpack('<IHHHHIIH', eocd)
    print(f'EOCD: {n_entries} entries, cd at {cd_off:#x} size {cd_size:#x}')
    # zip64?
    if cd_off == 0xFFFFFFFF or n_entries == 0xFFFF:
        # zip64 EOCD locator is right before EOCD
        j = tail.rfind(b'PK\x06\x07', 0, i)
        loc = tail[j:j + 20]
        _, _, _, z64_eocd_off = struct.unpack('<IIQ', loc[4:24 - 4]) if len(loc) >= 20 else (0, 0, 0, 0)
        z64 = ranged_get(ZIP_URL, z64_eocd_off, z64_eocd_off + 56 - 1)
        assert z64[:4] == b'PK\x06\x06'
        n_entries = struct.unpack('<Q', z64[32:40])[0]
        cd_size = struct.unpack('<Q', z64[40:48])[0]
        cd_off = struct.unpack('<Q', z64[48:56])[0]
        print(f'zip64 EOCD: {n_entries} entries, cd at {cd_off:#x} size {cd_size:#x}')
    # 2. central directory
    cd = ranged_get(ZIP_URL, cd_off, cd_off + cd_size - 1)
    members = []
    p = 0
    while p + 46 <= len(cd):
        sig, ver_m, ver_n, flags, method, mtime, mdate, crc, csize, usize, nlen, elen, clen, disk, iattr, eattr, lho = \
            struct.unpack('<IHHHHHHIIIHHHHHII', cd[p:p + 46])
        name = cd[p + 46:p + 46 + nlen].decode('utf-8', 'replace')
        members.append((name, method, csize, usize, lho, crc))
        p += 46 + nlen + elen + clen
    print('zip members:')
    for m in members:
        print(f'  {m[0]}  method={m[1]} csize={m[2]:,} usize={m[3]:,} local@{m[4]:#x}')
    # 3. first .bin member
    bins = sorted([m for m in members if m[0].lower().endswith('.bin')], key=lambda m: m[0])
    print(f'total {len(bins)} .bin slices')
    if not bins:
        print('NO .bin members found!')
        return
    name, method, csize, usize, lho, crc = bins[0]
    # local header: 30 bytes + name + extra
    lh = ranged_get(ZIP_URL, lho, lho + 29)
    sig, _, flags2, method2, _, _, crc2, csize2, usize2, nlen2, elen2 = \
        struct.unpack('<IHHHHHIIIHH', lh[:30])
    data_off = lho + 30 + nlen2 + elen2
    print(f'first bin: {name} data at {data_off:#x} (csize {csize2:,} method {method2})')
    if method2 == 0:
        prefix = ranged_get(ZIP_URL, data_off, data_off + 900_000 - 1)
    else:
        # deflate: fetch compressed prefix, inflate incrementally
        comp = ranged_get(ZIP_URL, data_off, data_off + 1_000_000 - 1)
        d = zlib.decompressobj(-15)
        prefix = d.decompress(comp, 900_000)
        print(f'  inflated {len(prefix):,} bytes from {len(comp):,} compressed')
    binpath = os.path.join(OUTDIR, name)
    open(binpath, 'wb').write(prefix)
    print(f'saved {len(prefix):,} bytes -> {binpath}')
    # also copy setup exe there under matching stem
    stem = os.path.splitext(os.path.basename(SETUP))[0]
    import shutil
    shutil.copy(SETUP, os.path.join(OUTDIR, os.path.basename(SETUP)))
    # 5. parse + extract first files
    exe_data = open(SETUP, 'rb').read()
    off = parse_loader_offsets(exe_data)
    vstr, ver, header_data, b1 = parse_setup_header_block(exe_data, off['header_offset'])
    info = SetupInfo(ver, header_data, quirk_byte=True)
    _, _, second_data, _ = parse_setup_header_block(exe_data, off['header_offset'] + 64 + 9 + b1, second=True)
    info.parse_data_entries(second_data)
    src = SliceSource(os.path.join(OUTDIR, os.path.basename(SETUP)), off['data_offset'],
                      '', info.h.get('slices_per_disk', 1), stem)
    ok = 0
    for i, f in enumerate(info.files[:12]):
        loc = f['location']
        if loc >= len(info.data_entries):
            continue
        d = info.data_entries[loc]
        if d['file_size'] == 0:
            continue
        try:
            chunk = read_chunk(src, d, info.h.get('compression', 1))
            filedata = chunk[d['file_offset']:d['file_offset'] + d['file_size']]
        except Exception as e:
            print(f'  [{i}] {f["destination"]}: {e}')
            continue
        md5 = hashlib.md5(filedata).hexdigest()
        expect = d.get('md5', b'').hex() if d.get('md5') else '?'
        status = 'MD5 OK' if md5 == expect else f'MD5 MISMATCH (expect {expect})'
        print(f'  [{i}] {f["destination"].decode("gbk", "replace")} '
              f'{d["file_size"]:,} B -> {status}')
        if md5 == expect:
            outp = os.path.join(OUTDIR, os.path.basename(f['destination'].decode('gbk', 'replace').replace('\\', '/')))
            open(outp, 'wb').write(filedata)
            ok += 1
    print(f'\nLIVE TEST: {ok} files extracted + MD5-verified from the live server')


if __name__ == '__main__':
    main()
