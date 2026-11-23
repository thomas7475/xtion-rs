#!/usr/bin/env python3
"""
inno_bin_extract.py — extract files from an Inno Setup disk-spanned installer
(setup.exe + setup-1.bin, setup-2.bin, ...) without running it.

Verified against Inno Setup 5.3.6 (ANSI and Unicode), format transcribed from
innoextract (dscharrer/innoextract, src/loader, src/setup, src/stream — BSD/zlib
licensed reference implementation) and tested on the i-dong "Sports King" PC
installer (i-dong_setup.exe + multi-GB .bin slices).

Usage:
    python3 inno_bin_extract.py <setup.exe>                 # list contents
    python3 inno_bin_extract.py <setup.exe> --extract OUT   # extract to OUT/
    python3 inno_bin_extract.py <setup.exe> --info          # metadata only

The .bin slice files must sit next to the setup.exe (named <stem>-1.bin,
<stem>-2.bin, ... or <base-filename>-1.bin, ...). Listing works without them;
extraction needs them.
"""
import argparse
import binascii
import hashlib
import io
import lzma
import os
import struct
import sys
import zlib

CHUNK_MAGIC = b'zlb\x1a'
SLICE_MAGICS = (b'idska16\x1a', b'idska32\x1a')
LOADER_MAGICS = {
    # magic -> earliest Inno version (only 5.1.5+ matter here)
    bytes.fromhex('72446c507453303887665678'): (1, 2, 10),
    bytes.fromhex('72446c507453303487655678'): (4, 0, 0),
    bytes.fromhex('72446c507453303587655678'): (4, 0, 3),
    bytes.fromhex('72446c507453303687655678'): (4, 0, 10),
    bytes.fromhex('72446c507453303787655678'): (4, 1, 6),
    bytes.fromhex('72446c507453cde6d77b0b2a'): (5, 1, 5),
    bytes.fromhex('6e53575737645483aa1b0f6a'): (5, 1, 5),
}
CODEPAGES = {
    0: 'cp1252', 437: 'cp437', 850: 'cp850', 852: 'cp852', 866: 'cp866',
    932: 'cp932', 936: 'gbk', 949: 'cp949', 950: 'cp950', 1250: 'cp1250',
    1251: 'cp1251', 1252: 'cp1252', 1253: 'cp1253', 1254: 'cp1254',
    1255: 'cp1255', 1256: 'cp1256', 1257: 'cp1257', 1258: 'cp1258',
    65001: 'utf-8',
}


def vkey(a, b, c):
    return (a << 24) | (b << 16) | (c << 8)


class InnoVersion:
    def __init__(self, a, b, c, unicode=False):
        self.a, self.b, self.c = a, b, c
        self.unicode = unicode
        self.k = vkey(a, b, c)

    def at_least(self, a, b, c):
        return self.k >= vkey(a, b, c)

    def __str__(self):
        return f"{self.a}.{self.b}.{self.c}{' (unicode)' if self.unicode else ' (ansi)'}"


class Reader:
    def __init__(self, data, pos=0, item_winver_28=False):
        self.d = data
        self.p = pos
        self.item_winver_28 = item_winver_28

    def read(self, n):
        if self.p + n > len(self.d):
            raise EOFError(f"need {n} bytes at {self.p}, have {len(self.d) - self.p}")
        r = self.d[self.p:self.p + n]
        self.p += n
        return r

    def u8(self):
        return self.read(1)[0]

    def bool_(self):
        return self.u8() != 0

    def u16(self):
        return struct.unpack('<H', self.read(2))[0]

    def i16(self):
        return struct.unpack('<h', self.read(2))[0]

    def u32(self):
        return struct.unpack('<I', self.read(4))[0]

    def i32(self):
        return struct.unpack('<i', self.read(4))[0]

    def u64(self):
        return struct.unpack('<Q', self.read(8))[0]

    def i64(self):
        return struct.unpack('<q', self.read(8))[0]

    def bstr(self):
        """Length-prefixed (u32) raw byte string."""
        n = self.u32()
        if n > 0x7fffffff:
            raise ValueError(f"bad string length {n}")
        return self.read(n)

    def enum(self):
        return self.u8()

    def flagreader(self, count, pad32=True):
        """stored_flag_reader: 1 bit per flag, LSB-first in each byte;
        3-byte sets are padded to 4 (32-bit builds)."""
        nbits = count
        nbytes = (nbits + 7) // 8
        raw = self.read(nbytes)
        if nbytes == 3 and pad32:
            self.read(1)
        bits = 0
        for i, b in enumerate(raw):
            bits |= b << (8 * i)
        return bits

    def bitfield(self, nbits):
        """stored_bitfield: packed bytes, 3-byte sets padded to 4."""
        nbytes = (nbits + 7) // 8
        raw = self.read(nbytes)
        if nbytes == 3:
            self.read(1)
        bits = 0
        for i, b in enumerate(raw):
            bits |= b << (8 * i)
        return bits

    def winver(self):
        """windows_version: (build u16, minor u8, major u8) x2 + sp (u8, u8)."""
        def one():
            build = self.u16()
            minor = self.u8()
            major = self.u8()
            return (major, minor, build)
        win = one()
        nt = one()
        sp_minor = self.u8()
        sp_major = self.u8()
        return (win, nt, sp_major, sp_minor)

    def winver_range(self):
        return (self.winver(), self.winver())

    def winver_item(self):
        """Item-entry version data. Standard Inno = windows_version_range (20 B);
        the custom 5.3.6 builds used by the i-dong installers write 28 B here."""
        return self.read(28 if self.item_winver_28 else 20)

    def charset(self):
        """stored_char_set: 256-bit lead-byte set (32 bytes)."""
        raw = self.read(32)
        return raw

    def remaining(self):
        return len(self.d) - self.p


def dechunk_block(data):
    """inno_block_filter: [u32 crc32][<=4096 bytes]... -> payload."""
    out = bytearray()
    p = 0
    while p < len(data):
        if p + 4 > len(data):
            raise ValueError("truncated block chunk crc")
        crc = struct.unpack('<I', data[p:p + 4])[0]
        p += 4
        chunk = data[p:p + 4096]
        p += len(chunk)
        if binascii.crc32(chunk) & 0xffffffff != crc:
            raise ValueError(f"block chunk CRC mismatch at {p}")
        out += chunk
    return bytes(out)


def lzma1_decode(stream):
    """Raw LZMA1 stream: 5-byte props + data (no size field).
    Wrap into .lzma 'alone' format with unknown size for liblzma."""
    if len(stream) < 5:
        raise ValueError("lzma stream too short")
    dec = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE)
    return dec.decompress(stream[:5] + b'\xff' * 8 + stream[5:])


def exe_decode_5200(data, flip_high_byte=False):
    """Undo Inno Setup's >= 5.2.0 call-instruction optimization for x86 EXE/DLL
    data (port of innoextract's inno_exe_decoder_5200)."""
    out = bytearray()
    offset = 0
    i = 0
    n = len(data)
    BLOCK = 0x10000
    while i < n:
        b = data[i]; i += 1
        out.append(b)
        offset += 1
        if b != 0xe8 and b != 0xe9:
            continue
        block_left = BLOCK - ((offset - 1) % BLOCK)
        if block_left < 5:
            continue
        if i + 4 > n:
            out += data[i:]
            break
        buf = data[i:i + 4]; i += 4; offset += 4
        if buf[3] == 0x00 or buf[3] == 0xff:
            addr = offset & 0xffffff
            rel = buf[0] | (buf[1] << 8) | (buf[2] << 16)
            rel -= addr
            nb = bytes([rel & 0xff, (rel >> 8) & 0xff, (rel >> 16) & 0xff, buf[3]])
            if flip_high_byte and (rel & 0x800000):
                nb = nb[:3] + bytes([nb[3] ^ 0xff])
            out += nb
        else:
            out += buf
    return bytes(out)


def parse_setup_header_block(exe_data, header_offset, second=False):
    """Returns (version_str, InnoVersion, decompressed block bytes).
    If second=True, parses the SECOND block at the given offset (data entries)."""
    if not second:
        return parse_block(exe_data, header_offset, with_version=True)
    return parse_block(exe_data, header_offset, with_version=False)


def parse_block(exe_data, offset, with_version=True):
    """Parse an Inno block (optionally preceded by the 64-byte version ID)."""
    ver = None
    vstr = None
    if with_version:
        # 64-byte version ID (uncompressed) at offset
        version_raw = exe_data[offset:offset + 64]
        vstr = version_raw.split(b'\x00')[0].decode('latin1')
        if 'Inno Setup' not in vstr:
            raise ValueError(f"no Inno version string at {offset:#x}: {vstr!r}")
        # parse "(a.b.c)" and optional " (u)"
        import re
        m = re.search(r'\((\d+)\.(\d+)\.(\d+)(?:\.\d+)?\)(\s*\(u\))?', vstr)
        if not m:
            raise ValueError(f"cannot parse version: {vstr!r}")
        ver = InnoVersion(int(m.group(1)), int(m.group(2)), int(m.group(3)),
                          unicode=bool(m.group(4)))
        p = offset + 64
    else:
        p = offset
    expected_crc, stored_size, compressed = struct.unpack('<IIB', exe_data[p:p + 9])
    if binascii.crc32(exe_data[p + 4:p + 9]) & 0xffffffff != expected_crc:
        raise ValueError("block header CRC mismatch")
    p += 9
    block = exe_data[p:p + stored_size]
    payload = dechunk_block(block)
    if compressed == 0:
        data = payload
    elif compressed == 1:
        data = lzma1_decode(payload)
    else:
        raise ValueError(f"unsupported block compression {compressed}")
    if with_version:
        return vstr, ver, data, stored_size
    return None, None, data, stored_size


def find_loader_table(exe_data):
    for magic, ver in LOADER_MAGICS.items():
        i = exe_data.find(magic)
        if i >= 0:
            return i
    raise ValueError("Inno loader magic not found")


def parse_loader_offsets(exe_data):
    pos = find_loader_table(exe_data)
    magic = exe_data[pos:pos + 12]
    p = pos + 12
    ver = LOADER_MAGICS[magic]
    crc_data = bytearray(magic)

    def rd():
        nonlocal p
        v, = struct.unpack('<I', exe_data[p:p + 4])
        crc_data.extend(exe_data[p:p + 4])
        p += 4
        return v

    if vkey(*ver) >= vkey(5, 1, 5):
        revision = rd()
        if revision != 1:
            print(f"warning: loader revision {revision}", file=sys.stderr)
    skip = rd()  # unknown / checksum
    exe_offset = rd()
    if vkey(*ver) < vkey(4, 1, 6):
        exe_compressed = rd()
    exe_uncompressed = rd()
    exe_crc = rd()
    header_offset = rd()
    data_offset = rd()
    expected = rd()
    ok = binascii.crc32(bytes(crc_data)) & 0xffffffff == expected
    return {
        'table_offset': pos, 'earliest_version': ver, 'exe_offset': exe_offset,
        'exe_uncompressed_size': exe_uncompressed, 'exe_crc': exe_crc,
        'header_offset': header_offset, 'data_offset': data_offset,
        'crc_ok': ok,
    }


def decode_bytes(raw, codepage, unicode_, lead_bytes=None):
    if not raw:
        return ''
    if unicode_:
        try:
            return raw.decode('utf-16-le', errors='replace')
        except Exception:
            return repr(raw)
    codec = CODEPAGES.get(codepage, 'cp1252')
    if lead_bytes and any(lead_bytes[b >> 3] & (1 << (b & 7)) for b in raw):
        # DBCS-aware walk
        out = []
        i = 0
        while i < len(raw):
            b = raw[i]
            if lead_bytes[b >> 3] & (1 << (b & 7)) and i + 1 < len(raw):
                out.append(bytes(raw[i:i + 2]).decode(codec, errors='replace'))
                i += 2
            else:
                out.append(bytes([b]).decode(codec, errors='replace'))
                i += 1
        return ''.join(out)
    return raw.decode(codec, errors='replace')


class SetupInfo:
    """Parsed setup.0 header + entry table for Inno 5.x (verified 5.3.6)."""

    def __init__(self, ver, header_data, quirk_byte=False, item_winver_28=True):
        self.ver = ver
        self.quirk_byte = quirk_byte
        # The i-dong installers were built with a custom Inno 5.3.6 that writes
        # 28-byte version data in item entries (standard Inno writes 20).
        self.r = Reader(header_data, item_winver_28=item_winver_28)
        self.languages = []
        self.files = []
        self.data_entries = []
        self.wizard_images = []
        self.parse()

    # ---- header ----
    def parse(self):
        v = self.ver
        r = self.r
        h = {}
        h['app_name'] = r.bstr()
        h['app_versioned_name'] = r.bstr()
        if v.at_least(1, 3, 0):
            h['app_id'] = r.bstr()
        h['app_copyright'] = r.bstr()
        if v.at_least(1, 3, 0):
            h['app_publisher'] = r.bstr()
            h['app_publisher_url'] = r.bstr()
        if v.at_least(5, 1, 13):
            h['app_support_phone'] = r.bstr()
        if v.at_least(1, 3, 0):
            h['app_support_url'] = r.bstr()
            h['app_updates_url'] = r.bstr()
            h['app_version'] = r.bstr()
        h['default_dir_name'] = r.bstr()
        h['default_group_name'] = r.bstr()
        h['base_filename'] = r.bstr()
        if v.at_least(1, 3, 3):
            h['uninstall_files_dir'] = r.bstr()
        if v.at_least(1, 3, 6):
            h['uninstall_name'] = r.bstr()
            h['uninstall_icon'] = r.bstr()
        if v.at_least(1, 3, 14):
            h['app_mutex'] = r.bstr()
        if v.at_least(3, 0, 0):
            h['default_user_name'] = r.bstr()
            h['default_user_organisation'] = r.bstr()
        if v.at_least(4, 0, 0):
            h['default_serial'] = r.bstr()
        if v.at_least(4, 2, 4):
            h['app_readme_file'] = r.bstr()
            h['app_contact'] = r.bstr()
            h['app_comments'] = r.bstr()
            h['app_modify_path'] = r.bstr()
        if v.at_least(5, 3, 8):
            h['create_uninstall_registry_key'] = r.bstr()
        if v.at_least(5, 5, 0):
            h['close_applications_filter'] = r.bstr()
        if v.at_least(5, 5, 6):
            h['setup_mutex'] = r.bstr()
        if v.at_least(5, 2, 5):
            h['license_text'] = r.bstr()
            h['info_before'] = r.bstr()
            h['info_after'] = r.bstr()
        if v.at_least(5, 2, 1) and not v.at_least(5, 3, 10):
            h['uninstaller_signature'] = r.bstr()
        if v.at_least(5, 2, 5):
            h['compiled_code'] = r.bstr()
        if v.at_least(2, 0, 6) and not v.unicode:
            h['lead_bytes'] = r.charset()
        else:
            h['lead_bytes'] = None
        self.h = h

        self.language_count = r.u32() if v.at_least(4, 0, 0) else 1
        self.message_count = r.u32() if v.at_least(4, 2, 1) else 0
        self.permission_count = r.u32() if v.at_least(4, 1, 0) else 0
        if v.at_least(2, 0, 0):
            self.type_count = r.u32()
            self.component_count = r.u32()
        else:
            self.type_count = self.component_count = 0
        self.task_count = r.u32() if v.at_least(2, 0, 0) else 0
        self.directory_count = r.u32()
        self.file_count = r.u32()
        self.data_entry_count = r.u32()
        self.icon_count = r.u32()
        self.ini_entry_count = r.u32()
        self.registry_entry_count = r.u32()
        self.delete_entry_count = r.u32()
        self.uninstall_delete_entry_count = r.u32()
        self.run_entry_count = r.u32()
        self.uninstall_run_entry_count = r.u32()

        self.winver_header = r.winver_range()  # windows_version_range: begin+end
        if not v.at_least(6, 4, 0):
            h['back_color'] = r.u32()
        if v.at_least(1, 3, 3) and not v.at_least(6, 4, 0):
            h['back_color2'] = r.u32()
        if not v.at_least(5, 5, 7):
            h['image_back_color'] = r.u32()
        if v.at_least(2, 0, 0) and not v.at_least(5, 0, 4):
            h['small_image_back_color'] = r.u32()
        # password
        if v.at_least(6, 4, 0):
            r.read(4)
        elif v.at_least(5, 3, 9):
            h['password_sha1'] = r.read(20)
        elif v.at_least(4, 2, 0):
            h['password_md5'] = r.read(16)
        else:
            h['password_crc32'] = r.u32()
        if v.at_least(4, 2, 2):
            h['password_salt'] = r.read(8)
        if v.at_least(4, 0, 0):
            h['extra_disk_space_required'] = r.i64()
            h['slices_per_disk'] = r.u32()
        if v.at_least(1, 3, 0):
            h['uninstall_log_mode'] = r.enum()
        if v.at_least(1, 3, 6):
            h['dir_exists_warning'] = r.enum()
        if v.at_least(3, 0, 4):
            h['privileges_required'] = r.enum()
        if v.at_least(4, 0, 10):
            h['show_language_dialog'] = r.enum()
            h['language_detection'] = r.enum()
        if v.at_least(5, 3, 9):
            h['compression'] = r.enum()
        elif v.at_least(4, 2, 6):
            h['compression'] = r.enum()  # 0 stored, 1 zlib, 2 bzip2, 3 lzma1
        if v.at_least(5, 1, 0):
            h['architectures_allowed'] = r.flagreader(4)
            h['architectures_installed_in_64bit'] = r.flagreader(4)
        if v.at_least(5, 2, 1) and not v.at_least(5, 3, 10):
            h['signed_uninstaller_original_size'] = r.u32()
            h['signed_uninstaller_header_checksum'] = r.u32()
        if self.quirk_byte:
            # 5.3.6 compiler quirk observed in the i-dong installers: one extra
            # byte (value 1) after the signed-uninstaller checksum that innoextract
            # does not expect (innoextract 1.9 fails on these files).
            h['quirk_byte'] = r.u8()
        if v.at_least(5, 3, 3):
            h['disable_dir_page'] = r.enum()
            h['disable_program_group_page'] = r.enum()
        if v.at_least(5, 3, 6) and not v.at_least(5, 5, 0):
            h['uninstall_display_size'] = r.u32()
        elif v.at_least(5, 5, 0):
            h['uninstall_display_size'] = r.u64()
        h['options'] = self.header_flags()

        # languages
        for _ in range(self.language_count):
            lang = {}
            if v.at_least(4, 0, 0):
                lang['name'] = r.bstr()
            lang['language_name'] = r.bstr()
            lang['dialog_font'] = r.bstr()
            lang['title_font'] = r.bstr()
            lang['welcome_font'] = r.bstr()
            lang['copyright_font'] = r.bstr()
            if v.at_least(4, 0, 0):
                lang['data'] = r.bstr()
            if v.at_least(4, 0, 1):
                lang['license_text'] = r.bstr()
                lang['info_before'] = r.bstr()
                lang['info_after'] = r.bstr()
            lang['language_id'] = r.u32()
            if v.at_least(4, 2, 2) and not v.unicode:
                lang['codepage'] = r.u32() or 1252
            else:
                if v.unicode and not v.at_least(5, 3, 0):
                    r.u32()
                lang['codepage'] = 65001  # utf16le marker
            lang['dialog_font_size'] = r.u32()
            lang['title_font_size'] = r.u32()
            lang['welcome_font_size'] = r.u32()
            lang['copyright_font_size'] = r.u32()
            if v.at_least(5, 2, 3):
                lang['right_to_left'] = r.bool_()
            self.languages.append(lang)
        if self.languages:
            self.codepage = self.languages[0]['codepage']
        else:
            self.codepage = 65001 if v.unicode else 1252

        # messages (come directly after languages for >= 4.0.0)
        self.messages = []
        for _ in range(self.message_count):
            m = {}
            m['name'] = r.bstr()
            m['value'] = r.bstr()
            m['language'] = r.i32()
            self.messages.append(m)
        # permissions
        self.permissions = [r.bstr() for _ in range(self.permission_count)]
        # types
        for _ in range(self.type_count):
            r.bstr(); r.bstr()
            if v.at_least(4, 0, 1):
                r.bstr()
            r.bstr()
            r.winver_range()
            r.flagreader(1)
            if v.at_least(4, 0, 3):
                r.enum()
            if v.at_least(4, 0, 0):
                r.u64()
        # components: name, description, types, languages(>=4.0.1), check(>=4.0.0) = 5 strings
        for _ in range(self.component_count):
            for _ in range(5):
                r.bstr()
            r.u64(); r.i32(); r.bool_()
            r.winver_range()
            if v.at_least(4, 2, 3):
                r.flagreader(5)
            if v.at_least(4, 0, 0):
                r.u64()
        # tasks
        for _ in range(self.task_count):
            for _ in range(6):
                r.bstr()
            r.i32(); r.bool_()
            r.winver_range()
            n = 2 + (1 if v.at_least(2, 0, 5) else 0) + (1 if v.at_least(2, 0, 6) else 0) \
                + (1 if v.at_least(4, 2, 3) else 0)
            r.flagreader(n)
        # directories
        for _ in range(self.directory_count):
            r.bstr()          # name
            self.condition_data()
            if v.at_least(2, 0, 11):
                r.u32()       # attributes
            r.winver_item()
            if v.at_least(4, 1, 0):
                r.i16()       # permission
            if v.at_least(5, 2, 0):
                r.flagreader(5)
        # files
        for _ in range(self.file_count):
            f = {}
            f['source'] = r.bstr()
            f['destination'] = r.bstr()
            f['install_font_name'] = r.bstr()
            if v.at_least(5, 2, 5):
                f['strong_assembly_name'] = r.bstr()
            self.condition_data()
            r.winver_item()
            f['location'] = r.u32()
            f['attributes'] = r.u32()
            f['external_size'] = r.u64() if v.at_least(4, 0, 0) else r.u32()
            if v.at_least(4, 1, 0):
                f['permission'] = r.i16()
            nflags = 4 + 3 + 2 + 1 + 1 + 2 + 1 + 1 + 3 + 1 + 1 + 1 + 1 + 1 + 2 + 3 + 1
            f['options'] = r.flagreader(nflags)
            f['type'] = r.enum()
            self.files.append(f)
        # icons
        for _ in range(self.icon_count):
            for _ in range(6):
                r.bstr()
            self.condition_data()
            if v.at_least(5, 3, 5):
                r.bstr()
            r.winver_item()
            r.i32()  # icon_index
            if v.at_least(1, 3, 24):
                r.i32()  # show_command
            if v.at_least(1, 3, 15):
                r.enum()  # close_on_exit
            if v.at_least(2, 0, 7):
                r.u16()  # hotkey
            n = 1 + 1 + 1 + (1 if v.at_least(5, 0, 3) else 0)
            r.flagreader(n)
        # ini entries
        for _ in range(self.ini_entry_count):
            for _ in range(4):
                r.bstr()
            self.condition_data()
            r.winver_item()
            r.flagreader(5)
        # registry
        for _ in range(self.registry_entry_count):
            r.bstr()
            r.bstr()  # name (bits != 16)
            r.bstr()  # value
            self.condition_data()
            r.winver_item()
            r.u32()  # hive
            if v.at_least(4, 1, 0):
                r.i16()
            r.enum()  # type
            n = 2 + 4 + 1 + 2 + 1 + 1 + 1 + (2 if v.at_least(5, 1, 0) else 0)
            r.flagreader(n)
        # delete entries (install + uninstall)
        for cnt in (self.delete_entry_count, self.uninstall_delete_entry_count):
            for _ in range(cnt):
                r.bstr()
                self.condition_data()
                r.winver_item()
                r.enum()
        # run entries (install + uninstall)
        for cnt in (self.run_entry_count, self.uninstall_run_entry_count):
            for _ in range(cnt):
                for _ in range(7):
                    r.bstr()
                self.condition_data()
                r.winver_item()
                if v.at_least(1, 3, 24):
                    r.i32()
                r.enum()  # wait
                n = (1 if v.at_least(1, 2, 3) else 0) + (1 if v.at_least(1, 3, 9) else 0) \
                    + 4 + 1 + (2 if v.at_least(5, 1, 10) else 0) + (1 if v.at_least(5, 2, 0) else 0)
                r.flagreader(n)
        # wizard images + dlls (>= 4.0.0: AFTER all entries)
        self.wizard_images = []
        self.wizard_images.append(r.bstr())
        if v.at_least(2, 0, 0):
            self.wizard_images.append(r.bstr())
        comp = self.h.get('compression', 1)
        if comp == 2 or (comp == 3 and v.k == vkey(4, 1, 5)) or (comp == 1 and v.at_least(4, 2, 6)):
            self.h['decompressor_dll'] = r.bstr()
        # EncryptionUsed = bit 39 of options
        if self.h['options'] & (1 << 39):
            self.h['decrypt_dll'] = r.bstr()
        self.leftover = r.remaining()

    def parse_data_entries(self, second_block_data):
        """Phase 2: data entries live in a SECOND compressed block after the
        primary header stream ends (Inno >= 4.0.0)."""
        v = self.ver
        r = Reader(second_block_data)
        self.data_entries = []
        for _ in range(self.data_entry_count):
            d = {}
            d['first_slice'] = r.u32()
            d['last_slice'] = r.u32()
            d['chunk_offset'] = r.u32()
            if v.at_least(4, 0, 1):
                d['file_offset'] = r.u64()
            if v.at_least(4, 0, 0):
                d['file_size'] = r.u64()
                d['chunk_size'] = r.u64()
            else:
                d['file_size'] = r.u32()
                d['chunk_size'] = r.u32()
            if v.at_least(6, 4, 0):
                d['md5'] = r.read(32)
            elif v.at_least(5, 3, 9):
                d['sha1'] = r.read(20)
            elif v.at_least(4, 2, 0):
                d['md5'] = r.read(16)
            elif v.at_least(4, 0, 1):
                d['crc32'] = r.u32()
            else:
                d['adler32'] = r.u32()
            d['filetime'] = r.i64()
            d['file_version_ms'] = r.u32()
            d['file_version_ls'] = r.u32()
            n = 2 + (1 if v.at_least(4, 0, 10) else 0) + (1 if v.at_least(4, 1, 0) else 0) \
                + (1 if v.at_least(4, 1, 8) else 0) + (1 if v.at_least(4, 2, 0) else 0) \
                + (1 if v.at_least(4, 2, 2) else 0) + (1 if v.at_least(4, 2, 5) else 0) \
                + (1 if v.at_least(5, 1, 13) else 0)
            d['flags'] = r.flagreader(n)
            self.data_entries.append(d)
        self.data_leftover = r.remaining()

    def condition_data(self):
        r = self.r
        r.bstr(); r.bstr(); r.bstr(); r.bstr()  # components, tasks, languages, check

    def header_flags(self):
        v = self.ver
        r = self.r
        n = 0
        n += 1  # DisableStartupPrompt
        if not v.at_least(5, 3, 10):
            n += 1  # Uninstallable
        n += 1  # CreateAppDir
        n += 1  # AllowNoIcons
        n += 1  # AlwaysRestart (>= 3.0.3)
        n += 1  # AlwaysUsePersonalGroup
        n += 4  # WindowVisible/ShowCaption/Resizable/StartMaximized
        n += 1  # EnableDirDoesntExistWarning
        n += 1  # Password
        n += 1  # AllowRootDirectory
        n += 1  # DisableFinishedPage
        n += 1  # ChangesAssociations
        n += 1  # CreateUninstallRegKey
        n += 1  # UsePreviousAppDir
        n += 1  # BackColorHorizontal
        n += 1  # UsePreviousGroup
        n += 1  # UpdateUninstallLogAppName
        n += 7  # UsePreviousSetupType..DisableReadyPage
        n += 2  # AlwaysShowDirOnReadyPage/Group
        n += 1  # AllowUNCPath
        n += 2  # UserInfoPage/UsePreviousUserInfo
        n += 1  # UninstallRestartComputer
        n += 1  # RestartIfNeededByRun
        n += 1  # ShowTasksTreeLines
        n += 1  # AllowCancelDuringInstall
        n += 1  # WizardImageStretch
        n += 2  # AppendDefaultDirName/GroupName
        n += 1  # EncryptionUsed
        n += 1  # ChangesEnvironment
        if v.at_least(5, 1, 7) and not v.unicode:
            n += 1  # ShowUndisplayableLanguages
        n += 1  # SetupLogging
        n += 1  # SignedUninstaller
        return r.flagreader(n)


class SliceSource:
    """Reads data slices: exe-embedded (data_offset != 0) or external .bin files."""

    def __init__(self, exe_path, data_offset, base_filename, slices_per_disk, stem):
        self.exe_path = exe_path
        self.exe_dir = os.path.dirname(os.path.abspath(exe_path))
        self.exe_data = open(exe_path, 'rb').read()
        self.data_offset = data_offset
        self.slices_per_disk = slices_per_disk or 1
        self.candidates = [stem, base_filename] if base_filename else [stem]
        self._cache = {}

    def _slice_name(self, base, slice_index):
        if self.slices_per_disk == 1:
            return f"{base}-{slice_index + 1}.bin"
        major = slice_index // self.slices_per_disk + 1
        minor = slice_index % self.slices_per_disk
        return f"{base}-{major}{chr(ord('a') + minor)}.bin"

    def open_slice(self, slice_index):
        if slice_index in self._cache:
            return self._cache[slice_index]
        data = None
        if slice_index == 0 and self.data_offset:
            data = self.exe_data
            base = self.data_offset
        else:
            for cand in self.candidates:
                name = self._slice_name(cand, slice_index)
                path = os.path.join(self.exe_dir, name)
                if not os.path.exists(path):
                    path2 = os.path.join(self.exe_dir, name.lower())
                    if os.path.exists(path2):
                        path = path2
                if os.path.exists(path):
                    raw = open(path, 'rb').read()
                    if raw[:8] not in SLICE_MAGICS:
                        raise ValueError(f"bad slice magic in {path}")
                    size, = struct.unpack('<I', raw[8:12])
                    data = raw
                    # chunk offsets are ABSOLUTE file offsets in external slices
                    base = 0
                    break
            if data is None:
                raise FileNotFoundError(
                    f"missing slice {slice_index} (tried "
                    + ', '.join(self._slice_name(c, slice_index) for c in self.candidates)
                    + f" in {self.exe_dir})")
        self._cache[slice_index] = (data, base)
        return data, base


def read_chunk(src, entry, header_compression):
    """Read one file's compressed chunk (possibly spanning slices) and return
    the decompressed chunk data."""
    size = entry['chunk_size'] + 4  # + 'zlb\x1a' magic
    first = entry['first_slice']
    off = entry['chunk_offset']
    buf = bytearray()
    slice_idx = first
    data, base = src.open_slice(slice_idx)
    pos = base + off
    while len(buf) < size:
        if pos >= len(data):
            slice_idx += 1
            data, base = src.open_slice(slice_idx)
            pos = base
            continue
        take = min(size - len(buf), len(data) - pos)
        buf += data[pos:pos + take]
        pos += take
    raw = bytes(buf)
    if raw[:4] != CHUNK_MAGIC:
        raise ValueError(f"bad chunk magic at slice {first}+{off:#x}: {raw[:4]!r}")
    payload = raw[4:4 + entry['chunk_size']]
    compressed = (entry['flags'] & (1 << 7)) != 0  # ChunkCompressed
    if not compressed:
        payload = payload
    elif header_compression == 1:
        payload = zlib.decompress(payload)
    elif header_compression == 2:
        import bz2
        payload = bz2.decompress(payload)
    elif header_compression == 3:
        payload = lzma1_decode(payload)
    if entry['flags'] & (1 << 4):  # CallInstructionOptimized -> filter 5.2.0
        payload = exe_decode_5200(payload, flip_high_byte=False)
    return payload
    method = header_compression
    if method == 1:
        return zlib.decompress(payload)
    if method == 2:
        import bz2
        return bz2.decompress(payload)
    if method == 3:
        return lzma1_decode(payload)
    return payload


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('setup', help='path to the Inno setup .exe')
    ap.add_argument('--extract', metavar='DIR', help='extract files into DIR')
    ap.add_argument('--info', action='store_true', help='print metadata only')
    ap.add_argument('--list', action='store_true', help='list files (default)')
    args = ap.parse_args()

    exe_data = open(args.setup, 'rb').read()
    off = parse_loader_offsets(exe_data)
    print(f"[loader] table @ {off['table_offset']:#x}  header @ {off['header_offset']:#x}  "
          f"data_offset={off['data_offset']:#x}  crc_ok={off['crc_ok']}")
    if not off['data_offset']:
        print("[loader] disk-spanned installer: file data is in external .bin slices")

    # primary block: 64-byte version + 9-byte block header + stored_size
    ho = off['header_offset']
    vstr, ver, header_data, b1_size = parse_setup_header_block(exe_data, ho)
    second_block_offset = ho + 64 + 9 + b1_size
    print(f"[setup.0] version: {vstr!r}  parsed as {ver}  "
          f"decompressed header: {len(header_data)} bytes")

    info = None
    # The i-dong installers use a custom Inno 5.3.6 build with two deviations
    # from the standard format; auto-detect which combination parses cleanly.
    for quirk in (False, True):
        for wv28 in (False, True):
            try:
                cand = SetupInfo(ver, header_data, quirk_byte=quirk, item_winver_28=wv28)
                if cand.leftover == 0 and cand.language_count > 0 and cand.languages:
                    lid = cand.languages[0].get('language_id', -1)
                    if 0 <= lid <= 0xffff:
                        info = cand
                        notes = []
                        if quirk:
                            notes.append('header quirk byte')
                        if wv28:
                            notes.append('28-byte item version data')
                        if notes:
                            print('[setup.0] custom-build quirks applied: ' + ', '.join(notes))
                        break
            except Exception:
                continue
        if info is not None:
            break
    if info is None:
        # last resort: parse without validation
        info = SetupInfo(ver, header_data)
        print('[setup.0] WARNING: header parse incomplete/inconsistent', file=sys.stderr)
    # phase 2: data entries in the second block
    try:
        _, _, second_data, _sz = parse_setup_header_block(exe_data, second_block_offset, second=True)
        info.parse_data_entries(second_data)
        if info.data_leftover:
            print(f"[setup.1] WARNING: {info.data_leftover} unparsed bytes in data-entry block")
        else:
            print("[setup.1] data-entry block fully consumed (parse consistent)")
    except Exception as e:
        print(f"[setup.1] ERROR: data-entry block parse failed: {e}", file=sys.stderr)
    lb = info.h.get('lead_bytes')
    def dec(raw):
        return decode_bytes(raw, info.codepage, ver.unicode, lb)

    print(f"[header] app={dec(info.h['app_name'])!r} version={dec(info.h['app_version'])!r} "
          f"base_filename={dec(info.h['base_filename'])!r}")
    print(f"[header] default_dir={dec(info.h['default_dir_name'])!r} "
          f"compression={info.h.get('compression')} slices_per_disk={info.h.get('slices_per_disk')}")
    print(f"[header] counts: languages={info.language_count} messages={info.message_count} "
          f"types={info.type_count} components={info.component_count} tasks={info.task_count} "
          f"dirs={info.directory_count} files={info.file_count} data={info.data_entry_count} "
          f"icons={info.icon_count} ini={info.ini_entry_count} registry={info.registry_entry_count} "
          f"delete={info.delete_entry_count} run={info.run_entry_count}")
    if info.leftover:
        print(f"[setup.0] WARNING: {info.leftover} unparsed bytes at end of header "
              f"(parse misalignment?)")
    else:
        print("[setup.0] header fully consumed (parse consistent)")

    for lang in info.languages:
        print(f"[language] {lang.get('name', b'').decode('latin1', 'replace')!r} "
              f"id={lang['language_id']} codepage={lang['codepage']}")

    total = sum(d['file_size'] for d in info.data_entries)
    print(f"[data] {info.data_entry_count} data entries, total uncompressed {total:,} bytes")

    for i, f in enumerate(info.files):
        loc = f['location']
        d = info.data_entries[loc] if loc < len(info.data_entries) else None
        src_name = dec(f['source'])
        dst_name = dec(f['destination'])
        size = d['file_size'] if d else f.get('external_size', 0)
        slice_info = (f"slice {d['first_slice']}..{d['last_slice']} @{d['chunk_offset']:#x}"
                      if d else "external")
        print(f"  [{i:4d}] {src_name}  ->  {dst_name}  ({size:,} B, {slice_info})")

    if args.extract:
        outdir = args.extract
        os.makedirs(outdir, exist_ok=True)
        stem = os.path.splitext(os.path.basename(args.setup))[0]
        src = SliceSource(args.setup, off['data_offset'],
                          dec(info.h['base_filename']),
                          info.h.get('slices_per_disk', 1), stem)
        compression = info.h.get('compression', 1)
        ok = fail = 0
        for i, f in enumerate(info.files):
            loc = f['location']
            if loc >= len(info.data_entries):
                continue
            d = info.data_entries[loc]
            if d['file_size'] == 0:
                continue
            try:
                chunk = read_chunk(src, d, compression)
            except FileNotFoundError as e:
                print(f"  SKIP [{i}] {dec(f['source'])}: {e}")
                fail += 1
                continue
            except Exception as e:
                print(f"  FAIL [{i}] {dec(f['source'])}: {e}")
                fail += 1
                continue
            filedata = chunk[d['file_offset']:d['file_offset'] + d['file_size']]
            if len(filedata) != d['file_size']:
                print(f"  FAIL [{i}] {dec(f['source'])}: chunk too small")
                fail += 1
                continue
            if 'md5' in d and d['md5']:
                if hashlib.md5(filedata).digest() != d['md5']:
                    print(f"  WARN [{i}] md5 mismatch for {dec(f['source'])}")
            rel = dec(f['destination']).lstrip('\\/').replace('\\', '/')
            rel = rel.replace('{app}/', '').replace('{app}\\', '')
            outpath = os.path.join(outdir, rel)
            os.makedirs(os.path.dirname(outpath) or outdir, exist_ok=True)
            with open(outpath, 'wb') as fh:
                fh.write(filedata)
            ok += 1
        print(f"[extract] done: {ok} extracted, {fail} failed")
    elif not args.info:
        pass


if __name__ == '__main__':
    main()
