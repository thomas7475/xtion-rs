#!/usr/bin/env python3
"""Figure 1 for the updated firmware analysis PDF: flash image sizes,
5.7.9 vs 5.8.22, horizontal grouped bars (charts.md compliant: no top/right
spines, no grid where values are labeled, no legend box)."""
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

ACCENT = '#87702a'   # 5.8.22 series (palette XS tier)
ACCENT2 = '#3a95b4'  # 5.7.9 series (palette accent-secondary)
MUTED = '#78766f'
TEXT = '#1c1c1a'

images = [
    ('source.out.bin.fla (main fw)', 134876, 153704),
    ('BootManager.out.bin.fla', 23214, 23214),
    ('ImageCmos.xml.bin.fla', 1648, 2278),
    ('PrimeProcessor.xml.bin.fla', 1116, 1116),
    ('AlgorithmParams.xml.bin.fla', 736, 780),
    ('FixedParamsRD108.xml.bin.fla', 526, 658),
    ('DepthCmos.xml.bin.fla', 240, 278),
]
names = [i[0] for i in images][::-1]
v579 = [i[1] for i in images][::-1]
v5822 = [i[2] for i in images][::-1]

y = np.arange(len(names))
h = 0.38
fig, ax = plt.subplots(figsize=(7.4, 3.6), dpi=200, constrained_layout=True)
b1 = ax.barh(y + h / 2, v579, h, color=ACCENT2, label='fw 5.7.9', edgecolor='none')
b2 = ax.barh(y - h / 2, v5822, h, color=ACCENT, label='fw 5.8.22', edgecolor='none')
ax.set_yticks(y)
ax.set_yticklabels(names, fontsize=9, color=TEXT)
ax.set_xlabel('bytes', fontsize=9, color=MUTED)
for spine in ('top', 'right', 'left'):
    ax.spines[spine].set_visible(False)
ax.spines['bottom'].set_color('#cfcab8')
ax.tick_params(colors=MUTED, labelsize=8)
ax.grid(False)
for bars in (b1, b2):
    for rect in bars:
        wv = rect.get_width()
        ax.text(wv + 2500, rect.get_y() + rect.get_height() / 2, f'{wv:,}',
                va='center', fontsize=7.5, color=TEXT)
ax.set_xlim(0, 190000)
ax.legend(loc='lower right', frameon=False, fontsize=9)
fig.savefig('/home/z/my-project/work/firmware-analysis/fig1_sizes.png')
print('chart saved: fig1_sizes.png')
