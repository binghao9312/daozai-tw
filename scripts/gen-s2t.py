#!/usr/bin/env python3
"""由 OpenCC 詞庫產生歌詞簡轉繁用的資料檔（assets/s2t/）。

做法與 OpenCC 的 `s2tw` 設定相同，分兩個階段：
  1. 先以詞組（STPhrases）、再以單字（STCharacters）做最長匹配，簡體轉繁體；
  2. 再把臺灣用字（TWVariants，例如 着→著、裏→裡）套在整段輸出上。
不做「軟件→軟體」之類的用語轉換，只轉字形。

資料來源：opencc-python-reimplemented（Apache-2.0，詞庫出自 OpenCC，同為 Apache-2.0）。
    pip install opencc-python-reimplemented
    python scripts/gen-s2t.py
"""
import os
import sys

import opencc

DICT = os.path.join(os.path.dirname(opencc.__file__), 'dictionary')
OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'assets', 's2t')


def load(name):
    """詞庫每行「來源<TAB>候選1 候選2…」，取第一個候選（OpenCC 也是取第一個）。"""
    table = {}
    with open(os.path.join(DICT, name), encoding='utf-8') as file:
        for line in file:
            line = line.rstrip('\r\n')
            if not line or line.startswith('#') or '\t' not in line:
                continue
            key, values = line.split('\t', 1)
            value = values.split(' ')[0]
            if key and value:
                table[key] = value
    return table


characters = {key: value for key, value in load('STCharacters.txt').items() if key != value}
variants = {key: value for key, value in load('TWVariants.txt').items() if key != value}

# 詞組全部保留，包括「結果和逐字轉換相同」的：它們會擋住別的詞組亂配對（例如「想念」不能被
# 「念的→唸的」之類的詞組吃掉）。實測精簡掉這類詞組只省 2%，卻讓真實歌詞裡的錯字變多，不值得。
phrases = {key: value for key, value in load('STPhrases.txt').items() if len(key) > 1}

os.makedirs(OUT, exist_ok=True)
for name, table in (('characters.tsv', characters), ('phrases.tsv', phrases), ('variants.tsv', variants)):
    path = os.path.join(OUT, name)
    with open(path, 'w', encoding='utf-8', newline='\n') as file:
        for key in sorted(table):
            file.write(f'{key}\t{table[key]}\n')
    print(f'{path}: {len(table):,} 條, {os.path.getsize(path):,} 位元組', file=sys.stderr)
