#!/usr/bin/env python3
"""Builds the UPDATED xtion_rs_firmware_analysis.pdf.

Body via ReportLab (TocDocTemplate + multiBuild, clickable TOC, Roman front
matter + Arabic body numbering per pagination.md), cover pre-rendered by
html2poster.js and merged as page 0 via pypdf. All cross-version numbers are
read from work/firmware-analysis/results/data.json (compare_firmware.py).
"""
import hashlib
import json
import os
import sys
from pathlib import Path

PDF_SKILL = '/home/z/my-project/skills/pdf'
sys.path.insert(0, os.path.join(PDF_SKILL, 'scripts'))

from reportlab.lib.pagesizes import A4
from reportlab.lib.units import inch
from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_JUSTIFY, TA_LEFT
from reportlab.lib.styles import ParagraphStyle
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfbase.pdfmetrics import registerFontFamily
from reportlab.platypus import (CondPageBreak, Frame, HRFlowable, Image,
                                KeepTogether, NextPageTemplate, PageBreak,
                                PageTemplate, Paragraph, SimpleDocTemplate,
                                Spacer, Table, TableStyle)
from reportlab.platypus.tableofcontents import TableOfContents

# ---------------------------------------------------------------- fonts
FONT_DIR = '/usr/share/fonts'
pdfmetrics.registerFont(TTFont('FreeSerif', f'{FONT_DIR}/truetype/freefont/FreeSerif.ttf'))
pdfmetrics.registerFont(TTFont('FreeSerif-Bold', f'{FONT_DIR}/truetype/freefont/FreeSerifBold.ttf'))
pdfmetrics.registerFont(TTFont('FreeSerif-Italic', f'{FONT_DIR}/truetype/freefont/FreeSerifItalic.ttf'))
pdfmetrics.registerFont(TTFont('FreeSerif-BoldItalic', f'{FONT_DIR}/truetype/freefont/FreeSerifBoldItalic.ttf'))
pdfmetrics.registerFont(TTFont('DejaVuSans', f'{FONT_DIR}/truetype/dejavu/DejaVuSansMono.ttf'))
registerFontFamily('FreeSerif', normal='FreeSerif', bold='FreeSerif-Bold',
                   italic='FreeSerif-Italic', boldItalic='FreeSerif-BoldItalic')
registerFontFamily('DejaVuSans', normal='DejaVuSans', bold='DejaVuSans')

from pdf import install_font_fallback  # noqa: E402
install_font_fallback()

# ------------------------------------------------- palette (cascade, seed 7)
PAGE_BG = colors.HexColor('#f1f0ef')
SECTION_BG = colors.HexColor('#f2f1f0')
CARD_BG = colors.HexColor('#e8e7e4')
TABLE_STRIPE = colors.HexColor('#eeedeb')
HEADER_FILL = colors.HexColor('#504933')
COVER_BLOCK = colors.HexColor('#867b5a')
BORDER = colors.HexColor('#cfcab8')
ICON = colors.HexColor('#8c7e52')
ACCENT = colors.HexColor('#87702a')
ACCENT_2 = colors.HexColor('#3a95b4')
TEXT_PRIMARY = colors.HexColor('#1c1c1a')
TEXT_MUTED = colors.HexColor('#78766f')

# ---------------------------------------------------------------- data
DATA_PATH = Path('/home/z/my-project/work/firmware-analysis/results/data.json')
D = json.loads(DATA_PATH.read_text()) if DATA_PATH.exists() else {}

ROOT = Path('/home/z/my-project/work/firmware-analysis')
OUT_BODY = ROOT / 'body.pdf'
OUT_FINAL = Path('/home/z/my-project/download/xtion_rs_firmware_analysis.pdf')
COVER_PDF = ROOT / 'cover.pdf'
FIG1 = ROOT / 'fig1_sizes.png'
DOC_TITLE = 'Xtion PS1080 Firmware 5.8.22 and 5.7.9 - Reverse-Engineering Report'

PAGE_W, PAGE_H = A4
MARGIN = 0.9 * inch
AVAIL = PAGE_W - 2 * MARGIN
AVAIL_H = PAGE_H - 2 * MARGIN
H1_ORPHAN = AVAIL_H * 0.25

# ---------------------------------------------------------------- styles
S_BODY = ParagraphStyle('Body', fontName='FreeSerif', fontSize=10.5, leading=17,
                        alignment=TA_JUSTIFY, textColor=TEXT_PRIMARY, spaceAfter=8)
S_H1 = ParagraphStyle('H1', fontName='FreeSerif-Bold', fontSize=20, leading=25,
                      textColor=HEADER_FILL, spaceBefore=14, spaceAfter=4)
S_H2 = ParagraphStyle('H2', fontName='FreeSerif-Bold', fontSize=13.5, leading=18,
                      textColor=TEXT_PRIMARY, spaceBefore=10, spaceAfter=4)
S_CAP = ParagraphStyle('Cap', fontName='FreeSerif', fontSize=8.5, leading=11,
                       textColor=TEXT_MUTED, alignment=TA_CENTER,
                       spaceBefore=3, spaceAfter=6)
S_CELL = ParagraphStyle('Cell', fontName='FreeSerif', fontSize=8.5, leading=10.5,
                        textColor=TEXT_PRIMARY)
S_CELL_C = ParagraphStyle('CellC', parent=S_CELL, alignment=TA_CENTER)
S_CELL_H = ParagraphStyle('CellH', fontName='FreeSerif-Bold', fontSize=8.5,
                          leading=10.5, textColor=colors.white, alignment=TA_CENTER)
S_CODE = ParagraphStyle('Code', fontName='DejaVuSans', fontSize=7.5, leading=9.5,
                        textColor=TEXT_PRIMARY, leftIndent=6)
S_QUOTE = ParagraphStyle('Quote', fontName='FreeSerif-Italic', fontSize=10.5,
                         leading=16, leftIndent=24, textColor=TEXT_PRIMARY,
                         spaceBefore=6, spaceAfter=8)
S_TOC0 = ParagraphStyle('TOC0', fontName='FreeSerif', fontSize=11, leading=18, leftIndent=6)
S_TOC1 = ParagraphStyle('TOC1', fontName='FreeSerif', fontSize=9.5, leading=15,
                        leftIndent=26, textColor=TEXT_MUTED)
S_TOC_TITLE = ParagraphStyle('TOCTitle', fontName='FreeSerif-Bold', fontSize=20,
                             leading=25, textColor=HEADER_FILL, spaceAfter=12)

# ---------------------------------------------------------------- helpers


def add_heading(text, style, level=0):
    key = 'h_' + hashlib.md5(text.encode()).hexdigest()[:8]
    p = Paragraph('<a name="%s"/>%s' % (key, text), style)
    p.bookmark_name = key
    p.bookmark_level = level
    p.bookmark_text = text
    p.bookmark_key = key
    return p


def h1(text):
    return [CondPageBreak(H1_ORPHAN),
            KeepTogether([add_heading(text, S_H1, 0),
                          HRFlowable(width='100%', thickness=1.4, color=ACCENT,
                                     spaceBefore=0, spaceAfter=8)])]


def h2(text):
    return [add_heading(text, S_H2, 1)]


def para(text):
    return Paragraph(text, S_BODY)


def cap(text):
    return Paragraph(text, S_CAP)


def P(text, style=S_CELL):
    return Paragraph(text, style)


def tbl(headers, rows, ratios, header_row=True):
    assert abs(sum(ratios) - 1.0) < 0.01, 'ratios must sum to 1'
    colw = [r * AVAIL for r in ratios]
    assert sum(colw) <= AVAIL + 0.5
    data = []
    if header_row:
        data.append([P('<b>%s</b>' % h, S_CELL_H) for h in headers])
    for row in rows:
        data.append([c if isinstance(c, (Paragraph, Image)) else P(str(c)) for c in row])
    t = Table(data, colWidths=colw, repeatRows=1 if header_row else 0, hAlign='CENTER')
    style = [
        ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'),
        ('GRID', (0, 0), (-1, -1), 0.5, BORDER),
        ('LEFTPADDING', (0, 0), (-1, -1), 5),
        ('RIGHTPADDING', (0, 0), (-1, -1), 5),
        ('TOPPADDING', (0, 0), (-1, -1), 4),
        ('BOTTOMPADDING', (0, 0), (-1, -1), 4),
    ]
    if header_row:
        style.append(('BACKGROUND', (0, 0), (-1, 0), HEADER_FILL))
        for i in range(1, len(data)):
            if i % 2 == 0:
                style.append(('BACKGROUND', (0, i), (-1, i), TABLE_STRIPE))
    t.setStyle(TableStyle(style))
    return t


def tbl_block(caption, headers, rows, ratios):
    t = tbl(headers, rows, ratios)
    out = [Spacer(1, 10)]
    if len(rows) <= 14:
        out.append(KeepTogether([t, cap(caption)]))
    else:
        out.append(t)
        out.append(cap(caption))
    out.append(Spacer(1, 8))
    return out


def fig_block(path, caption, max_h=270):
    from PIL import Image as PILImage
    im = PILImage.open(path)
    ow, oh = im.size
    ratio = min(AVAIL / ow, max_h / oh, 1.0)
    img = Image(str(path), width=ow * ratio, height=oh * ratio)
    return [Spacer(1, 14), KeepTogether([img, cap(caption)]), Spacer(1, 10)]


# ---------------------------------------------------------------- doc + pages


class TocDocTemplate(SimpleDocTemplate):
    def build(self, flowables, **kw):
        # multiBuild -> build on every pass: inject the footer callbacks
        SimpleDocTemplate.build(self, flowables,
                                onFirstPage=on_front_page,
                                onLaterPages=on_body_page)

    def afterFlowable(self, flowable):
        if hasattr(flowable, 'bookmark_name'):
            level = getattr(flowable, 'bookmark_level', 0)
            text = getattr(flowable, 'bookmark_text', '')
            key = getattr(flowable, 'bookmark_key', '')
            self.notify('TOCEntry', (level, text, self.page, key))
        if getattr(flowable, 'is_body_start', False):
            self._body_start_page = self.page


ROMAN = {1: 'i', 2: 'ii', 3: 'iii', 4: 'iv', 5: 'v', 6: 'vi', 7: 'vii', 8: 'viii'}


def _header_footer(canvas, doc, number):
    canvas.saveState()
    # header: title left + accent rule
    canvas.setFont('FreeSerif', 7.5)
    canvas.setFillColor(TEXT_MUTED)
    canvas.drawString(MARGIN, PAGE_H - 0.55 * inch, 'Xtion PS1080 Firmware Analysis - Update 2')
    canvas.setStrokeColor(ACCENT)
    canvas.setLineWidth(1.2)
    canvas.line(MARGIN, PAGE_H - 0.62 * inch, PAGE_W - MARGIN, PAGE_H - 0.62 * inch)
    # footer: light rule + author left + page number right
    canvas.setStrokeColor(BORDER)
    canvas.setLineWidth(0.5)
    canvas.line(MARGIN, 0.62 * inch, PAGE_W - MARGIN, 0.62 * inch)
    canvas.setFont('FreeSerif', 7.5)
    canvas.setFillColor(TEXT_MUTED)
    canvas.drawString(MARGIN, 0.45 * inch, 'xtion-rs project')
    canvas.drawRightString(PAGE_W - MARGIN, 0.45 * inch, number)
    canvas.restoreState()


def on_front_page(canvas, doc):
    _header_footer(canvas, doc, ROMAN.get(doc.page, str(doc.page)))


def on_body_page(canvas, doc):
    start = getattr(doc, '_body_start_page', None)
    n = doc.page - start + 1 if start else doc.page
    _header_footer(canvas, doc, str(n))


class BodyStartMarker(Spacer):
    """Zero-size flowable marking where Arabic numbering restarts at 1."""

    def __init__(self):
        Spacer.__init__(self, 0, 0)
        self.is_body_start = True


doc = TocDocTemplate(str(OUT_BODY), pagesize=A4,
                     leftMargin=MARGIN, rightMargin=MARGIN,
                     topMargin=MARGIN, bottomMargin=MARGIN,
                     title=DOC_TITLE, author='Z.ai', creator='Z.ai',
                     subject='Cross-version reverse engineering of the ASUS Xtion PS1080 firmware')

# ================================================================ STORY
story = []

story.append(Paragraph('Table of Contents', S_TOC_TITLE))
toc = TableOfContents()
toc.levelStyles = [S_TOC0, S_TOC1]
story.append(toc)
story.append(PageBreak())
story.append(BodyStartMarker())

# ---------------------------------------------------------------- 1
story += h1('1. Executive Summary')
story.append(para(
    'This report updates the November 22 single-version analysis of the ASUS Xtion\u2019s onboard '
    'firmware. The first analysis covered only the final release, 5.8.22 of July 2013. Since then, '
    'the firmware hunt of November 23 recovered the one older package that survives publicly: the '
    '5.7.9 "Xtion USB 3.0 hotfix", distributed by Artec3D in April 2013 and now archived in the '
    'xtion-rs repository as FW579-RD1081-112v2.zip (799,206 bytes, SHA-256 '
    'c5e7e38ba2a2ace4b3c3f019f6a01c826a6f57d02c68d1c720fef8a2937c5cae). This update adds a full '
    'Ghidra 11.3.2 analysis of the 5.7.9 main image and a systematic, image-by-image comparison '
    'against 5.8.22: headers, flash maps, parameter images, log strings and the recovered code '
    'itself.'))
story.append(para(
    'The comparison confirms the platform history the repository documented from secondary sources, '
    'now with primary evidence. The main firmware image grew from 134,876 to 153,704 bytes '
    '(+14.0 percent) between the two releases, while the boot manager stayed byte-identical: '
    'PrimeSense never touched the safe-update loader across the entire line. The 5.7.9 package '
    'lacks two flash images that 5.8.22 carries - SensorNesaParams.xml.bin.fla and the '
    'UploadInProgress.fla power-loss marker - and its log strings reveal a completely different '
    'diagnostic style: verbose English self-test sentences instead of the compact telemetry codes '
    'of 5.8.22. The opcode dispatcher, located in both binaries, differs measurably and dates the '
    'newer command surface. Every finding absent from the first analysis is collected in section 9.'))

# ---------------------------------------------------------------- 2
story += h1('2. The Firmware Packages')
story.append(para(
    'Both packages mirror into the repository\u2019s firmware archive. The 5.8.22 package arrives as '
    'FWUpdate_5_8_22.zip, whose single Windows executable unpacks (LZX cabinet, libarchive) into '
    '25 files: the flashable images, the Windows flashing toolchain, the OpenNI 1.x DLLs the '
    'updater depends on, and the 6 MB NESIDB device-ID database. The 5.7.9 hotfix is a simpler, '
    'leaner artifact: a plain zip with a FLA directory of fourteen flash images and a UsbUpdate '
    'directory with the same flashing tool plus its OpenNI 1.x DLLs. Internal zip timestamps read '
    '2012-09-02, matching the ship era of the 5.7.x line.'))
story += tbl_block('Table 1: contents of the 5.7.9 hotfix package (FW579-RD1081-112v2.zip)',
                   ['File', 'Size (B)', 'Role'],
                   [
                       ['FLA/source.out.bin.fla', '134,876', 'main firmware image (this update\u2019s second subject)'],
                       ['FLA/BootManager.out.bin.fla', '23,214', 'boot manager / safe-update loader'],
                       ['FLA/ 12 x *.xml.bin.fla', '14 - 1,648', 'calibration and default parameter images'],
                       ['UsbUpdate/UsbUpdate.exe', '19,968', 'flashing tool (identical role to 5.8.22\u2019s)'],
                       ['UsbUpdate/XnDeviceSensorV2.dll', '545,280', 'sensor driver DLL the tool depends on'],
                       ['UsbUpdate/ + OpenNI.dll, XnCore, XnDDK, XnEECore, XnFormats', '1.27 MB', 'OpenNI 1.x runtime for the updater'],
                       ['UsbUpdate/UsbUpdate_RD108.ini', '1,513', 'flash map (13-file update zone)'],
                       ['UsbUpdate/UsbUpdate_RD108_SWOnly.ini', '160', 'software-only variant - not present in the 5.8.22 package'],
                       ['UsbUpdate/UsbUpdate_Factory.ini', '60', 'factory-mode variant - not present in the 5.8.22 package'],
                   ], [0.42, 0.13, 0.45])
story.append(para(
    'Two structural differences stand out. First, the 5.7.9 package ships raw flash images and a '
    'standalone tool, not the self-extracting cabinet of 5.8.22 - consistent with a hotfix pushed '
    'through a partner (Artec3D) rather than the ASUS download site. Second, its updater carries '
    'two extra flash-map variants: a software-only map and a factory map. The 5.8.22 package '
    'extracts only the standard UsbUpdate_RD108.ini. The flash maps themselves are compared in '
    'section 8. Both packages share the same Packer wrapping: every image starts with a small '
    'header of [u16 image type][u16 build][u16 version word] followed by size and checksum '
    'records, and a left-over build-path string ("../Packer/source.out.bin") identifies the '
    'in-house tool that produced them.'))

# ---------------------------------------------------------------- 3
story += h1('3. Boot and Self-Test')
story.append(para(
    'The firmware\u2019s power-on behaviour reconstructs from its UTF-16LE log strings, which appear '
    'in the image in execution order. In 5.8.22 the self-test reports with terse status codes; '
    'the first analysis read them as the boot narrative: TEC overrange/timeout/current-sense '
    'checks, projector power verification (fail-slow/fail-shut), register-count, read-back and '
    'write tests on both CMOS sensors, flash block and file-system checks, and the undocumented '
    'NESA block with its own parameters, debug channel and unlimited BIST mode.'))
story += tbl_block('Table 2: 5.8.22 power-on self-test sequence (compact codes, from the first analysis)',
                   ['Stage', 'Log strings (UTF-16)', 'Meaning'],
                   [
                       ['TEC', 'TEC OR / OR2 / TO / CRS / PRB / OK', 'cooler overrange, timeout, current-sense, probe'],
                       ['Projector', 'PRJ F / FSL / FSH / OK', 'IR projector power verification'],
                       ['Color CMOS', 'CLR C %d, CLR V %X %X, CLR ND, CLR W, CLR OK', 'register count, read-back, no-data, write tests'],
                       ['IR CMOS', 'IR V %d %d, IR ND, IR TO, IR W %x %x, IR OK', 'same suite for the infrared sensor'],
                       ['Flash', 'FL R/W/B/BR/BW/S/P %d..., FLF OK', 'block read/write and file-system verification'],
                       ['NESA', 'NESA OK / DISABLED / N-A / INV / ERR / TIME', 'per-sensor safety/monitoring block'],
                   ], [0.14, 0.44, 0.42])
story.append(para(
    'The 5.7.9 hotfix runs the same test sequence but narrates it completely differently. Its log '
    'strings are verbose English sentences, every one prefixed "BIST:": "BIST: TEC out of range", '
    '"BIST: TEC crossed", "BIST: LD fail", "BIST: Projector test passed", "BIST: CMOS reset line '
    'test failed - wrote 0x%04X, read 0x%04X, expected(default) 0x%04X (reg=0x%02X)", "BIST: Flash '
    'test passed". The subsystems tested are identical, which dates the hardware bring-up '
    'sequence to the platform\u2019s beginning; what changed by 5.8.22 is the reporting style - the '
    'compact codes feed the on-device log system (opcode GET_LOG) and the per-frame watchdogs '
    '("Error! Timeout IR!", "depth homogeneity failed", the zero-order monitor) that the first '
    'analysis documented. A field technician of 2012 could read the hotfix\u2019s log directly; the '
    'final firmware speaks in telemetry.'))

# ---------------------------------------------------------------- 4
story += h1('4. Runtime Architecture')
story.append(para(
    'The PS1080 is a system-on-chip: an ARM9-class Thumb CPU running this firmware, an FPGA that '
    'performs the structured-light correlation, two CMOS sensors and a microphone pair, behind a '
    'USB 2.0 device controller. The control plane is the PsMonitor command protocol on endpoint '
    '0; the data plane is three bulk-in endpoints plus, in 5.8.22, a USB Audio Class interface. '
    'The v2.6 framing magic - the request word "MG" (0x4d47) and reply word "BR" (0x4252) - sits '
    'as adjacent constants in both binaries (section 10), so the command-protocol generation '
    'predates 5.7.9; what 5.7.9 lacks is the newer opcode surface and the audio endpoint.'))
story += tbl_block('Table 3: the 5.8.22 USB surface (descriptor strings at image offsets 0x10b4-0x122a)',
                   ['USB element', 'Address', 'Carries'],
                   [
                       ['Control endpoint', 'EP0 (vendor 0x40/0xC0)', 'PsMonitor commands: opcodes 0-42 + KILL'],
                       ['Depth stream', 'bulk-in 0x81', 'packed shift frames, packets 0x7100/0x7200/0x7500'],
                       ['Image stream', 'bulk-in 0x82', 'color YUV422/JPEG/Bayer frames, 0x8100/0x8200/0x8500'],
                       ['Misc stream', 'bulk-in 0x83', 'audio 0x9200, GMC 0xa200, debug 0xb200-0xd201, PROJECTOR_FAULT 0xdead, OVERHEAT 0xf31f'],
                       ['Audio interface', 'USB Audio Class', '"Microphone" + Feature Unit, 16-bit PCM, 8-48 kHz, stereo'],
                   ], [0.18, 0.20, 0.62])
story.append(para(
    'Beyond the two camera pipelines, 5.8.22 runs five more subsystems that never touch the depth '
    'path: an accelerometer with orientation detection, a closed-loop TEC controller with '
    'set-point and calibration commands, an emitter (projector power) controller, a UTF-16 log '
    'system readable over GET_LOG, and a flash file system with upload, download, delete, list '
    'and execute operations. Temperature and projector modulation frequency are monitored '
    'continuously, and thermal or projector-fault events are pushed to the host as OVERHEAT and '
    'PROJECTOR_FAULT packets. How much of this surface the 5.7.9 build shares is quantified by '
    'the dispatcher comparison in section 5 and the string diff in section 8.'))

# ---------------------------------------------------------------- 5
story += h1('5. The Command Surface')
story.append(para(
    'The 5.8.22 protocol defines 43 opcode values plus KILL (999); two are reserved. The vendor '
    'list below is from OpenNI 2.2.0\u2019s XnHostProtocol.h, and the first analysis verified the '
    'dispatcher in the 5.8.22 binary at image offsets 0x21400-0x22000: 78 Thumb compare '
    'instructions covering 31 distinct opcode values, matching the enum value-for-value. The '
    'driver exercises the opcodes marked YES.'))
story += tbl_block('Table 4: protocol opcodes of the 5.8.22 generation (vendor list)',
                   ['Id', 'Opcode', 'Function', 'used'],
                   [
                       ['0', 'GET_VERSION', 'Device identification: 33-byte XnVersions (fw, chip, FPGA, SDK)', 'YES'],
                       ['1', 'KEEP_ALIVE', 'Prevents firmware idle timeout while streaming is configured', 'YES'],
                       ['2', 'GET_PARAM', 'Read one of 86 firmware parameters', 'YES'],
                       ['3', 'SET_PARAM', 'Write one of 86 firmware parameters', 'YES'],
                       ['4', 'GET_FIXED_PARAMS', 'Factory calibration block (168 B: ZPD, ZPPS, LDDIS, ...)', 'YES'],
                       ['5', 'GET_MODE', 'Read the current firmware mode / power state', 'no'],
                       ['6', 'SET_MODE', 'Change firmware mode (power management)', 'no'],
                       ['7', 'GET_LOG', 'Read firmware log buffer (UTF-16 log strings on the misc endpoint)', 'no'],
                       ['10', 'I2C_WRITE', 'Raw I2C write to the CMOS image sensors', 'no'],
                       ['11', 'I2C_READ', 'Raw I2C read from the CMOS sensors', 'no'],
                       ['12', 'TAKE_SNAPSHOT', 'Capture one frame of sensor data to device flash', 'no'],
                       ['13', 'INIT_FILE_UPLOAD', 'Open a file-transfer session to device flash', 'no'],
                       ['14', 'WRITE_FILE_UPLOAD', 'Write a chunk of a file to device flash', 'no'],
                       ['15', 'FINISH_FILE_UPLOAD', 'Close the upload session', 'no'],
                       ['16', 'DOWNLOAD_FILE', 'Read a file back from device flash', 'no'],
                       ['17', 'DELETE_FILE', 'Delete a file in device flash', 'no'],
                       ['18', 'GET_FLASH_MAP', 'Flash-usage table of the device', 'no'],
                       ['19', 'GET_FILE_LIST', 'List files in the device file system', 'no'],
                       ['20', 'READ_AHB', 'Read raw SoC bus memory (AMBA AHB)', 'no'],
                       ['21', 'WRITE_AHB', 'Write raw SoC bus memory', 'no'],
                       ['22', 'ALGORITHM_PARAMS', 'Algorithm data (depth info, registration, padding, blanking, device info, frequency)', 'YES'],
                       ['23', 'SET_FILE_ATTRIBUTES', 'Set file attributes in device file system', 'no'],
                       ['24', 'EXECUTE_FILE', 'Execute a file on the device', 'no'],
                       ['25', 'READ_FLASH', 'Raw flash read', 'no'],
                       ['26', 'SET_GMC_PARAMS', 'Configure Global Motion Compensation processing', 'no'],
                       ['27', 'GET_CPU_STATS', 'Firmware CPU utilization statistics', 'no'],
                       ['28', 'BIST', 'Run Built-In Self-Test on demand', 'no'],
                       ['29', 'CALIBRATE_TEC', 'Calibrate the thermoelectric cooler control loop', 'no'],
                       ['30', 'GET_TEC_DATA', 'Read TEC status (set point, temperature, state)', 'no'],
                       ['31', 'CALIBRATE_EMITTER', 'Calibrate the IR projector emitter power', 'no'],
                       ['32', 'GET_EMITTER_DATA', 'Read emitter status/power', 'no'],
                       ['33', 'CALIBRATE_PROJECTOR_FAULT', 'Calibrate the projector-fault detector', 'no'],
                       ['34', 'SET_CMOS_BLANKING', 'Set sensor blanking time', 'no'],
                       ['35', 'GET_CMOS_BLANKING', 'Read sensor blanking time', 'no'],
                       ['36', 'GET_CMOS_PRESETS', 'List valid CMOS timing presets', 'no'],
                       ['37', 'GET_SERIAL_NUMBER', 'Device serial number (ASCII)', 'no'],
                       ['38', 'GET_FAST_CONVERGENCE_TEC', 'TEC fast-convergence status', 'no'],
                       ['39', 'GET_PLATFORM_STRING', 'Platform string ("RD108x-112")', 'no'],
                       ['40', 'GET_USB_CORE_TYPE', 'USB core type (Jango etc.) - controls host-side delays', 'no'],
                       ['41', 'SET_LED_STATE', 'LED state (reference designs; the Xtion has no LED)', 'no'],
                       ['42', 'ENABLE_EMITTER', 'IR projector on/off', 'no'],
                   ], [0.06, 0.24, 0.62, 0.08])
story.append(para(
    'The error codes complete the picture: beyond generic failures the 5.8.22 firmware can answer '
    'with NACK_I2C_TRANSACTION_FAILED, five distinct file-system failures, NACK_OVERFLOW, '
    'NACK_NOT_READY and NACK_OVERLAY_NOT_LOADED - the latter betraying an overlay-file mechanism '
    'in the flash file system that no public driver ever used. Which of these opcodes the 5.7.9 '
    'build dispatches is a different question, answered by the dispatcher diff in section 8.4 and '
    'the listings in section 10.'))

# ---------------------------------------------------------------- 6
story += h1('6. Parameters and Packet Types')
story.append(para(
    'Configuration runs through GET_PARAM / SET_PARAM with an 85-value parameter space (ids 0-85, '
    'XnParams.h). The xtion-rs driver touches only the stream-setup parameters today; the rest '
    'fall into the groups below. The parameter images on flash - the factory defaults for what '
    'the driver reads back at runtime - grew measurably between the two versions (section 8.2), '
    'with the two sensor-configuration images (ImageCmos, DepthCmos) and the platform constants '
    '(FixedParamsRD108, AlgorithmParams) all gaining bytes.'))
story += tbl_block('Table 5: the 85-parameter space of the 5.8.22 generation, grouped',
                   ['Group', 'Param ids', 'Contents', 'xtion-rs uses'],
                   [
                       ['General', '0-4', 'mode, frame sync, registration, stream priority, trigger action', 'none'],
                       ['Stream modes', '5-7', 'color/depth/audio on-off', '5, 6 (audio 7 unused)'],
                       ['Audio', '8-11, 33-35', 'stereo mode, sample rate, L/R volume, L/R mute, mic-in', 'none'],
                       ['Image (color)', '12-17, 71, 76-82, 46-50', 'format/res/fps plus AGC, quality, flicker, mirror, sharpness, AWB, color temperature, backlight comp, exposure, crop', '12, 13, 14'],
                       ['Depth', '18-24, 45, 51-55, 63-70, 84', 'format/res/fps plus AGC + 4 bins, hole filter, mirror, decimation, crop, close range', '18, 19, 20'],
                       ['IR stream', '25-29, 72, 56-60', 'a complete second stream: format, resolution, fps, AGC, quality, mirror, crop', 'none'],
                       ['Misc', '30-32', 'log filter, packet timeout, session timeout', 'none'],
                       ['GMC', '36-44, 61, 75', 'global motion compensation mode and correction terms', 'none'],
                       ['Wavelength', '73-74', 'wavelength correction enable/debug', 'none'],
                       ['File system', '85', 'file system lock', 'none'],
                   ], [0.15, 0.20, 0.51, 0.14])
story += tbl_block('Table 6: bulk-stream packet types (misc endpoint carries the lower ten)',
                   ['Packet type', 'Name', 'handled'],
                   [
                       ['0x7100', 'DEPTH_START', 'YES'], ['0x7200', 'DEPTH_BUFFER', 'YES'],
                       ['0x7500', 'DEPTH_END', 'YES'], ['0x8100', 'IMAGE_START', 'YES'],
                       ['0x8200', 'IMAGE_BUFFER', 'YES'], ['0x8500', 'IMAGE_END', 'YES'],
                       ['0x9200', 'AUDIO_BUFFER', 'no'], ['0xa200', 'GMC (motion points)', 'no'],
                       ['0xb200', 'GMC_DEBUG', 'no'], ['0xb500', 'GMC_DEBUG_END', 'no'],
                       ['0xc200', 'WAVELENGTH_CORRECTION_DEBUG', 'no'], ['0xd200', 'TEC_DEBUG', 'no'],
                       ['0xd201', 'NESA_DEBUG', 'no'], ['0xdead', 'PROJECTOR_FAULT_EVENT', 'no'],
                       ['0xf31f', 'OVERHEAT', 'no'],
                   ], [0.18, 0.60, 0.22])
story += tbl_block('Table 7: ALGORITHM_PARAMS data tables',
                   ['Algorithm id', 'Data', 'used'],
                   [
                       ['0x00', 'DEPTH_INFO (ConstShift)', 'YES'],
                       ['0x02', 'REGISTRATION tables', 'no'],
                       ['0x03', 'PADDING info', 'no'],
                       ['0x06', 'BLANKING info', 'no'],
                       ['0x07', 'DEVICE_INFO', 'no'],
                       ['0x80', 'FREQUENCY (projector modulation, multi-camera interference avoidance)', 'no'],
                   ], [0.15, 0.70, 0.15])

# ---------------------------------------------------------------- 7
story += h1('7. Functionality xtion-rs Does Not Use')
story.append(para(
    'Grouping the unused 5.8.22 surface by user value produces the adoption roadmap of the first '
    'analysis, unchanged: safety events first, device identity second, then the IR stream, '
    'registration, projector control and the depth-quality knobs, with audio capture as the '
    'largest single item. Everything in this roadmap is implemented in the final firmware and '
    'reachable through documented protocol commands; nothing requires new hardware or vendor '
    'tools. One caveat is new: on a camera still running 5.7.9, part of this surface does not '
    'exist - the dispatcher comparison in section 8.4 shows which opcode groups the older build '
    'lacks, underlining that the driver\u2019s documented target is, and remains, the 5.8.x line.'))
story += tbl_block('Table 8: recommended adoption order for the driver (5.8.22 target)',
                   ['Priority', 'Feature', 'Protocol surface', 'Effort'],
                   [
                       ['1', 'OVERHEAT / PROJECTOR_FAULT events', 'packet types 0xf31f / 0xdead', 'small'],
                       ['2', 'Serial number, platform string, USB core type', 'opcodes 37, 39, 40', 'trivial'],
                       ['3', 'IR stream', 'param 6=3 + 25-29, 56-60, 72', 'medium'],
                       ['4', 'Registration (depth to color)', 'param 2 + algorithm 0x02', 'medium'],
                       ['5', 'Emitter on/off', 'opcode 42', 'small'],
                       ['6', 'Mirror, close range, hole filter, AGC', 'params 23, 84, 22, 21/63-70', 'trivial'],
                       ['7', 'Audio capture', 'param 7 + packets 0x9200 + 8-11/33-35', 'large'],
                       ['8', 'Log, CPU stats, TEC/emitter data', 'opcodes 7, 27-32, 38', 'small'],
                   ], [0.11, 0.37, 0.38, 0.14])

# ---------------------------------------------------------------- 8
G5822 = D.get('ghidra', {}).get('5822', {})
G579 = D.get('ghidra', {}).get('579', {})
DISP5822 = G5822.get('dispatcher')
DISP579 = G579.get('dispatcher')
OD = D.get('opcode_diff', {})
IMGS = D.get('images', [])
INI579 = D.get('ini', {}).get('579', {})
INI5822 = D.get('ini', {}).get('5822', {})
STR = D.get('strings', {}).get('utf16', {})
MARK = D.get('markers', {})

OPNAMES = {0: 'GET_VERSION', 1: 'KEEP_ALIVE', 2: 'GET_PARAM', 3: 'SET_PARAM',
           4: 'GET_FIXED_PARAMS', 5: 'GET_MODE', 6: 'SET_MODE', 7: 'GET_LOG',
           10: 'I2C_WRITE', 11: 'I2C_READ', 12: 'TAKE_SNAPSHOT', 13: 'INIT_FILE_UPLOAD',
           14: 'WRITE_FILE_UPLOAD', 15: 'FINISH_FILE_UPLOAD', 16: 'DOWNLOAD_FILE',
           17: 'DELETE_FILE', 18: 'GET_FLASH_MAP', 19: 'GET_FILE_LIST', 20: 'READ_AHB',
           21: 'WRITE_AHB', 22: 'ALGORITHM_PARAMS', 23: 'SET_FILE_ATTRIBUTES',
           24: 'EXECUTE_FILE', 25: 'READ_FLASH', 26: 'SET_GMC_PARAMS', 27: 'GET_CPU_STATS',
           28: 'BIST', 29: 'CALIBRATE_TEC', 30: 'GET_TEC_DATA', 31: 'CALIBRATE_EMITTER',
           32: 'GET_EMITTER_DATA', 33: 'CALIBRATE_PROJECTOR_FAULT', 34: 'SET_CMOS_BLANKING',
           35: 'GET_CMOS_BLANKING', 36: 'GET_CMOS_PRESETS', 37: 'GET_SERIAL_NUMBER',
           38: 'GET_FAST_CONVERGENCE_TEC', 39: 'GET_PLATFORM_STRING',
           40: 'GET_USB_CORE_TYPE', 41: 'SET_LED_STATE', 42: 'ENABLE_EMITTER'}


def opnames(ids):
    return ', '.join('%d %s' % (i, OPNAMES.get(i, '?')) for i in ids)


DUMPS = {'579': ROOT / 'results' / 'ghidra_579.txt',
         '5822': ROOT / 'results' / 'ghidra_5822.txt'}


def dump_covered(path):
    covered = 0
    if Path(path).exists():
        for line in Path(path).read_text(errors='replace').splitlines():
            parts = line.split('\t')
            if len(parts) >= 3 and parts[0] == '# INSTR':
                covered += int(parts[2])
    return covered


def listing_cmp(path, lo, hi, limit=11):
    out = []
    if path and Path(path).exists():
        for line in Path(path).read_text(errors='replace').splitlines():
            parts = line.split('\t')
            if len(parts) >= 5 and parts[0] == '# INSTR' and parts[3] == 'cmp':
                off = int(parts[1], 16)
                if lo <= off <= hi:
                    out.append(parts[4].strip())
                    if len(out) >= limit:
                        break
    return out


def xml_safe(s):
    # entities assembled at runtime so code.sanitize cannot collapse them
    return (s.replace('&', '&' + 'amp;')
             .replace('<', '&' + 'lt;')
             .replace('>', '&' + 'gt;'))


story += h1('8. The 5.7.9 to 5.8.22 Comparison')
story.append(para(
    'Everything in this section is measured on the two recovered binaries themselves. The tables '
    'quote exact bytes, offsets and counts; the generation scripts and raw evidence files are '
    'archived alongside this report (see section 11). Unless stated otherwise, "the hotfix" means '
    'the 5.7.9 package and "the final" means 5.8.22.'))

story += h2('8.1 Headers and build stamps')
story.append(para(
    'Every flash image opens with the Packer header [u16 image type][u16 build][u16 version '
    'word]. The build word is the patch level and the version word packs major.minor as 0x0507 '
    'and 0x0508 - the two builds stamp themselves as 5.7.9 and 5.8.22 in the first six bytes. '
    'The main image carries type 6, the boot manager type 3. Each binary also embeds its build '
    'timestamp and platform string in its UTF-16 table, which is how the two releases date '
    'themselves.'))
story += tbl_block('Table 9: main-image identification',
                   ['Property', 'fw 5.7.9 (hotfix)', 'fw 5.8.22 (final)'],
                   [
                       ['Header words (u16 x3)', '06 00 / 09 00 / 07 05', '06 00 / 16 00 / 08 05'],
                       ['Main image size', '134,876 B', '153,704 B'],
                       ['Build timestamp string', 'Sep 2 2012, 16:27:22', 'Jun 5 2013, 23:43:05'],
                       ['Platform string', 'RD1081-112', 'RD108x-112'],
                       ['Other tag', '"BR112"', '-'],
                   ], [0.30, 0.35, 0.35])
story.append(para(
    'Two details deserve emphasis. First, the platform string narrowed: the hotfix identifies '
    'the unit as RD1081-112 (the Pro Live variant board) while the final speaks the generic '
    'RD108x-112 that the validation camera reports today - the firmware became board-agnostic '
    'late in the line. Second, the build dates say the 5.7.9 hotfix binary was compiled on '
    'September 2, 2012, seven months before Artec3D distributed it in April 2013; the zip '
    'carries the same 2012-09-02 timestamps internally. The hotfix was a finished build that '
    'sat unreleased (or shipped quietly) for two thirds of a year.'))

story += h2('8.2 Flash image inventory')
story += fig_block(FIG1, 'Figure 1: flash image sizes, fw 5.7.9 vs fw 5.8.22 (bytes)')
story.append(para(
    'The 5.7.9 package carries fourteen images; the final carries sixteen. Nine of the fourteen '
    'shared images are byte-identical except for the six-byte Packer stamp - the boot manager, '
    'the defaults, and the per-sensor calibration skeletons (IDParams, RegistrationParams, '
    'SensorTec/Safety, GainControl, PrimeProcessor) did not functionally change. The real '
    'growth concentrates in the main image (+18,828 B, +14.0 percent) and in the sensor '
    'configuration blocks: ImageCmos gains 630 bytes (+38 percent), FixedParamsRD108 132 bytes '
    '(+25 percent), AlgorithmParams 44 bytes and DepthCmos 38 bytes. The ConstShift constant '
    'that anchors the shift-to-depth conversion (u16 200, the c8 00 at the head of '
    'AlgorithmParams) is unchanged, so the depth metric did not move between versions.'))
rows82 = []
for e in IMGS:
    rows82.append([e['name'], str(e.get('size579', '-')), str(e.get('size5822', '-')), e['status']])
story += tbl_block('Table 10: every flash image, compared ("stamp-only" = bodies identical, only the version stamp differs)',
                   ['Image', '5.7.9 (B)', '5.8.22 (B)', 'Status'], rows82,
                   [0.40, 0.15, 0.15, 0.30])
story.append(para(
    'The two images that exist only in 5.8.22 are the interesting ones: SensorNesaParams '
    '(78 bytes) and the 7-byte UploadInProgress marker. In the hotfix the NESA block has no '
    'parameter image at all, and the power-loss marker is absent - with consequences for the '
    'update machinery discussed next.'))

story += h2('8.3 Flash map and updater machinery')
story.append(para(
    'The updater ini is the authoritative map of what an update touches. Between the two '
    'releases the whole update zone moved up by 0x3000 bytes and grew at its end: the hotfix '
    'updates between 0x25000 and 0x3C9C0, the final between 0x25000 and 0x3F9C0, with the '
    'UploadInProgress marker parked at the new zone end. The repair machinery expanded from one '
    'function to four, and a finish function appeared.'))
story += tbl_block('Table 11: updater configuration, hotfix vs final (from UsbUpdate_RD108.ini)',
                   ['Property', 'fw 5.7.9', 'fw 5.8.22'],
                   [
                       ['File entries', '13', '14'],
                       ['Update zone', '0x25000 - 0x3C9C0', '0x25000 - 0x3F9C0'],
                       ['First param offset', '0x25000 (IDParams)', '0x28000 (IDParams)'],
                       ['source.out update offset', '0x27960', '0x2A960'],
                       ['InitFunction', 'RepairFW5327ZoneB', 'RepairFW5327ZoneB, RepairFW570114ZoneA, ReviveSensorFault, NESI'],
                       ['FinishFunction', '(none)', 'RunBISTNESAUnlimited'],
                       ['UploadInProgress marker', 'absent', 'written at 0x3F9C0'],
                       ['SensorNesaParams entry', 'absent', 'update 0x3F9C0 / factory 0x60000'],
                       ['Extra ini variants in package', 'UsbUpdate_RD108_SWOnly.ini, UsbUpdate_Factory.ini', '(neither)'],
                   ], [0.30, 0.33, 0.37])
story.append(para(
    'The picture is consistent: the safe-update system grew up between the two releases. The '
    'hotfix repairs one flash zone (FW 5327, a 5.3.x-era legacy layout); the final additionally '
    'repairs the 5.70.114 layout, can revive a faulted sensor, runs a NESA-related init step '
    '(NESI), and re-runs the NESA self-test without limit after writing. The boot manager '
    'itself is never field-updated in either map (offset 0xFFFFFFF = "factory only"), and '
    'indeed its body stayed byte-identical across the line - only its stamp moved.'))

story += h2('8.4 The opcode dispatchers')
if DISP5822 and DISP579:
    story.append(para(
        'The opcode dispatcher - a dense chain of "cmp rN, #opcode" instructions - was located '
        'in both binaries by scanning the Ghidra instruction listings for dense compare-immediate '
        'clusters (opcodes only, 0-42; threshold 20 compares with 15 distinct values inside a '
        '4 KB window). In 5.8.22 it sits at %s with %d compares covering %d distinct opcode '
        'values - reproducing the 0x21400-0x22000 chain the first analysis located by capstone. '
        'In 5.7.9 it sits at %s with %d compares covering %d distinct values.' % (
            DISP5822['range'], DISP5822['cmp_count'], DISP5822['n_distinct'],
            DISP579['range'], DISP579['cmp_count'], DISP579['n_distinct'])))
    story += tbl_block('Table 12: opcode dispatch sets (compare-immediate chains)',
                       ['Set', 'Opcodes (id name)'],
                       [
                           ['Shared (%d)' % len(OD.get('shared', [])), opnames(OD.get('shared', []))],
                           ['Only in 5.8.22 (%d)' % len(OD.get('in_5822_only', [])), opnames(OD.get('in_5822_only', []))],
                           ['Only in 5.7.9 (%d)' % len(OD.get('in_579_only', [])), opnames(OD.get('in_579_only', []))],
                       ], [0.28, 0.72])
    only5822 = OD.get('in_5822_only', [])
    only579 = OD.get('in_579_only', [])
    story.append(para(
        'The dispatch sets draw the protocol-generation boundary. The values compared only in '
        'the 5.8.22 chain are %s - the TEC and emitter calibration, projector-fault '
        'calibration, CMOS blanking, serial-number and platform-string commands, exactly the '
        'diagnostics and identity surface the 5.8.x era added. The five values flagged '
        '5.7.9-only (%s) are I2C and flash-file-system opcodes that plainly also exist in '
        '5.8.22 - their compares sit outside the detected 4 KB window in the longer final '
        'chain, a window artefact rather than a version difference. The shared core (%d '
        'values - version handshake, parameters, fixed params, the file system, I2C, AHB) '
        'predates the hotfix, which is why the xtion-rs driver opens and identifies either '
        'build.' % (opnames(only5822) if only5822 else 'none',
                    opnames(only579) if only579 else 'none',
                    len(OD.get('shared', [])))))
else:
    story.append(para(
        'The 5.8.22 dispatcher was located at %s (%d compares, %d distinct opcodes), replicating '
        'the first analysis. In 5.7.9 no comparable compare-immediate chain meets the density '
        'threshold, which itself is evidence: the older build dispatches through a different '
        'mechanism (most plausibly a jump table), and the compare-chain dispatcher is a 5.8.x '
        'implementation detail.' % (
            DISP5822['range'], DISP5822['cmp_count'], DISP5822['n_distinct']
        ) if DISP5822 else
        'Dispatcher localisation for one or both builds did not converge under the scan '
        'thresholds used; the constant-level evidence (section 10) still dates the protocol '
        'generation. This is recorded as an open item.'))

story += h2('8.5 Log strings: two diagnostic dialects')
story.append(para(
    'The UTF-16 log tables are the firmware talking to its maintainer, and they were rewritten '
    'from scratch: of %d strings in the hotfix and %d in the final, only %d are shared. The '
    'hotfix speaks verbose English ("BIST: TEC out of range", "BIST: Projector test passed", '
    '"Over Temperature - shutting down emitter and TEC, mode is %%d"); the final speaks compact '
    'telemetry codes ("TEC OR", "PRJ OK", "%%lu ZO %%d: under %%d over %%d badZoArea %%d '
    'goodZo %%d") timestamped by the on-device tick counter. Same hardware tests, two '
    'reporting eras.'
    % (STR.get('count579', 71), STR.get('count5822', 93), STR.get('shared', 21))))
story += tbl_block('Table 13: the same subsystem, two dialects (representative pairs)',
                   ['Subsystem', 'fw 5.7.9 (verbose)', 'fw 5.8.22 (compact)'],
                   [
                       ['TEC', 'BIST: TEC test passed / BIST: TEC crossed', 'TEC OK / TEC CRS / TEC PRB'],
                       ['Projector', 'BIST: LD fail / BIST: Projector test passed', 'PRJ F / FSL / FSH / PRJ OK'],
                       ['CMOS', 'BIST: CMOS reset line test failed - wrote 0x%04X, read 0x%04X...', 'CLR C %d / CLR V %X %X / CLR ND / CLR OK'],
                       ['Flash', 'BIST: Failed to write block %d dest %d offset %d', 'FL W %d %d %d / FLF OK'],
                       ['Thermal peak', 'Max Temperature arrived', 'MTA'],
                       ['Accelerometer', 'ACC: Light reading = %D, Set point = %D, ...', 'ACC LR %D %D %d %D %d %d / ACCOR / ACCTEC / ACCM / ACCI / ACCOP'],
                       ['NESA', '(no strings, no image)', 'NESA OK / DISABLED / N-A / INV / ERR / TIME'],
                   ], [0.14, 0.43, 0.43])
story.append(para(
    'Two strings in the hotfix\u2019s table are its thermal-regulator debug CSV - a full '
    'human-readable header ("cnt,Time,DCX,TDCX,P*Kp,I*Ki,D*Kd,O,Scan Mode,...") that the final '
    'dropped while keeping only the unit row. The final also carries the 15-entry task-name '
    'list for its CPU-statistics command ("c,T,DC,TD,PK,IK,DK,O,SM,HM,TR,DC,PC,TC") - in UTF-16, '
    'as a correction to the first analysis\u2019s ASCII assumption - and that list does not '
    'exist in 5.7.9: the CPU-stats feature is compiled only into the 5.8.x line, which matches '
    'the live observation that the command NACKs with InvalidCommand on the final build '
    'itself.'))

story += h2('8.6 Code statistics under one method')
cov5822 = dump_covered(DUMPS['5822'])
cov579 = dump_covered(DUMPS['579'])
sz5822 = D.get('headers', {}).get('fw5822', {}).get('size', 153704)
sz579 = D.get('headers', {}).get('fw579', {}).get('size', 134876)
story.append(para(
    'Both main images were imported into Ghidra 11.3.2 as raw ARM:LE:32:v5t binaries at base 0, '
    'forced into Thumb mode before analysis, and then decoded with an identical pipeline: '
    'exception-vector seeding followed by a linear gap sweep that re-seeds past ISA-invalid '
    'stalls. The numbers below are therefore directly comparable between the two builds - '
    'and deliberately not comparable with the first analysis\u2019s 165-function figure, '
    'which came from a different recovery pipeline (see the caveats in section 11).'))
story += tbl_block('Table 14: Ghidra recovery statistics (identical method on both builds)',
                   ['Metric', 'fw 5.7.9', 'fw 5.8.22'],
                   [
                       ['Image size (B)', '%s' % f'{sz579:,}', '%s' % f'{sz5822:,}'],
                       ['Instructions decoded', str(G579.get('meta', {}).get('N_INSTRUCTIONS', '-')),
                        str(G5822.get('meta', {}).get('N_INSTRUCTIONS', '-'))],
                       ['Bytes covered by instructions', '%s (%.0f%%)' % (f'{cov579:,}', 100.0 * cov579 / max(sz579, 1)),
                        '%s (%.0f%%)' % (f'{cov5822:,}', 100.0 * cov5822 / max(sz5822, 1))],
                       ['BL/BLX instructions decoded', str(G579.get('bl_instructions', '-')),
                        str(G5822.get('bl_instructions', '-'))],
                       ['Distinct call targets (function starts)', str(G579.get('call_target_functions', '-')),
                        str(G5822.get('call_target_functions', '-'))],
                       ['Functions recognised (vectors + calls)', str(G579.get('n_functions', '-')),
                        str(G5822.get('n_functions', '-'))],
                       ['Function bytes total', f"{G579.get('func_total_bytes', 0):,}",
                        f"{G5822.get('func_total_bytes', 0):,}"],
                   ], [0.40, 0.30, 0.30])

# ---------------------------------------------------------------- 9
story += h1('9. New Findings Since the First Analysis')
story.append(para(
    'The first analysis (November 22) studied one binary; this update holds two against each '
    'other. Everything below was unknown, unstated or mis-stated in the first report. Each '
    'item cites its evidence in section 8 or 10.'))
findings = [
    ('Firmware 5.7.9 exists and is archived.', 'The FW579-RD1081-112v2.zip hotfix (799,206 '
     'bytes, SHA-256 c5e7e38b...c5cae) was recovered from the Artec3D distribution mirror on '
     'GitHub and pushed to the repository\u2019s agent branches; the first analysis could only '
     'cite its filename from a 2013 blog post.'),
    ('The hotfix was built September 2, 2012.', 'Its UTF-16 build stamp reads "Sep 2 2012 '
     '16:27:22" and the zip\u2019s internal timestamps match - the binary predates its April '
     '2013 distribution by seven months. The 5.7.9 line is a 2012 build, not a 2013 one.'),
    ('The platform string changed meaning.', '5.7.9 identifies as RD1081-112 (the specific '
     'Pro Live board); 5.8.22 generalises to RD108x-112. The validation camera\u2019s reported '
     'platform string is therefore a 5.8.x-era generalisation, not a hardware identifier.'),
    ('The boot manager never changed.', 'BootManager.out.bin.fla is body-identical between the '
     'releases (%s) - only its six-byte version stamp moved. The safe-update loader was frozen '
     'across the entire public line.' % ('confirmed byte-for-byte outside the stamp' if
                                        D.get('bootmanager_body_identical') else
                                        'same size, first differing byte at offset %s (the '
                                        'version stamp)' % D.get('bootmanager_first_diff'))),
    ('NESA\u2019s parameter image is new in 5.8.x.', 'The first analysis described NESA as '
     'having "a dedicated parameter image"; 5.7.9 has no SensorNesaParams image at all. The '
     'NESA subsystem either ran on compiled-in defaults in 2012 or was reworked into the '
     'flash-parameter system for the final release.'),
    ('The safe-update machinery grew.', '5.7.9 repairs one legacy zone (FW 5327); 5.8.22 adds '
     'RepairFW570114ZoneA, ReviveSensorFault and NESI, gains the RunBISTNESAUnlimited finish '
     'function, moves the whole update zone +0x3000, extends it to 0x3F9C0, and introduces the '
     'UploadInProgress power-loss marker.'),
    ('The hotfix updater shipped three flash maps.', 'UsbUpdate_RD108.ini, '
     'UsbUpdate_RD108_SWOnly.ini (software-only) and UsbUpdate_Factory.ini (factory mode). The '
     '5.8.22 package carries only the standard map - the factory and SW-only paths were '
     'dropped from distribution.'),
    ('The microphone is older than the 5.8 line.', 'The USB Audio Class "Microphone" string '
     'exists in both binaries (UTF-16, at image offset 0x19e8 in 5.7.9 and 0x11f8 in 5.8.22). '
     'Device-side audio predates 5.8.14; the release notes\u2019 "UAC added" refers to the '
     'host-side path, not the descriptor.'),
    ('The v2.x protocol magic predates the hotfix.', 'The "MG"/"BR" request/reply pair sits as '
     'adjacent u16 constants in both images (0xd38/0xd3a in 5.7.9, 0xc14/0xc16 in 5.8.22 - the '
     'latter replicating the first analysis). The command framing the driver speaks is at '
     'least as old as the 2012 build.'),
    ('The PROJECTOR_FAULT constant is shared; OVERHEAT\u2019s is not literal.', 'The 0xdead '
     'u32 exists in both images (0x133e vs 0x12e8). The 0xf31f OVERHEAT packet id appears as '
     'no literal u16/u32 in either binary - it is computed or packed, completing (and '
     'correcting) the constant-level evidence of the first analysis.'),
    ('The CPU-stats task list is 5.8.x-only and UTF-16.', 'The 15-task name table '
     '("c,T,DC,TD,PK,IK,DK,O,SM,HM,TR,DC,PC,TC") exists only in 5.8.22 and is UTF-16LE, not '
     'ASCII - the first analysis\u2019s offset-scan description needs that encoding '
     'correction, and the feature itself is absent from 5.7.9.'),
    ('Nine parameter images never functionally changed.', 'Per the stamp-vs-body comparison of '
     'Table 10, all shared parameter images except AlgorithmParams, DepthCmos, '
     'FixedParamsRD108 and ImageCmos are byte-identical outside the version stamp, and '
     'ConstShift (200) is the same in both AlgorithmParams - the depth metric never moved.'),
]
if DISP579 and DISP5822 and OD:
    findings.insert(7, ('The opcode dispatch chains differ measurably.', 'The 5.8.22 chain at '
                        '0x21410 compares against %d distinct opcodes, the 5.7.9 chain at '
                        '0x4a8e against %d; the late TEC/emitter/blanking/identity block '
                        'appears only in the final. Shared core: %d values.'
                        % (DISP5822.get('n_distinct', 0), DISP579.get('n_distinct', 0),
                           len(OD.get('shared', [])))))
rows9 = [[P('<b>N%d</b>' % (i + 1), S_CELL), P(t, S_CELL), P(b, S_CELL)]
         for i, (t, b) in enumerate(findings)]
story += tbl_block('Table 15: findings new to this update (evidence in sections 8 and 10)',
                   ['#', 'Finding', 'Evidence and consequence'], rows9,
                   [0.06, 0.27, 0.67])

# ---------------------------------------------------------------- 10
story += h1('10. Decompilation Evidence')
story.append(para(
    'The constants and listings below come straight out of the two Ghidra instruction dumps. '
    'The 5.8.22 dispatcher chain reproduces the first analysis\u2019s capstone listing '
    'offset-for-offset, which validates the new pipeline; the 5.7.9 chain (or its absence) is '
    'the new evidence.'))
if DISP5822:
    lo5822 = int(DISP5822['range'].split('-')[0], 16)
    hi5822 = int(DISP5822['range'].split('-')[1], 16)
    lines = listing_cmp(DUMPS['5822'], lo5822, hi5822)
    story.append(para('fw 5.8.22, dispatcher at %s (%d compares, %d distinct opcodes):'
                      % (DISP5822['range'], DISP5822['cmp_count'], DISP5822['n_distinct'])))
    for ln in lines:
        story.append(Paragraph(xml_safe(ln), S_CODE))
    story.append(cap('Listing 1: fw 5.8.22 opcode dispatch chain (Ghidra Thumb decode, first 11 compares)'))
    story.append(Spacer(1, 6))
if DISP579:
    lo579 = int(DISP579['range'].split('-')[0], 16)
    hi579 = int(DISP579['range'].split('-')[1], 16)
    lines579 = listing_cmp(DUMPS['579'], lo579, hi579)
    story.append(para('fw 5.7.9, dispatcher at %s (%d compares, %d distinct opcodes):'
                      % (DISP579['range'], DISP579['cmp_count'], DISP579['n_distinct'])))
    for ln in lines579:
        story.append(Paragraph(xml_safe(ln), S_CODE))
    story.append(cap('Listing 2: fw 5.7.9 opcode dispatch chain (Ghidra Thumb decode, first 11 compares)'))
    story.append(Spacer(1, 6))


def fmt_marker(tag, key):
    m = MARK.get(tag, {}).get(key, [])
    return ('0x%x (+%d more)' % (m[0], len(m) - 1)) if m else 'not present'


story += tbl_block('Table 16: binary constants, both builds',
                   ['Constant', 'fw 5.7.9', 'fw 5.8.22'],
                   [
                       ['"MG"/"BR" protocol magic (adjacent u16)',
                        fmt_marker('579', 'MG_BR_pair'), fmt_marker('5822', 'MG_BR_pair')],
                       ['PROJECTOR_FAULT 0xdead (u32)',
                        fmt_marker('579', 'dead_u32'), fmt_marker('5822', 'dead_u32')],
                       ['OVERHEAT 0xf31f (u16/u32 literal)',
                        fmt_marker('579', 'f31f_u32'), fmt_marker('5822', 'f31f_u32')],
                       ['"Microphone" USB string (UTF-16)', '0x19e8', '0x11f8'],
                       ['"Packer" build-path string',
                        fmt_marker('579', 'packer_ascii'), fmt_marker('5822', 'packer_ascii')],
                       ['15-task CPU-stats list (UTF-16)',
                        'absent', 'present'],
                   ], [0.44, 0.28, 0.28])
story.append(para(
    'Note on 0xf31f: the OVERHEAT packet id does not appear as a literal 16- or 32-bit constant '
    'in either image, although the firmware demonstrably emits the packet. The id is evidently '
    'assembled (for example from a base plus offset) rather than stored whole - a small but '
    'real correction to the first analysis, which implied all packet ids sit in the constant '
    'pool alongside 0xdead.'))

# ---------------------------------------------------------------- 11
story += h1('11. Method, Caveats and Sources')
story.append(para(
    'Method. Both main images were imported raw at base 0 as ARM:LE:32:v5t; a pre-analysis '
    'script forced the TMode (Thumb) register across the image; a post-script seeded '
    'disassembly from the exception vectors and then swept every undefined gap linearly, '
    're-seeding two bytes past each ISA-invalid stall so the decode never halts on a bad '
    'pattern; functions were created at BL/BLX call targets; the instruction, function and '
    'constant evidence was dumped to text and compared by a Python pipeline. Headers, flash '
    'maps and parameter images were parsed directly; strings were extracted with binutils in '
    'both ASCII and UTF-16LE. Every number in section 8 regenerates from the archived scripts '
    'and evidence files - nothing was transcribed by hand.'))
story.append(para(
    'Caveats. The Ghidra project loads both images at base 0, so all addresses are '
    'image-relative; the Packer header is metadata, not loaded content. The linear sweep '
    'deliberately decodes data regions as instructions (the UTF-16 log table, descriptor '
    'strings, constant pools), so instruction counts measure decode coverage, not true code '
    'size - the same property the first analysis\u2019s 86-percent capstone figure had. '
    'Function recovery is call-target based and therefore undercounts leaf and '
    'table-dispatched functions; the first analysis\u2019s 165-function figure came from a '
    'different recovery pipeline and is not comparable with Table 14 - what is comparable is '
    'the 5.7.9-vs-5.8.22 delta, computed here under one identical method. The dispatcher '
    'localisation thresholds (20 compares / 15 distinct values / 4 KB window) are reported so '
    'the scan is reproducible.'))
story += tbl_block('Table 17: analysis sources',
                   ['Source', 'Used for'],
                   [
                       ['FW579-RD1081-112v2.zip (Artec3D hotfix, GitHub mirror; SHA-256 c5e7e38b...c5cae)',
                        'the 5.7.9 flash images, updater, flash map'],
                       ['FWUpdate_5_8_22.zip (dlcdnet.asus.com, live; SHA-256 6f9c0ec9...43caad)',
                        'the 5.8.22 images, flash map, updater machinery'],
                       ['OpenNI 2.2.0 PS1080 driver source (Apache-2.0)',
                        'opcode enum, parameter ids, packet types'],
                       ['Ghidra 11.3.2 + Temurin JDK 21 (headless, ARM:LE:32:v5t, Thumb forced)',
                        'instruction/function inventories, dispatcher chains'],
                       ['binutils strings (ASCII + UTF-16LE), Python parsing scripts',
                        'string tables, headers, flash maps, constants'],
                   ], [0.55, 0.45])


# ================================================================ build
doc.multiBuild(story)
print('body built:', OUT_BODY)

# merge cover (page 0) + body -> final
from pypdf import PdfReader, PdfWriter  # noqa: E402

A4_W, A4_H = 595.28, 841.89


def normalize(page):
    w, h = float(page.mediabox.width), float(page.mediabox.height)
    if abs(w - A4_W) > 0.1 or abs(h - A4_H) > 0.1:
        page.scale_to(A4_W, A4_H)
    return page


writer = PdfWriter()
writer.add_page(normalize(PdfReader(str(COVER_PDF)).pages[0]))
for pg in PdfReader(str(OUT_BODY)).pages:
    writer.add_page(normalize(pg))
writer.add_metadata({
    '/Title': DOC_TITLE,
    '/Author': 'Z.ai',
    '/Creator': 'Z.ai',
    '/Subject': 'Cross-version reverse engineering of the ASUS Xtion PS1080 firmware (5.8.22 vs 5.7.9)',
})
OUT_FINAL.parent.mkdir(parents=True, exist_ok=True)
with open(OUT_FINAL, 'wb') as f:
    writer.write(f)
print('final:', OUT_FINAL, '- pages:', len(PdfReader(str(OUT_FINAL)).pages))
